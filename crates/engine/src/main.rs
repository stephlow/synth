//! Live engine host: cpal IO + MIDI + gRPC.
//!
//! **Latency model:** DSP runs inside the **output device callback** (device
//! clock is the master). Capture uses a small SPSC ring between the input
//! and output callbacks. There is no soft-RT worker pre-filling a large
//! output buffer.
//!
//! **Device hot-swap:** streams live on the IO control thread (cpal `Stream`
//! is `!Send` on some hosts). `AudioCore` is shared via `Arc`; the output
//! callback has exclusive access to DSP state while a stream is running.
//! Dropping the stream joins the callback, then the IO thread may reopen.

mod audio_io;
mod grpc;

use std::cell::UnsafeCell;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use cpal::traits::DeviceTrait;
use cpal::{FromSample, Sample, SampleFormat};
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

use audio_io::{
    open_input_stream, open_output_config, open_output_for_format, open_output_process_stream,
    status_from_endpoints, warn_optional_input, SharedCapture, NUM_EXTERNAL_INPUT_CHANNELS,
};
use dsp::midi::{setup_midi, MidiMessage};
use dsp::modules::{Attenuverter, Envelope, Lfo, Vcf};
use dsp::{
    rt, Command, EngineMeters, IoEndpoint, ModuleInfo, Synth, IO_ENDPOINT_AUDIO_IN,
    IO_ENDPOINT_AUDIO_OUT, MAX_BLOCK_FRAMES,
};

/// Address the gRPC server listens on; the `rack` TUI connects here.
const GRPC_ADDR: &str = "127.0.0.1:50051";

/// Control-plane requests: device reopen and off-audio-thread patch topology.
#[derive(Debug)]
pub enum IoControl {
    SetDevice {
        endpoint_id: String,
        device_name: String,
        reply: std::sync::mpsc::Sender<Result<(), String>>,
    },
    /// Mutate topology / load patch with exclusive access (stream dropped).
    ApplyPatchOp {
        op: PatchOp,
        reply: std::sync::mpsc::Sender<Result<PatchApplyResult, String>>,
    },
}

/// Topology / load operations applied off the audio callback.
#[derive(Debug)]
pub enum PatchOp {
    AddModule { kind: String },
    RemoveModule { id: u32 },
    Connect {
        src_module: u32,
        src_port: u32,
        dst_module: u32,
        dst_port: u32,
    },
    Disconnect {
        cable_id: Option<u32>,
        src_module: u32,
        src_port: u32,
        dst_module: u32,
        dst_port: u32,
    },
    LoadDocument { json: String },
    /// Snapshot only (no rebuild); still takes exclusive access briefly.
    GetSnapshot,
    /// Control-surface lattice coordinates for one module.
    SetModuleCoordinates { id: u32, x: u32, y: u32 },
    /// Batch coordinates + optional canvas size.
    SetCoordinates {
        positions: Vec<(u32, u32, u32)>,
        canvas: Option<dsp::PatchCanvas>,
    },
}

/// Result of a successful patch op (for gRPC tables + client refresh).
#[derive(Debug, Clone)]
pub struct PatchApplyResult {
    pub document_json: String,
    pub module_infos: Vec<ModuleInfo>,
}

/// Mutable DSP + command state accessed **only** from the output callback
/// while a stream is running, or from the IO thread after the stream is
/// dropped (never concurrently).
struct AudioInner {
    synth: Synth,
    rx: Receiver<Command>,
    calibration: OutputCalibration,
    cc_targets: CcTargets,
    external_scratch: [[f32; MAX_BLOCK_FRAMES]; NUM_EXTERNAL_INPUT_CHANNELS],
}

/// Shared with the output stream callback. Capture slot uses try_lock only.
struct AudioCore {
    inner: UnsafeCell<AudioInner>,
    capture: SharedCapture,
    meters: Arc<EngineMeters>,
}

// Safety: exclusive access protocol — callback XOR IO thread after stream drop.
unsafe impl Send for AudioCore {}
unsafe impl Sync for AudioCore {}

