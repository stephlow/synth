//! Hardware audio IO realized from patch-owned [`dsp::IoEndpoint`] bindings.
//!
//! Device open/close stays on the IO control thread. Steady-state audio:
//! cpal input pushes into small SPSC rings; the **output callback** owns
//! DSP processing (no worker prefill, no large output ring).

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig, SupportedStreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};
use dsp::{
    IoDirection, IoEndpoint, IoEndpointStatus, MAX_BLOCK_FRAMES, IO_DEVICE_DEFAULT,
    IO_ENDPOINT_AUDIO_IN, IO_ENDPOINT_AUDIO_OUT,
};
use tracing::{error, info, warn};

/// How many external-input channels are captured — must match the patch /
/// `Synth` external-input layout (2 today).
pub const NUM_EXTERNAL_INPUT_CHANNELS: usize = 2;

/// Ring capacity for **input** only — absorbs scheduling jitter between the
/// independent input and output callbacks. Sized for a few device periods,
/// not as a multi-ten-ms latency sink for pre-rendered audio.
pub const CAPTURE_RING_FRAMES: usize = MAX_BLOCK_FRAMES * 2;

/// Shared capture consumers the output callback pops from. Replaced as a
/// unit when the input device is reopened. Callback uses **`try_lock` only**.
pub type SharedCapture = Arc<Mutex<Option<[Consumer<f32>; NUM_EXTERNAL_INPUT_CHANNELS]>>>;

/// Host audio device summary for control surfaces.
#[derive(Clone, Debug)]
pub struct HostAudioDevice {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    pub max_input_channels: u32,
    pub max_output_channels: u32,
}

/// Enumerate all host devices, merging duplex names into one entry when
/// the same name appears as both input and output.
pub fn list_audio_devices() -> Vec<HostAudioDevice> {
    let host = cpal::default_host();
    let mut by_name: std::collections::BTreeMap<String, HostAudioDevice> =
        std::collections::BTreeMap::new();

    if let Ok(devices) = host.input_devices() {
        for device in devices {
            let name = device.name().unwrap_or_else(|_| "<unnamed>".into());
            let max_in = device
                .default_input_config()
                .map(|c| c.channels() as u32)
                .unwrap_or(0);
            let entry = by_name.entry(name.clone()).or_insert_with(|| HostAudioDevice {
                name: name.clone(),
                is_input: false,
                is_output: false,
                max_input_channels: 0,
                max_output_channels: 0,
            });
            entry.is_input = true;
            entry.max_input_channels = max_in;
        }
    }

    if let Ok(devices) = host.output_devices() {
        for device in devices {
            let name = device.name().unwrap_or_else(|_| "<unnamed>".into());
            let max_out = device
                .default_output_config()
                .map(|c| c.channels() as u32)
                .unwrap_or(0);
            let entry = by_name.entry(name.clone()).or_insert_with(|| HostAudioDevice {
                name: name.clone(),
                is_input: false,
                is_output: false,
                max_input_channels: 0,
                max_output_channels: 0,
            });
            entry.is_output = true;
            entry.max_output_channels = max_out;
        }
    }

    by_name.into_values().collect()
}

fn is_default_device_name(name: &str) -> bool {
    name.is_empty() || name == IO_DEVICE_DEFAULT
}

/// Resolve a patch device binding to a cpal [`Device`].
pub fn resolve_device(kind: IoDirection, device_name: &str) -> Result<Device, anyhow::Error> {
    let host = cpal::default_host();
    match kind {
        IoDirection::Input => {
            if is_default_device_name(device_name) {
                host.default_input_device()
                    .ok_or_else(|| anyhow::anyhow!("no default input device"))
            } else {
                find_device(host.input_devices()?, device_name)
            }
        }
        IoDirection::Output => {
            if is_default_device_name(device_name) {
                host.default_output_device()
                    .ok_or_else(|| anyhow::anyhow!("no default output device"))
            } else {
                find_device(host.output_devices()?, device_name)
            }
        }
    }
}

fn find_device(
    devices: impl Iterator<Item = Device>,
    want: &str,
) -> Result<Device, anyhow::Error> {
    for device in devices {
        if device.name().ok().as_deref() == Some(want) {
            return Ok(device);
        }
    }
    Err(anyhow::anyhow!("audio device not found: {want}"))
}

/// Look up an endpoint by id from the patch list.
pub fn endpoint_by_id<'a>(endpoints: &'a [IoEndpoint], id: &str) -> Option<&'a IoEndpoint> {
    endpoints.iter().find(|e| e.id == id)
}

