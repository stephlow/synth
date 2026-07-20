//! The gRPC server side: turns `synth-proto` requests into `Command`s sent
//! to the audio thread, and turns `EngineMeters` into a `StreamState`
//! stream. Keeps everything real-time-unsafe (allocation, locking, async)
//! strictly off the audio thread — see `Command`/`EngineMeters` in the
//! `synth` crate for the wait-free handoff this relies on.

use std::pin::Pin;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use synth::{Command, EngineMeters, Synth};
use synth_proto::synth_server::{Synth as SynthRpc, SynthServer};
use synth_proto::{
    CalibrationRequest, ControlDescriptor, ControlList, Empty, EngineState, NoteOffRequest,
    NoteOnRequest, SetParamRequest,
};
use tokio::sync::mpsc as tokio_mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, error, info};

/// Builds the control descriptor table this engine exposes over gRPC by
/// translating `Synth::describe_patch()` — the module/param metadata the
/// running `Patch` produced when it built the graph — into the wire type. No
/// module list is hand-maintained here: whatever the patch describes itself
/// as is exactly what the `rack` TUI renders.
pub fn build_control_descriptors(probe: &Synth) -> Vec<ControlDescriptor> {
    let modules = probe.describe_patch();
    debug!(module_count = modules.len(), "describing patch for control descriptors");
    let descriptors: Vec<ControlDescriptor> = modules
        .iter()
        .flat_map(|module_info| {
            debug!(module = module_info.module, name = module_info.name, kind = module_info.kind, param_count = module_info.params.len(), "module described");
            module_info.params.iter().map(move |param_info| ControlDescriptor {
                name: param_info.name.into(),
                module: module_info.module as u32,
                param: param_info.param as u32,
                min: param_info.min,
                max: param_info.max,
                default_value: param_info.default_value,
                module_name: module_info.name.into(),
                module_kind: module_info.kind.into(),
            })
        })
        .collect();
    info!(count = descriptors.len(), "built control descriptor table");
    descriptors
}

pub struct SynthService {
    tx: Sender<Command>,
    meters: Arc<EngineMeters>,
    descriptors: Vec<ControlDescriptor>,
}

impl SynthService {
    fn send(&self, command: Command) -> Result<(), Status> {
        self.tx.send(command).map_err(|_| {
            error!("failed to send command: audio thread is no longer running");
            Status::internal("audio thread is no longer running")
        })
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
        debug!(channel = req.channel, scale = req.scale, offset = req.offset, "set_output_calibration");
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
    debug!(addr, "starting gRPC server");
    let result = Server::builder()
        .add_service(SynthServer::new(service))
        .serve(addr.parse()?)
        .await;
    match &result {
        Ok(()) => info!("gRPC server stopped"),
        Err(err) => error!(%err, "gRPC server exited with an error"),
    }
    result?;
    Ok(())
}
