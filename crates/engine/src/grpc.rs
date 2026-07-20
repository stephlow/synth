//! The gRPC server side: turns `synth-proto` requests into `Command`s sent
//! to the audio worker, and turns `EngineMeters` into a `StreamState`
//! stream. Device open/close and patch topology are delegated to the IO
//! control thread via `IoControl` — never done on the audio path.

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use dsp::{
    registry, Command, EngineMeters, IoDirection, Synth, IO_ENDPOINT_AUDIO_IN,
    IO_ENDPOINT_AUDIO_OUT, MASTER_IO_MODULE,
};
use synth_proto::synth_server::{Synth as SynthRpc, SynthServer};
use synth_proto::{
    AddModuleRequest, AudioDevice, AudioDeviceList, CableDescriptor, CableSignal,
    CalibrationRequest, ConnectRequest, ControlDescriptor, ControlList, DisconnectRequest, Empty,
    EngineState, IoChannelMeter, IoEndpointDescriptor, IoEndpointList, LoadPatchRequest,
    ModuleCoordinates as ProtoModuleCoordinates, ModuleInstanceDescriptor, ModuleSignal,
    ModuleType, ModuleTypeList, NoteOffRequest, NoteOnRequest, ParamTypeDescriptor,
    PatchCanvas as ProtoPatchCanvas, PatchRolesDescriptor, PatchSnapshot, PortDescriptor,
    RemoveModuleRequest, SavePatchResponse, SetCoordinatesRequest, SetIoDeviceRequest,
    SetModuleCoordinatesRequest, SetParamRequest,
};
use tokio::sync::mpsc as tokio_mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, error, info};

use crate::audio_io::list_audio_devices;
use crate::{IoControl, PatchOp};

/// Shared control/IO descriptor tables refreshed after topology edits.
#[derive(Clone, Default)]
pub struct ControlTables {
    pub controls: Vec<ControlDescriptor>,
    pub io_endpoints: Vec<IoEndpointDescriptor>,
}

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
                name = %module_info.name,
                kind = %module_info.kind,
                param_count = module_info.params.len(),
                "module described"
            );
            module_info
                .params
                .iter()
                .map(move |param_info| ControlDescriptor {
                    name: param_info.name.clone(),
                    module: module_info.module,
                    param: param_info.param as u32,
                    min: param_info.min,
                    max: param_info.max,
                    default_value: param_info.default_value,
                    module_name: module_info.name.clone(),
                    module_kind: module_info.kind.clone(),
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
                name: param_info.name.clone(),
                module: MASTER_IO_MODULE as u32,
                param: param_info.param as u32,
                min: param_info.min,
                max: param_info.max,
                default_value: param_info.default_value,
                module_name: out.name.clone(),
                module_kind: out.kind.clone(),
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
            id: ep.id.clone(),
            name: ep.name.clone(),
            direction: match ep.direction {
                IoDirection::Input => "input".into(),
                IoDirection::Output => "output".into(),
            },
            channels: ep.channels as u32,
            device_name: ep.device.clone(),
            coordinates: Some(ProtoModuleCoordinates {
                x: ep.coordinates.x,
                y: ep.coordinates.y,
            }),
        })
        .collect()
}

fn build_module_type_list() -> ModuleTypeList {
    let types = registry::all_types()
        .into_iter()
        .map(|t| ModuleType {
            kind: t.kind.into(),
            display_name: t.display_name.into(),
            inputs: t
                .inputs
                .iter()
                .map(|p| PortDescriptor {
                    name: p.name.into(),
                    index: p.index as u32,
                })
                .collect(),
            outputs: t
                .outputs
                .iter()
                .map(|p| PortDescriptor {
                    name: p.name.into(),
                    index: p.index as u32,
                })
                .collect(),
            params: t
                .params
                .iter()
                .map(|p| ParamTypeDescriptor {
                    name: p.name.into(),
                    index: p.index as u32,
                    min: p.min,
                    max: p.max,
                    default_value: p.default,
                    choices: p.choices.iter().map(|s| (*s).into()).collect(),
                })
                .collect(),
        })
        .collect();
    ModuleTypeList { types }
}