impl AudioCore {
    fn new(
        sample_rate: f32,
        rx: Receiver<Command>,
        cc_targets: CcTargets,
        capture: SharedCapture,
        meters: Arc<EngineMeters>,
    ) -> Self {
        Self {
            inner: UnsafeCell::new(AudioInner {
                synth: Synth::new(sample_rate),
                rx,
                calibration: OutputCalibration::default(),
                cc_targets,
                external_scratch: [[0.0; MAX_BLOCK_FRAMES]; NUM_EXTERNAL_INPUT_CHANNELS],
            }),
            capture,
            meters,
        }
    }

    /// RT callback entry: no blocking locks, no sleep, no heap alloc.
    fn process_output<T: Sample + FromSample<f32>>(&self, output: &mut [T], num_channels: usize) {
        rt::flush_denormals_to_zero();

        // SAFETY: only the output callback may call this while the stream
        // is alive; cpal does not re-enter the callback.
        let inner = unsafe { &mut *self.inner.get() };

        while let Ok(command) = inner.rx.try_recv() {
            match command {
                Command::Midi(midi_message) => match midi_message {
                    MidiMessage::NoteOn(note, velocity) => inner.synth.note_on(note, velocity),
                    MidiMessage::NoteOff(note, velocity) => inner.synth.note_off(note, velocity),
                    MidiMessage::ProgramChange(_program) => {}
                    MidiMessage::ControlChange(controller, value) => {
                        apply_control_change(
                            &mut inner.synth,
                            &inner.cc_targets,
                            &mut inner.calibration,
                            controller,
                            value,
                        );
                    }
                },
                Command::SetParam { module, param, value } => {
                    inner.synth.set_param(module, param, value);
                }
                Command::SetOutputCalibration {
                    channel,
                    scale,
                    offset,
                } => inner.synth.set_output_calibration(channel, scale, offset),
                Command::SetIoDevice {
                    endpoint_id,
                    device_name,
                } => {
                    let _ = inner.synth.set_io_device(&endpoint_id, &device_name);
                }
                Command::NoteOn { note, velocity } => inner.synth.note_on(note, velocity),
                Command::NoteOff { note } => {
                    inner.synth.note_off(note, wmidi::Velocity::MIN);
                }
            }
        }

        process_frame(
            output,
            &mut inner.synth,
            num_channels,
            &self.capture,
            &mut inner.external_scratch,
        );

        let peak_l = inner
            .synth
            .output_channel(Synth::CHANNEL_AUDIO_L)
            .iter()
            .fold(0.0f32, |m, &s| m.max(s.abs()));
        let peak_r = inner
            .synth
            .output_channel(Synth::CHANNEL_AUDIO_R)
            .iter()
            .fold(0.0f32, |m, &s| m.max(s.abs()));
        let peak = peak_l.max(peak_r);
        self.meters
            .update(peak, inner.synth.active_voice_count());
        self.meters
            .publish_waveform(inner.synth.scope_waveform());
        self.meters
            .publish_live_signals(inner.synth.live_signals());

        let mut out_peaks = [0.0f32; 8];
        let n_out = Synth::NUM_OUTPUT_CHANNELS.min(8);
        for ch in 0..n_out {
            out_peaks[ch] = inner
                .synth
                .output_channel(ch)
                .iter()
                .fold(0.0f32, |m, &s| m.max(s.abs()));
        }
        self.meters
            .update_io_peaks(EngineMeters::SLOT_AUDIO_OUT, &out_peaks[..n_out]);

        let mut in_peaks = [0.0f32; 8];
        let n_in = NUM_EXTERNAL_INPUT_CHANNELS.min(8);
        for ch in 0..n_in {
            in_peaks[ch] = inner.synth.external_input_peak(ch);
        }
        self.meters
            .update_io_peaks(EngineMeters::SLOT_AUDIO_IN, &in_peaks[..n_in]);
    }
}

/// Owns live cpal streams; realizes patch IO bindings.
struct IoRuntime {
    endpoints: Vec<IoEndpoint>,
    sample_rate: u32,
    core: Arc<AudioCore>,
    _input_stream: Option<cpal::Stream>,
    _output_stream: Option<cpal::Stream>,
    out_resolved: String,
    in_resolved: String,
    out_connected: bool,
    in_connected: bool,
    meters: Arc<EngineMeters>,
}

impl IoRuntime {
    fn publish_status(&self) {
        self.meters.set_io_status(status_from_endpoints(
            &self.endpoints,
            self.out_connected,
            &self.out_resolved,
            self.in_connected,
            &self.in_resolved,
        ));
    }