/// Open the default output config for the patch's audio_out binding.
pub fn open_output_config(
    endpoints: &[IoEndpoint],
) -> Result<(Device, SupportedStreamConfig), anyhow::Error> {
    let ep = endpoint_by_id(endpoints, IO_ENDPOINT_AUDIO_OUT)
        .ok_or_else(|| anyhow::anyhow!("patch has no audio_out endpoint"))?;
    let device = resolve_device(IoDirection::Output, &ep.device)?;
    let config = device.default_output_config()?;
    info!(
        device = %device.name().unwrap_or_else(|_| "<unnamed>".into()),
        ?config,
        binding = %ep.device,
        "output device resolved from patch"
    );
    Ok((device, config))
}

/// Build an input stream for the patch's audio_in binding.
pub fn open_input_stream(
    endpoints: &[IoEndpoint],
) -> Result<(Stream, [Consumer<f32>; NUM_EXTERNAL_INPUT_CHANNELS], String), anyhow::Error> {
    let ep = endpoint_by_id(endpoints, IO_ENDPOINT_AUDIO_IN)
        .ok_or_else(|| anyhow::anyhow!("patch has no audio_in endpoint"))?;
    let device = resolve_device(IoDirection::Input, &ep.device)?;
    let resolved = device.name().unwrap_or_else(|_| "<unnamed>".into());
    let config = device.default_input_config()?;
    info!(device = %resolved, ?config, binding = %ep.device, "input device resolved from patch");

    let in_channels = config.channels() as usize;
    let offset = ep.device_channel_offset;
    let (producer_left, consumer_left) = RingBuffer::<f32>::new(CAPTURE_RING_FRAMES);
    let (producer_right, consumer_right) = RingBuffer::<f32>::new(CAPTURE_RING_FRAMES);
    let mut producers = [producer_left, producer_right];

    let err_fn = |err| error!(%err, "input sound stream error");
    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                push_captured_input(data, in_channels, offset, &mut producers);
            },
            err_fn,
            None,
        )?,
        other => {
            return Err(anyhow::anyhow!(
                "unsupported input sample format '{other}' (only F32 is supported)"
            ))
        }
    };
    stream.play()?;
    Ok((stream, [consumer_left, consumer_right], resolved))
}

fn push_captured_input(
    data: &[f32],
    in_channels: usize,
    channel_offset: usize,
    producers: &mut [Producer<f32>; NUM_EXTERNAL_INPUT_CHANNELS],
) {
    let ch = in_channels.max(1);
    for frame in data.chunks(ch) {
        let left = frame.get(channel_offset).copied().unwrap_or(0.0);
        let right = frame
            .get(channel_offset + 1)
            .copied()
            .unwrap_or(left);
        let _ = producers[0].push(left);
        let _ = producers[1].push(right);
    }
}

/// Build an output stream whose callback runs `data_callback` (DSP).
/// The callback must be RT-safe (no blocking locks, no alloc).
pub fn open_output_process_stream<T, F>(
    device: &Device,
    config: &StreamConfig,
    mut data_callback: F,
) -> Result<Stream, anyhow::Error>
where
    T: SizedSample + FromSample<f32>,
    F: FnMut(&mut [T]) + Send + 'static,
{
    let err_fn = |err| error!(%err, "output sound stream error");
    let stream = device.build_output_stream(
        config,
        move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
            data_callback(output);
        },
        err_fn,
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

/// Dispatch on sample format. `build` is called once per format with a
/// typed stream builder; callers typically clone an `Arc` into each arm.
pub fn open_output_for_format(
    _device: &Device,
    supported: &SupportedStreamConfig,
    mut build: impl FnMut(SampleFormat, &StreamConfig) -> Result<Stream, anyhow::Error>,
) -> Result<Stream, anyhow::Error> {
    let config: StreamConfig = supported.clone().into();
    build(supported.sample_format(), &config)
}

/// Build status rows for StreamState from patch endpoints + connection flags.
pub fn status_from_endpoints(
    endpoints: &[IoEndpoint],
    out_connected: bool,
    out_resolved: &str,
    in_connected: bool,
    in_resolved: &str,
) -> Vec<IoEndpointStatus> {
    endpoints
        .iter()
        .map(|ep| {
            let (connected, device_name) = match ep.id {
                id if id == IO_ENDPOINT_AUDIO_OUT => (out_connected, out_resolved.to_string()),
                id if id == IO_ENDPOINT_AUDIO_IN => (in_connected, in_resolved.to_string()),
                _ => (false, ep.device.clone()),
            };
            IoEndpointStatus {
                endpoint_id: ep.id.to_string(),
                device_name,
                connected,
                channels: ep.channels,
            }
        })
        .collect()
}

/// Publish a warning when optional input fails at startup.
pub fn warn_optional_input(err: &anyhow::Error) {
    warn!(%err, "no audio input available; continuing without external audio/CV input");
}