fn snapshot_from_json(
    document_json: &str,
    tables: &ControlTables,
) -> Result<PatchSnapshot, Status> {
    let patch = dsp::Patch::from_json(document_json)
        .map_err(|e| Status::invalid_argument(e.to_string()))?;
    Ok(PatchSnapshot {
        controls: tables.controls.clone(),
        cables: patch
            .cables
            .iter()
            .map(|c| CableDescriptor {
                id: c.id,
                src_module: c.src_module,
                src_port: c.src_port as u32,
                dst_module: c.dst_module,
                dst_port: c.dst_port as u32,
            })
            .collect(),
        modules: patch
            .modules
            .iter()
            .map(|m| ModuleInstanceDescriptor {
                id: m.id,
                kind: m.kind.clone(),
                name: m.name.clone(),
                coordinates: Some(ProtoModuleCoordinates {
                    x: m.coordinates.x,
                    y: m.coordinates.y,
                }),
            })
            .collect(),
        roles: Some(PatchRolesDescriptor {
            note_source: patch.roles.note_source,
            activity_gate: patch.roles.activity_gate,
            external_input: patch.roles.external_input,
            output_tap_module: patch.roles.output_tap_module,
            output_tap_port: patch.roles.output_tap_port as u32,
        }),
        io_endpoints: tables.io_endpoints.clone(),
        document_json: document_json.to_string(),
        canvas: patch.canvas.map(|c| ProtoPatchCanvas {
            width: c.width,
            height: c.height,
        }),
    })
}

pub struct SynthService {
    tx: Sender<Command>,
    io_tx: Sender<IoControl>,
    meters: Arc<EngineMeters>,
    tables: Arc<RwLock<ControlTables>>,
    /// Topology revision for control-surface sync (see `EngineState.patch_generation`).
    patch_generation: Arc<AtomicU64>,
}

impl SynthService {
    fn send(&self, command: Command) -> Result<(), Status> {
        self.tx.send(command).map_err(|_| {
            error!("failed to send command: audio thread is no longer running");
            Status::internal("audio thread is no longer running")
        })
    }

    fn apply_op(&self, op: PatchOp) -> Result<PatchSnapshot, Status> {
        // Topology edits bump generation so control surfaces re-fetch.
        // Coordinates/canvas and pure snapshots do not (layout is document-only;
        // the client that wrote coords already has them).
        let bumps_generation = !matches!(
            op,
            PatchOp::GetSnapshot
                | PatchOp::SetModuleCoordinates { .. }
                | PatchOp::SetCoordinates { .. }
        );
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.io_tx
            .send(IoControl::ApplyPatchOp {
                op,
                reply: reply_tx,
            })
            .map_err(|_| Status::internal("IO control thread is no longer running"))?;

        let result = reply_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| Status::deadline_exceeded("timed out applying patch operation"))?
            .map_err(Status::failed_precondition)?;

        if bumps_generation {
            self.patch_generation.fetch_add(1, Ordering::Relaxed);
        }

        // Rebuild descriptor tables from the applied document + probe modules.
        // Controls must include master IO; reconstruct via a temporary synth
        // would re-open audio — instead merge module infos with existing
        // master-out controls pattern.
        let patch = dsp::Patch::from_json(&result.document_json)
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut controls: Vec<ControlDescriptor> = result
            .module_infos
            .iter()
            .flat_map(|module_info| {
                module_info.params.iter().map(move |param_info| ControlDescriptor {
                    name: param_info.name.clone(),
                    module: module_info.module,
                    param: param_info.param as u32,
                    min: param_info.min,
                    max: param_info.max,
                    default_value: param_info.default_value,
                    module_name: module_info.name.clone(),
                    module_kind: module_info.kind.clone(),
                })
            })
            .collect();