    fn set_device(&mut self, endpoint_id: &str, device_name: &str) -> Result<(), String> {
        let ep = self
            .endpoints
            .iter_mut()
            .find(|e| e.id == endpoint_id)
            .ok_or_else(|| format!("unknown IO endpoint '{endpoint_id}'"))?;
        ep.device = if device_name.is_empty() {
            dsp::IO_DEVICE_DEFAULT.to_string()
        } else {
            device_name.to_string()
        };
        let endpoints = self.endpoints.clone();

        match endpoint_id {
            id if id == IO_ENDPOINT_AUDIO_IN => self.reopen_input(&endpoints),
            id if id == IO_ENDPOINT_AUDIO_OUT => self.reopen_output(&endpoints),
            other => Err(format!("unsupported IO endpoint '{other}'")),
        }
    }

    fn reopen_input(&mut self, endpoints: &[IoEndpoint]) -> Result<(), String> {
        match open_input_stream(endpoints) {
            Ok((stream, consumers, resolved)) => {
                if let Ok(mut slot) = self.core.capture.lock() {
                    *slot = Some(consumers);
                }
                self._input_stream = Some(stream);
                self.in_resolved = resolved;
                self.in_connected = true;
                self.publish_status();
                info!(device = %self.in_resolved, "input device reopened");
                Ok(())
            }
            Err(err) => {
                if let Ok(mut slot) = self.core.capture.lock() {
                    *slot = None;
                }
                self._input_stream = None;
                self.in_connected = false;
                self.in_resolved = String::new();
                self.publish_status();
                Err(format!("failed to open input: {err}"))
            }
        }
    }

    /// Apply a topology op with exclusive AudioCore access (drops/reopens output).
    fn apply_patch_op(&mut self, op: PatchOp) -> Result<PatchApplyResult, String> {
        // Drop output so the callback joins and we own AudioInner exclusively.
        self._output_stream = None;

        // SAFETY: output stream is dropped; no callback can be running.
        let inner = unsafe { &mut *self.core.inner.get() };

        let result = (|| -> Result<PatchApplyResult, String> {
            match op {
                PatchOp::AddModule { kind } => {
                    inner
                        .synth
                        .add_module(&kind)
                        .map_err(|e| e.to_string())?;
                }
                PatchOp::RemoveModule { id } => {
                    inner
                        .synth
                        .remove_module(id)
                        .map_err(|e| e.to_string())?;
                }
                PatchOp::Connect {
                    src_module,
                    src_port,
                    dst_module,
                    dst_port,
                } => {
                    inner
                        .synth
                        .connect(
                            src_module,
                            src_port as usize,
                            dst_module,
                            dst_port as usize,
                        )
                        .map_err(|e| e.to_string())?;
                }
                PatchOp::Disconnect {
                    cable_id,
                    src_module,
                    src_port,
                    dst_module,
                    dst_port,
                } => {
                    if let Some(id) = cable_id {
                        inner
                            .synth
                            .disconnect_cable(id)
                            .map_err(|e| e.to_string())?;
                    } else {
                        inner
                            .synth
                            .disconnect(
                                src_module,
                                src_port as usize,
                                dst_module,
                                dst_port as usize,
                            )
                            .map_err(|e| e.to_string())?;
                    }
                }
                PatchOp::LoadDocument { json } => {
                    let patch = dsp::Patch::from_json(&json).map_err(|e| e.to_string())?;
                    inner.synth.load_patch(patch).map_err(|e| e.to_string())?;
                }
                PatchOp::GetSnapshot => {}
                PatchOp::SetModuleCoordinates { id, x, y } => {
                    inner
                        .synth
                        .set_module_coordinates(id, x, y)
                        .map_err(|e| e.to_string())?;
                }
                PatchOp::SetCoordinates { positions, canvas } => {
                    inner
                        .synth
                        .set_coordinates(&positions, canvas)
                        .map_err(|e| e.to_string())?;
                }
            }

            // Refresh MIDI CC targets from the new topology (best-effort:
            // missing kinds leave previous targets; resolve may panic on
            // incomplete patches — use try_resolve pattern).
            if let Ok(cc) = CcTargets::try_resolve(inner.synth.describe_patch()) {
                inner.cc_targets = cc;
            }

            self.endpoints = inner.synth.describe_io().to_vec();

            let document_json = inner
                .synth
                .save_patch_json()
                .map_err(|e| e.to_string())?;
            let module_infos = inner.synth.describe_patch().to_vec();
            Ok(PatchApplyResult {
                document_json,
                module_infos,
            })
        })();

        // Always try to reopen output so audio resumes even if op failed.
        let endpoints = self.endpoints.clone();
        if let Err(err) = self.reopen_output(&endpoints) {
            if result.is_ok() {
                return Err(format!("patch applied but failed to reopen output: {err}"));
            }
            // Prefer the original op error.
        }

        result
    }

