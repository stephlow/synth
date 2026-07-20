//! The gRPC server side: turns `synth-proto` requests into `Command`s sent
//! to the audio thread, and turns `EngineMeters` into a `StreamState`
//! stream. Keeps everything real-time-unsafe (allocation, locking, async)
//! strictly off the audio thread — see `Command`/`EngineMeters` in the
//! `synth` crate for the wait-free handoff this relies on.

use std::pin::Pin;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use synth::modules::{Attenuverter, Envelope, Lfo, Vca, Vcf, Vco};
use synth::{Command, EngineMeters, Synth};
use synth_proto::synth_server::{Synth as SynthRpc, SynthServer};
use synth_proto::{
    CalibrationRequest, ControlDescriptor, ControlList, Empty, EngineState, NoteOffRequest,
    NoteOnRequest, SetParamRequest,
};
use tokio::sync::mpsc as tokio_mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, info};

/// Builds the fixed control descriptor table this engine exposes over
/// gRPC. Mirrors the actual patch graph (`patch::build_default_patch`) one
/// panel per module — VCO, VCF, VCA, both envelopes, the LFO, and the
/// attenuverter it modulates the filter through — not a curated CC list, so
/// a `console` TUI renders the real module rack rather than an invented
/// subset of "performance knobs". Module indices come from `probe.modules`
/// (`DefaultPatchModules`), which are data-independent (same every time a
/// default patch is built), so a throwaway `Synth` is enough to read them
/// off.
pub fn build_control_descriptors(probe: &Synth) -> Vec<ControlDescriptor> {
    let m = &probe.modules;

    fn envelope_controls(module: u32, module_name: &str, defaults: [f32; 4]) -> [ControlDescriptor; 4] {
        [
            ControlDescriptor {
                name: "Attack".into(),
                module,
                param: Envelope::PARAM_ATTACK as u32,
                min: 0.001,
                max: 2.0,
                default_value: defaults[0],
                module_name: module_name.into(),
                module_kind: "envelope".into(),
            },
            ControlDescriptor {
                name: "Decay".into(),
                module,
                param: Envelope::PARAM_DECAY as u32,
                min: 0.001,
                max: 2.0,
                default_value: defaults[1],
                module_name: module_name.into(),
                module_kind: "envelope".into(),
            },
            ControlDescriptor {
                name: "Sustain".into(),
                module,
                param: Envelope::PARAM_SUSTAIN as u32,
                min: 0.0,
                max: 1.0,
                default_value: defaults[2],
                module_name: module_name.into(),
                module_kind: "envelope".into(),
            },
            ControlDescriptor {
                name: "Release".into(),
                module,
                param: Envelope::PARAM_RELEASE as u32,
                min: 0.01,
                max: 5.0,
                default_value: defaults[3],
                module_name: module_name.into(),
                module_kind: "envelope".into(),
            },
        ]
    }

    let mut controls = vec![
        ControlDescriptor {
            name: "Tune".into(),
            module: m.vco as u32,
            param: Vco::PARAM_TUNE as u32,
            min: -2.0,
            max: 2.0,
            default_value: 0.0,
            module_name: "VCO".into(),
            module_kind: "vco".into(),
        },
        ControlDescriptor {
            name: "Cutoff".into(),
            module: m.vcf as u32,
            param: Vcf::PARAM_CUTOFF as u32,
            min: 20.0,
            max: 10_000.0,
            default_value: 1200.0,
            module_name: "VCF".into(),
            module_kind: "vcf".into(),
        },
        ControlDescriptor {
            name: "Resonance".into(),
            module: m.vcf as u32,
            param: Vcf::PARAM_RESONANCE as u32,
            min: 0.0,
            max: 1.0,
            default_value: 0.15,
            module_name: "VCF".into(),
            module_kind: "vcf".into(),
        },
    ];

    controls.extend(envelope_controls(m.filter_env as u32, "Envelope", [0.05, 0.2, 0.3, 0.4]));
    controls.extend(envelope_controls(m.amp_env as u32, "Envelope", [0.01, 0.1, 0.7, 0.3]));

    controls.push(ControlDescriptor {
        name: "Rate".into(),
        module: m.lfo as u32,
        param: Lfo::PARAM_RATE as u32,
        min: 0.1,
        max: 20.0,
        default_value: 2.0,
        module_name: "LFO".into(),
        module_kind: "lfo".into(),
    });

    controls.push(ControlDescriptor {
        name: "Scale".into(),
        module: m.lfo_depth as u32,
        param: Attenuverter::PARAM_SCALE as u32,
        min: 0.0,
        max: 2.0,
        default_value: 0.2,
        module_name: "Attenuverter".into(),
        module_kind: "attenuverter".into(),
    });
    controls.push(ControlDescriptor {
        name: "Offset".into(),
        module: m.lfo_depth as u32,
        param: Attenuverter::PARAM_OFFSET as u32,
        min: -1.0,
        max: 1.0,
        default_value: 0.0,
        module_name: "Attenuverter".into(),
        module_kind: "attenuverter".into(),
    });

    controls.push(ControlDescriptor {
        name: "Level".into(),
        module: m.vca as u32,
        param: Vca::PARAM_LEVEL as u32,
        min: 0.0,
        max: 1.5,
        default_value: 1.0,
        module_name: "VCA".into(),
        module_kind: "vca".into(),
    });

    controls
}