        // Preserve master Audio Out calibration knobs from previous tables
        // (or rebuild from patch io endpoints).
        if let Some(out) = patch
            .io_endpoints
            .iter()
            .find(|e| e.id == IO_ENDPOINT_AUDIO_OUT)
        {
            for param_info in &out.params {
                controls.push(ControlDescriptor {
                    name: param_info.name.clone(),
                    module: MASTER_IO_MODULE as u32,
                    param: param_info.param as u32,
                    min: param_info.min,
                    max: param_info.max,
                    default_value: param_info.default_value,
                    module_name: out.name.clone(),
                    module_kind: out.kind.clone(),
                });
            }
        }

        let io_endpoints: Vec<IoEndpointDescriptor> = patch
            .io_endpoints
            .iter()
            .map(|ep| IoEndpointDescriptor {
                id: ep.id.clone(),
                name: ep.name.clone(),
                direction: match ep.direction {
                    IoDirection::Input => "input".into(),
                    IoDirection::Output => "output".into(),
                },
                channels: ep.channels as u32,
                device_name: ep.device.clone(),
                coordinates: Some(ProtoModuleCoordinates {
                    x: ep.coordinates.x,
                    y: ep.coordinates.y,
                }),
            })
            .collect();

        let tables = ControlTables {
            controls,
            io_endpoints,
        };
        if let Ok(mut guard) = self.tables.write() {
            *guard = tables.clone();
        }

