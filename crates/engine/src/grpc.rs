//! The gRPC server side: turns `synth-proto` requests into `Command`s sent
//! to the audio worker, and turns `EngineMeters` into a `StreamState`
//! stream. Device open/close is delegated to the IO control thread via
//! `IoControl` — never done on the audio path.

use std::pin::Pin;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use dsp::{
    Command, EngineMeters, IoDirection, Synth, IO_ENDPOINT_AUDIO_IN, IO_ENDPOINT_AUDIO_OUT,
    MASTER_IO_MODULE,
};
use synth_proto::synth_server::{Synth as SynthRpc, SynthServer};
use synth_proto::{
    AudioDevice, AudioDeviceList, CalibrationRequest, ControlDescriptor, ControlList, Empty,
    EngineState, IoChannelMeter, IoEndpointDescriptor, IoEndpointList, NoteOffRequest,
    NoteOnRequest, SetIoDeviceRequest, SetParamRequest,
};
use tokio::sync::mpsc as tokio_mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, error, info};

use crate::audio_io::list_audio_devices;
use crate::IoControl;

/// Builds the control descriptor table this engine exposes over gRPC by
/// translating `Synth::describe_patch()` plus master IO calibration knobs
/// from `describe_io()`.
pub fn build_control_descriptors(probe: &Synth) -> Vec<ControlDescriptor> {
    let modules = probe.describe_patch();
    debug!(
        module_count = modules.len(),
        "describing patch for control descriptors"
    );
    let mut descriptors: Vec<ControlDescriptor> = modules
        .iter()
        .flat_map(|module_info| {
            debug!(
                module = module_info.module,
                name = module_info.name,
                kind = module_info.kind,
                param_count = module_info.params.len(),
                "module described"
            );
            module_info
                .params
                .iter()
                .map(move |param_info| ControlDescriptor {
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

    // Master Audio Out calibration lives outside the voice graph.
    if let Some(out) = probe
        .describe_io()
        .iter()
        .find(|e| e.id == IO_ENDPOINT_AUDIO_OUT)
    {
        for param_info in &out.params {
            descriptors.push(ControlDescriptor {
                name: param_info.name.into(),
                module: MASTER_IO_MODULE as u32,
                param: param_info.param as u32,
                min: param_info.min,
                max: param_info.max,
                default_value: param_info.default_value,
                module_name: out.name.into(),
                module_kind: out.kind.into(),
            });
        }
    }

    info!(count = descriptors.len(), "built control descriptor table");
    descriptors
}

pub fn build_io_descriptors(probe: &Synth) -> Vec<IoEndpointDescriptor> {
    probe
        .describe_io()
        .iter()
        .map(|ep| IoEndpointDescriptor {
            id: ep.id.into(),
            name: ep.name.into(),
            direction: match ep.direction {
                IoDirection::Input => "input".into(),
                IoDirection::Output => "output".into(),
            },
            channels: ep.channels as u32,
            device_name: ep.device.clone(),
        })
        .collect()
}

pub struct SynthService {
    tx: Sender<Command>,
    io_tx: Sender<IoControl>,
    meters: Arc<EngineMeters>,
    descriptors: Vec<ControlDescriptor>,
    io_descriptors: Vec<IoEndpointDescriptor>,
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
        debug!(
            module = req.module,
            param = req.param,
            value = req.value,
            "set_param"
        );
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
        debug!(
            channel = req.channel,
            scale = req.scale,
            offset = req.offset,
            "set_output_calibration"
        );
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

    async fn list_controls(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ControlList>, Status> {
        debug!(count = self.descriptors.len(), "list_controls");
        Ok(Response::new(ControlList {
            controls: self.descriptors.clone(),
        }))
    }

    async fn list_io_endpoints(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<IoEndpointList>, Status> {
        // Merge patch descriptors with live resolved device names from meters.
        let status = self.meters.io_status();
        let mut endpoints = self.io_descriptors.clone();
        for ep in &mut endpoints {
            if let Some(s) = status.iter().find(|s| s.endpoint_id == ep.id) {
                if !s.device_name.is_empty() {
                    ep.device_name = s.device_name.clone();
                }
            }
        }
        Ok(Response::new(IoEndpointList { endpoints }))
    }

    async fn list_audio_devices(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<AudioDeviceList>, Status> {
        let devices = list_audio_devices()
            .into_iter()
            .map(|d| AudioDevice {
                name: d.name,
                is_input: d.is_input,
                is_output: d.is_output,
                max_input_channels: d.max_input_channels,
                max_output_channels: d.max_output_channels,
            })
            .collect();
        Ok(Response::new(AudioDeviceList { devices }))
    }

    async fn set_io_device(
        &self,
        request: Request<SetIoDeviceRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        debug!(
            endpoint_id = %req.endpoint_id,
            device_name = %req.device_name,
            "set_io_device"
        );

        // Update patch state on the audio worker first.
        self.send(Command::SetIoDevice {
            endpoint_id: req.endpoint_id.clone(),
            device_name: req.device_name.clone(),
        })?;

        // Realize hardware on the IO control thread.
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.io_tx
            .send(IoControl::SetDevice {
                endpoint_id: req.endpoint_id,
                device_name: req.device_name,
                reply: reply_tx,
            })
            .map_err(|_| Status::internal("IO control thread is no longer running"))?;

        // Blocking wait is fine here: we're on the gRPC worker, not audio.
        let result = reply_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| Status::deadline_exceeded("timed out reopening audio device"))?;

        result.map_err(Status::failed_precondition)?;
        Ok(Response::new(Empty {}))
    }

    type StreamStateStream =
        Pin<Box<dyn futures_core::Stream<Item = Result<EngineState, Status>> + Send>>;

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
                let status = meters.io_status();
                let io_meters: Vec<IoChannelMeter> = status
                    .iter()
                    .map(|s| {
                        let slot = match s.endpoint_id.as_str() {
                            id if id == IO_ENDPOINT_AUDIO_OUT => EngineMeters::SLOT_AUDIO_OUT,
                            id if id == IO_ENDPOINT_AUDIO_IN => EngineMeters::SLOT_AUDIO_IN,
                            _ => usize::MAX,
                        };
                        let peak = if slot == usize::MAX {
                            Vec::new()
                        } else {
                            meters.io_peaks(slot)
                        };
                        IoChannelMeter {
                            endpoint_id: s.endpoint_id.clone(),
                            peak,
                            device_name: s.device_name.clone(),
                            connected: s.connected,
                        }
                    })
                    .collect();

                let state = EngineState {
                    active_voices: meters.active_voices(),
                    output_peak: meters.output_peak(),
                    waveform: meters.waveform(),
                    io_meters,
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

/// Runs the gRPC server until the process exits.
pub async fn serve(
    addr: &str,
    tx: Sender<Command>,
    io_tx: Sender<IoControl>,
    meters: Arc<EngineMeters>,
    descriptors: Vec<ControlDescriptor>,
    io_descriptors: Vec<IoEndpointDescriptor>,
) -> Result<(), Box<dyn std::error::Error>> {
    let service = SynthService {
        tx,
        io_tx,
        meters,
        descriptors,
        io_descriptors,
    };
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