    fn reopen_output(&mut self, endpoints: &[IoEndpoint]) -> Result<(), String> {
        // Drop current output first so the callback finishes and we regain
        // exclusive access to AudioCore (Synth state preserved).
        self._output_stream = None;

        let (device, supported) =
            open_output_config(endpoints).map_err(|e| format!("resolve output: {e}"))?;
        let rate = supported.sample_rate().0;
        if rate != self.sample_rate {
            return Err(format!(
                "sample rate mismatch: device wants {rate} Hz, engine runs at {} Hz",
                self.sample_rate
            ));
        }

        let stream = build_output_stream(&device, &supported, Arc::clone(&self.core))
            .map_err(|e| format!("open output: {e}"))?;
        let resolved = device.name().unwrap_or_else(|_| "<unnamed>".into());
        let channels = supported.channels() as usize;

        self._output_stream = Some(stream);
        self.out_resolved = resolved;
        self.out_connected = true;
        self.publish_status();
        info!(device = %self.out_resolved, channels, "output device reopened (callback DSP)");
        Ok(())
    }
}

fn build_output_stream(
    device: &cpal::Device,
    supported: &cpal::SupportedStreamConfig,
    core: Arc<AudioCore>,
) -> Result<cpal::Stream, anyhow::Error> {
    let num_channels = supported.channels() as usize;
    open_output_for_format(device, supported, |format, config| {
        let core = Arc::clone(&core);
        match format {
            SampleFormat::I8 => open_output_process_stream::<i8, _>(device, config, move |out| {
                core.process_output(out, num_channels);
            }),
            SampleFormat::I16 => open_output_process_stream::<i16, _>(device, config, move |out| {
                core.process_output(out, num_channels);
            }),
            SampleFormat::I32 => open_output_process_stream::<i32, _>(device, config, move |out| {
                core.process_output(out, num_channels);
            }),
            SampleFormat::I64 => open_output_process_stream::<i64, _>(device, config, move |out| {
                core.process_output(out, num_channels);
            }),
            SampleFormat::U8 => open_output_process_stream::<u8, _>(device, config, move |out| {
                core.process_output(out, num_channels);
            }),
            SampleFormat::U16 => open_output_process_stream::<u16, _>(device, config, move |out| {
                core.process_output(out, num_channels);
            }),
            SampleFormat::U32 => open_output_process_stream::<u32, _>(device, config, move |out| {
                core.process_output(out, num_channels);
            }),
            SampleFormat::U64 => open_output_process_stream::<u64, _>(device, config, move |out| {
                core.process_output(out, num_channels);
            }),
            SampleFormat::F32 => open_output_process_stream::<f32, _>(device, config, move |out| {
                core.process_output(out, num_channels);
            }),
            SampleFormat::F64 => open_output_process_stream::<f64, _>(device, config, move |out| {
                core.process_output(out, num_channels);
            }),
            other => Err(anyhow::anyhow!("unsupported sample format '{other}'")),
        }
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    info!(version = env!("CARGO_PKG_VERSION"), "starting engine");

    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
    let (io_tx, io_rx) = mpsc::channel::<IoControl>();

    debug!("connecting MIDI input");
    let _conn_in = setup_midi(cmd_tx.clone())?;
    info!("MIDI input connected");

    let probe = Synth::new(48_000.0);
    let endpoints = probe.describe_io().to_vec();
    let descriptors = grpc::build_control_descriptors(&probe);
    let cc_targets = CcTargets::resolve(probe.describe_patch());
    let io_descriptors = grpc::build_io_descriptors(&probe);

    let (out_device, out_config) = open_output_config(&endpoints)?;
    let sample_rate = out_config.sample_rate().0;
    let out_resolved_probe = out_device
        .name()
        .unwrap_or_else(|_| "<unnamed>".into());
    info!(
        sample_rate,
        channels = out_config.channels(),
        device = %out_resolved_probe,
        "output from patch binding"
    );
    drop(out_device);

    let meters = Arc::new(EngineMeters::default());
    let shared_capture: SharedCapture = Arc::new(Mutex::new(None));

    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

    let io_meters = Arc::clone(&meters);
    let io_capture = Arc::clone(&shared_capture);
    let io_endpoints = endpoints.clone();
    thread::Builder::new()
        .name("io-control".into())
        .spawn(move || {
            let core = Arc::new(AudioCore::new(
                sample_rate as f32,
                cmd_rx,
                cc_targets,
                Arc::clone(&io_capture),
                Arc::clone(&io_meters),
            ));

            let mut input_stream = None;
            let mut in_resolved = String::new();
            let mut in_connected = false;
            match open_input_stream(&io_endpoints) {
                Ok((stream, consumers, resolved)) => {
                    *io_capture.lock().unwrap() = Some(consumers);
                    input_stream = Some(stream);
                    in_resolved = resolved;
                    in_connected = true;
                    info!(device = %in_resolved, "input from patch binding");
                }
                Err(err) => warn_optional_input(&err),
            }

            let open_out = (|| {
                let (device, supported) = open_output_config(&io_endpoints)?;
                let stream = build_output_stream(&device, &supported, Arc::clone(&core))?;
                let resolved = device.name().unwrap_or_else(|_| "<unnamed>".into());
                Ok::<_, anyhow::Error>((stream, resolved))
            })();

            let (output_stream, out_resolved, out_connected) = match open_out {
                Ok((stream, resolved)) => {
                    info!(device = %resolved, "output stream started (callback DSP)");
                    (Some(stream), resolved, true)
                }
                Err(err) => {
                    let _ = ready_tx.send(Err(format!("failed to open output: {err}")));
                    return;
                }
            };

            let mut io_runtime = IoRuntime {
                endpoints: io_endpoints,
                sample_rate,
                core,
                _input_stream: input_stream,
                _output_stream: output_stream,
                out_resolved,
                in_resolved,
                out_connected,
                in_connected,
                meters: io_meters,
            };
            io_runtime.publish_status();
            let _ = ready_tx.send(Ok(()));

            while let Ok(msg) = io_rx.recv() {
                match msg {
                    IoControl::SetDevice {
                        endpoint_id,
                        device_name,
                        reply,
                    } => {
                        let result = io_runtime.set_device(&endpoint_id, &device_name);
                        let _ = reply.send(result);
                    }
                    IoControl::ApplyPatchOp { op, reply } => {
                        let result = io_runtime.apply_patch_op(op);
                        let _ = reply.send(result);
                    }
                }
            }
        })?;

    ready_rx
        .recv()
        .map_err(|_| anyhow::anyhow!("IO control thread exited before ready"))?
        .map_err(|e| anyhow::anyhow!(e))?;

    info!(addr = GRPC_ADDR, "gRPC server listening");
    grpc::serve(
        GRPC_ADDR,
        cmd_tx,
        io_tx,
        meters,
        descriptors,
        io_descriptors,
        probe,
    )
    .await?;

    info!("engine shutting down");
    Ok(())
}

/// Renders `output` (interleaved across `num_channels`) from the synth.
/// Capture consumers are taken with **try_lock** only — never blocks.
fn process_frame<SampleType>(
    output: &mut [SampleType],
    synth: &mut Synth,
    num_channels: usize,
    capture: &SharedCapture,
    external_scratch: &mut [[f32; MAX_BLOCK_FRAMES]; NUM_EXTERNAL_INPUT_CHANNELS],
) where
    SampleType: Sample + FromSample<f32>,
{
    let total_frames = output.len() / num_channels.max(1);
    let mut done = 0;
    while done < total_frames {
        let chunk_frames = (total_frames - done).min(MAX_BLOCK_FRAMES);

        // try_lock: if the IO thread is swapping devices, feed silence.
        match capture.try_lock() {
            Ok(mut guard) => match guard.as_mut() {
                Some(consumers) => {
                    for channel in 0..NUM_EXTERNAL_INPUT_CHANNELS {
                        let scratch = &mut external_scratch[channel][..chunk_frames];
                        for sample in scratch.iter_mut() {
                            *sample = consumers[channel].pop().unwrap_or(0.0);
                        }
                        synth.write_external_input(channel, scratch);
                    }
                }
                None => {
                    for channel in 0..NUM_EXTERNAL_INPUT_CHANNELS {
                        external_scratch[channel][..chunk_frames].fill(0.0);
                        synth.write_external_input(
                            channel,
                            &external_scratch[channel][..chunk_frames],
                        );
                    }
                }
            },
            Err(_) => {
                for channel in 0..NUM_EXTERNAL_INPUT_CHANNELS {
                    external_scratch[channel][..chunk_frames].fill(0.0);
                    synth.write_external_input(
                        channel,
                        &external_scratch[channel][..chunk_frames],
                    );
                }
            }
        }

        synth.process_block(chunk_frames);

        for i in 0..chunk_frames {
            let frame_start = (done + i) * num_channels;
            for (channel, out) in output[frame_start..frame_start + num_channels]
                .iter_mut()
                .enumerate()
            {
                let sample = if channel < Synth::NUM_OUTPUT_CHANNELS {
                    synth.output_channel(channel)[i]
                } else {
                    0.0
                };
                *out = SampleType::from_sample(sample);
            }
        }

        done += chunk_frames;
    }
}

/// Writes `frames` of synth output into an interleaved f32 buffer (tests).
fn render_interleaved(synth: &Synth, interleaved: &mut [f32], num_channels: usize, frames: usize) {
    debug_assert_eq!(interleaved.len(), frames * num_channels);
    for i in 0..frames {
        for channel in 0..num_channels {
            let sample = if channel < Synth::NUM_OUTPUT_CHANNELS {
                synth.output_channel(channel)[i]
            } else {
                0.0
            };
            interleaved[i * num_channels + channel] = sample;
        }
    }
}

struct OutputCalibration {
    pitch_scale: f32,
    pitch_offset: f32,
    gate_scale: f32,
    gate_offset: f32,
}

impl Default for OutputCalibration {
    fn default() -> Self {
        Self {
            pitch_scale: 1.0,
            pitch_offset: 0.0,
            gate_scale: 1.0,
            gate_offset: 0.0,
        }
    }
}

/// Best-effort MIDI CC map: each target is optional so incomplete patches
/// (e.g. default without attenuverter) still boot; unbound CCs are ignored.
#[derive(Debug, Default, Clone)]
pub struct CcTargets {
    vcf: Option<usize>,
    lfo: Option<usize>,
    lfo_depth: Option<usize>,
    amp_env: Option<usize>,
}

impl CcTargets {
    fn resolve(modules: &[ModuleInfo]) -> Self {
        let module_of = |kind: &str| -> Option<usize> {
            modules
                .iter()
                .find(|m| m.kind == kind)
                .map(|m| m.module as usize)
        };
        // Prefer the first envelope named like amp when several exist.
        let amp_env = modules
            .iter()
            .find(|m| m.kind == "envelope" && m.name.to_ascii_lowercase().contains("amp"))
            .or_else(|| modules.iter().find(|m| m.kind == "envelope"))
            .map(|m| m.module as usize);
        let targets = Self {
            vcf: module_of("vcf"),
            lfo: module_of("lfo"),
            lfo_depth: module_of("attenuverter"),
            amp_env,
        };
        debug!(?targets, "resolved MIDI CC targets");
        targets
    }

    fn try_resolve(modules: &[ModuleInfo]) -> Result<Self, String> {
        Ok(Self::resolve(modules))
    }
}

fn apply_control_change(
    synth: &mut Synth,
    targets: &CcTargets,
    calibration: &mut OutputCalibration,
    controller: u8,
    value: u8,
) {
    let normalized = value as f32 / 127.0;

    match controller {
        74 => {
            if let Some(vcf) = targets.vcf {
                let cutoff = 20.0 * (500.0f32).powf(normalized);
                synth.set_param(vcf, Vcf::PARAM_CUTOFF, cutoff);
            }
        }
        71 => {
            if let Some(vcf) = targets.vcf {
                synth.set_param(vcf, Vcf::PARAM_RESONANCE, normalized);
            }
        }
        1 => {
            if let Some(depth) = targets.lfo_depth {
                synth.set_param(depth, Attenuverter::PARAM_SCALE, normalized * 2.0);
            }
        }
        76 => {
            if let Some(lfo) = targets.lfo {
                let rate = 0.1 * (200.0f32).powf(normalized);
                synth.set_param(lfo, Lfo::PARAM_RATE, rate);
            }
        }
        72 => {
            if let Some(env) = targets.amp_env {
                let release = 0.01 + normalized * 5.0;
                synth.set_param(env, Envelope::PARAM_RELEASE, release);
            }
        }
        20 => {
            calibration.pitch_scale = normalized;
            synth.set_output_calibration(
                Synth::CHANNEL_PITCH_CV,
                calibration.pitch_scale,
                calibration.pitch_offset,
            );
        }
        21 => {
            calibration.pitch_offset = normalized * 2.0 - 1.0;
            synth.set_output_calibration(
                Synth::CHANNEL_PITCH_CV,
                calibration.pitch_scale,
                calibration.pitch_offset,
            );
        }
        22 => {
            calibration.gate_scale = normalized;
            synth.set_output_calibration(
                Synth::CHANNEL_GATE_CV,
                calibration.gate_scale,
                calibration.gate_offset,
            );
        }
        23 => {
            calibration.gate_offset = normalized * 2.0 - 1.0;
            synth.set_output_calibration(
                Synth::CHANNEL_GATE_CV,
                calibration.gate_scale,
                calibration.gate_offset,
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wmidi::{Note, Velocity};

    fn synth_with_a_note_held() -> Synth {
        let mut synth = Synth::new(48_000.0);
        synth.note_on(
            Note::try_from(69).unwrap(),
            Velocity::try_from(100).unwrap(),
        );
        synth
    }

    #[test]
    fn routes_each_physical_channel_to_its_own_synth_output_channel() {
        let mut synth = synth_with_a_note_held();
        let num_channels = 4;
        let frames = 8;
        synth.process_block(frames);
        let mut interleaved = vec![0.0f32; frames * num_channels];
        render_interleaved(&synth, &mut interleaved, num_channels, frames);

        let mut reference = synth_with_a_note_held();
        reference.process_block(frames);

        for (i, frame) in interleaved.chunks_exact(num_channels).enumerate() {
            assert_eq!(frame[0], reference.output_channel(0)[i]);
            assert_eq!(frame[1], reference.output_channel(1)[i]);
            assert_eq!(frame[2], reference.output_channel(2)[i]);
            assert_eq!(frame[3], 0.0);
        }
    }

    #[test]
    fn channels_beyond_what_the_synth_defines_are_silent() {
        let mut synth = synth_with_a_note_held();
        let num_channels = 6;
        let frames = 4;
        synth.process_block(frames);
        let mut interleaved = vec![1.0f32; frames * num_channels];
        render_interleaved(&synth, &mut interleaved, num_channels, frames);

        for frame in interleaved.chunks_exact(num_channels) {
            for &sample in &frame[Synth::NUM_OUTPUT_CHANNELS..] {
                assert_eq!(sample, 0.0);
            }
        }
    }

    #[test]
    fn process_frame_writes_every_sample_without_capture() {
        let mut synth = synth_with_a_note_held();
        let capture: SharedCapture = Arc::new(Mutex::new(None));
        let mut scratch = [[0.0f32; MAX_BLOCK_FRAMES]; NUM_EXTERNAL_INPUT_CHANNELS];
        let num_channels = 1;
        let total_frames = MAX_BLOCK_FRAMES * 2 + 17;
        let mut output = vec![f32::NAN; total_frames];

        process_frame(
            &mut output,
            &mut synth,
            num_channels,
            &capture,
            &mut scratch,
        );

        assert!(
            output.iter().all(|s| !s.is_nan()),
            "process_frame left samples unwritten"
        );
    }

    #[test]
    fn default_patch_declares_audio_in_and_out_endpoints() {
        let synth = Synth::new(48_000.0);
        let ids: Vec<&str> = synth.describe_io().iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&IO_ENDPOINT_AUDIO_OUT));
        assert!(ids.contains(&IO_ENDPOINT_AUDIO_IN));
        for ep in synth.describe_io() {
            assert_eq!(ep.device, dsp::IO_DEVICE_DEFAULT);
        }
    }
}