        snapshot_from_json(&result.document_json, &tables)
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
        let controls = self
            .tables
            .read()
            .map(|t| t.controls.clone())
            .unwrap_or_default();
        debug!(count = controls.len(), "list_controls");
        Ok(Response::new(ControlList { controls }))
    }

    async fn list_io_endpoints(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<IoEndpointList>, Status> {
        let status = self.meters.io_status();
        let mut endpoints = self
            .tables
            .read()
            .map(|t| t.io_endpoints.clone())
            .unwrap_or_default();
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

        self.send(Command::SetIoDevice {
            endpoint_id: req.endpoint_id.clone(),
            device_name: req.device_name.clone(),
        })?;

        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.io_tx
            .send(IoControl::SetDevice {
                endpoint_id: req.endpoint_id,
                device_name: req.device_name,
                reply: reply_tx,
            })
            .map_err(|_| Status::internal("IO control thread is no longer running"))?;

        let result = reply_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| Status::deadline_exceeded("timed out reopening audio device"))?;

        result.map_err(Status::failed_precondition)?;
        Ok(Response::new(Empty {}))
    }

    async fn list_module_types(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ModuleTypeList>, Status> {
        Ok(Response::new(build_module_type_list()))
    }

    async fn get_patch(&self, _request: Request<Empty>) -> Result<Response<PatchSnapshot>, Status> {
        let snap = self.apply_op(PatchOp::GetSnapshot)?;
        Ok(Response::new(snap))
    }

    async fn add_module(
        &self,
        request: Request<AddModuleRequest>,
    ) -> Result<Response<PatchSnapshot>, Status> {
        let kind = request.into_inner().kind;
        debug!(%kind, "add_module");
        let snap = self.apply_op(PatchOp::AddModule { kind })?;
        Ok(Response::new(snap))
    }

    async fn remove_module(
        &self,
        request: Request<RemoveModuleRequest>,
    ) -> Result<Response<PatchSnapshot>, Status> {
        let id = request.into_inner().module_id;
        debug!(id, "remove_module");
        let snap = self.apply_op(PatchOp::RemoveModule { id })?;
        Ok(Response::new(snap))
    }

    async fn patch_connect(
        &self,
        request: Request<ConnectRequest>,
    ) -> Result<Response<PatchSnapshot>, Status> {
        let req = request.into_inner();
        debug!(
            src = req.src_module,
            dst = req.dst_module,
            "patch_connect"
        );
        let snap = self.apply_op(PatchOp::Connect {
            src_module: req.src_module,
            src_port: req.src_port,
            dst_module: req.dst_module,
            dst_port: req.dst_port,
        })?;
        Ok(Response::new(snap))
    }

    async fn patch_disconnect(
        &self,
        request: Request<DisconnectRequest>,
    ) -> Result<Response<PatchSnapshot>, Status> {
        let req = request.into_inner();
        let snap = if req.use_cable_id {
            self.apply_op(PatchOp::Disconnect {
                cable_id: Some(req.cable_id),
                src_module: 0,
                src_port: 0,
                dst_module: 0,
                dst_port: 0,
            })?
        } else {
            self.apply_op(PatchOp::Disconnect {
                cable_id: None,
                src_module: req.src_module,
                src_port: req.src_port,
                dst_module: req.dst_module,
                dst_port: req.dst_port,
            })?
        };
        Ok(Response::new(snap))
    }

    async fn load_patch(
        &self,
        request: Request<LoadPatchRequest>,
    ) -> Result<Response<PatchSnapshot>, Status> {
        let json = request.into_inner().document_json;
        debug!(bytes = json.len(), "load_patch");
        let snap = self.apply_op(PatchOp::LoadDocument { json })?;
        Ok(Response::new(snap))
    }

    async fn save_patch(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<SavePatchResponse>, Status> {
        let snap = self.apply_op(PatchOp::GetSnapshot)?;
        Ok(Response::new(SavePatchResponse {
            document_json: snap.document_json,
        }))
    }

    async fn set_module_coordinates(
        &self,
        request: Request<SetModuleCoordinatesRequest>,
    ) -> Result<Response<PatchSnapshot>, Status> {
        let req = request.into_inner();
        let snap = self.apply_op(PatchOp::SetModuleCoordinates {
            id: req.module_id,
            x: req.x,
            y: req.y,
        })?;
        Ok(Response::new(snap))
    }

    async fn set_coordinates(
        &self,
        request: Request<SetCoordinatesRequest>,
    ) -> Result<Response<PatchSnapshot>, Status> {
        let req = request.into_inner();
        let positions: Vec<(u32, u32, u32)> = req
            .modules
            .iter()
            .map(|m| (m.module_id, m.x, m.y))
            .collect();
        let canvas = req.canvas.map(|c| dsp::PatchCanvas {
            width: c.width.max(1),
            height: c.height.max(1),
        });
        let snap = self.apply_op(PatchOp::SetCoordinates { positions, canvas })?;
        Ok(Response::new(snap))
    }

    type StreamStateStream =
        Pin<Box<dyn futures_core::Stream<Item = Result<EngineState, Status>> + Send>>;

    async fn stream_state(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::StreamStateStream>, Status> {
        info!("stream_state: client subscribed");
        let meters = Arc::clone(&self.meters);
        let patch_generation = Arc::clone(&self.patch_generation);
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

                let module_signals: Vec<ModuleSignal> = meters
                    .module_signals()
                    .into_iter()
                    .map(|m| ModuleSignal {
                        module_id: m.module_id,
                        peak: m.peak,
                        waveform: m.waveform,
                    })
                    .collect();
                let cable_signals: Vec<CableSignal> = meters
                    .cable_signals()
                    .into_iter()
                    .map(|c| CableSignal {
                        cable_id: c.cable_id,
                        peak: c.peak,
                    })
                    .collect();
                let state = EngineState {
                    active_voices: meters.active_voices(),
                    output_peak: meters.output_peak(),
                    waveform: meters.waveform(),
                    io_meters,
                    patch_generation: patch_generation.load(Ordering::Relaxed),
                    module_signals,
                    cable_signals,
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
    _probe: Synth,
) -> Result<(), Box<dyn std::error::Error>> {
    let tables = Arc::new(RwLock::new(ControlTables {
        controls: descriptors,
        io_endpoints: io_descriptors,
    }));
    let service = SynthService {
        tx,
        io_tx,
        meters,
        tables,
        patch_generation: Arc::new(AtomicU64::new(0)),
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