pub struct SynthService {
    tx: Sender<Command>,
    meters: Arc<EngineMeters>,
    descriptors: Vec<ControlDescriptor>,
}

impl SynthService {
    fn send(&self, command: Command) -> Result<(), Status> {
        self.tx
            .send(command)
            .map_err(|_| Status::internal("audio thread is no longer running"))
    }
}

#[tonic::async_trait]
impl SynthRpc for SynthService {
    async fn set_param(&self, request: Request<SetParamRequest>) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        debug!(module = req.module, param = req.param, value = req.value, "set_param");
        self.send(Command::SetParam {
            module: req.module as usize,
            param: req.param as usize,
            value: req.value,
        })?;
        Ok(Response::new(Empty {}))
    }

    async fn set_output_calibration(
        &self,
        request: Request<CalibrationRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        self.send(Command::SetOutputCalibration {
            channel: req.channel as usize,
            scale: req.scale,
            offset: req.offset,
        })?;
        Ok(Response::new(Empty {}))
    }

    async fn note_on(&self, request: Request<NoteOnRequest>) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let note = wmidi::Note::try_from(req.note as u8)
            .map_err(|_| Status::invalid_argument("note out of MIDI range"))?;
        let velocity = wmidi::Velocity::try_from(req.velocity.clamp(1, 127) as u8)
            .map_err(|_| Status::invalid_argument("velocity out of MIDI range"))?;
        debug!(note = req.note, velocity = req.velocity, "note_on");
        self.send(Command::NoteOn { note, velocity })?;
        Ok(Response::new(Empty {}))
    }

    async fn note_off(&self, request: Request<NoteOffRequest>) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let note = wmidi::Note::try_from(req.note as u8)
            .map_err(|_| Status::invalid_argument("note out of MIDI range"))?;
        debug!(note = req.note, "note_off");
        self.send(Command::NoteOff { note })?;
        Ok(Response::new(Empty {}))
    }

    async fn list_controls(&self, _request: Request<Empty>) -> Result<Response<ControlList>, Status> {
        debug!(count = self.descriptors.len(), "list_controls");
        Ok(Response::new(ControlList {
            controls: self.descriptors.clone(),
        }))
    }

    type StreamStateStream = Pin<Box<dyn futures_core::Stream<Item = Result<EngineState, Status>> + Send>>;

    async fn stream_state(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::StreamStateStream>, Status> {
        info!("stream_state: client subscribed");
        let meters = Arc::clone(&self.meters);
        let (tx, rx) = tokio_mpsc::channel(8);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(33)); // ~30 Hz
            loop {
                interval.tick().await;
                let state = EngineState {
                    active_voices: meters.active_voices(),
                    output_peak: meters.output_peak(),
                };
                if tx.send(Ok(state)).await.is_err() {
                    info!("stream_state: client disconnected");
                    break;
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

/// Runs the gRPC server until the process exits. `tx` is the same
/// `mpsc::Sender<Command>` the audio callback drains; `meters` is the same
/// `EngineMeters` the audio callback updates after every block.
pub async fn serve(
    addr: &str,
    tx: Sender<Command>,
    meters: Arc<EngineMeters>,
    descriptors: Vec<ControlDescriptor>,
) -> Result<(), Box<dyn std::error::Error>> {
    let service = SynthService { tx, meters, descriptors };
    Server::builder()
        .add_service(SynthServer::new(service))
        .serve(addr.parse()?)
        .await?;
    Ok(())
}
