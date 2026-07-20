use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SizedSample,
};
use cpal::{FromSample, Sample};
use rtrb::{Consumer, Producer, RingBuffer};
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::sync::mpsc::{self, Receiver};
use synth::midi::{setup_midi, MidiMessage};
use synth::modules::{Envelope, Lfo, Vcf};
use synth::{rt, AppMessage, Synth, MAX_BLOCK_FRAMES};

/// How many external-input channels are captured from hardware and fed
/// into the patch — must match `synth::synth::NUM_EXTERNAL_INPUT_CHANNELS`
/// (not public; kept in sync by convention, since it's just 2 today).
const NUM_EXTERNAL_INPUT_CHANNELS: usize = 2;
/// Ring buffer capacity between the input and output audio callbacks:
/// generous relative to a typical block so occasional scheduling jitter
/// between the two independent callbacks doesn't cause audible gaps.
const RING_BUFFER_FRAMES: usize = MAX_BLOCK_FRAMES * 4;

fn main() -> Result<(), Box<dyn Error>> {
    let (tx, rx) = mpsc::channel::<AppMessage>();

    // _conn_in needs to be a named parameter, because it needs to be kept alive until the end of the scope
    let _conn_in = setup_midi(tx)?;

    // Hardware audio/CV input is optional: if no input device is
    // available (or it fails to open), we fall back to running with
    // silent external-input channels rather than failing to start.
    let (consumers, _input_stream) = match setup_input_stream() {
        Ok((stream, consumers)) => (Some(consumers), Some(stream)),
        Err(err) => {
            eprintln!("No audio input available ({err}); continuing without external audio/CV input.");
            (None, None)
        }
    };

    let stream = stream_setup_for(rx, consumers)?;
    stream.play()?;

    // Keep the process alive while the audio and MIDI threads run in the
    // background; the streams are driven entirely by their own callbacks.
    loop {
        std::thread::park();
    }
}

pub fn stream_setup_for(
    rx: Receiver<AppMessage>,
    input: Option<[Consumer<f32>; NUM_EXTERNAL_INPUT_CHANNELS]>,
) -> Result<cpal::Stream, anyhow::Error> {
    let (_host, device, config) = host_device_setup()?;

    match config.sample_format() {
        cpal::SampleFormat::I8 => make_stream::<i8>(&device, &config.into(), rx, input),
        cpal::SampleFormat::I16 => make_stream::<i16>(&device, &config.into(), rx, input),
        cpal::SampleFormat::I32 => make_stream::<i32>(&device, &config.into(), rx, input),
        cpal::SampleFormat::I64 => make_stream::<i64>(&device, &config.into(), rx, input),
        cpal::SampleFormat::U8 => make_stream::<u8>(&device, &config.into(), rx, input),
        cpal::SampleFormat::U16 => make_stream::<u16>(&device, &config.into(), rx, input),
        cpal::SampleFormat::U32 => make_stream::<u32>(&device, &config.into(), rx, input),
        cpal::SampleFormat::U64 => make_stream::<u64>(&device, &config.into(), rx, input),
        cpal::SampleFormat::F32 => make_stream::<f32>(&device, &config.into(), rx, input),
        cpal::SampleFormat::F64 => make_stream::<f64>(&device, &config.into(), rx, input),
        sample_format => Err(anyhow::Error::msg(format!(
            "Unsupported sample format '{sample_format}'"
        ))),
    }
}

pub fn host_device_setup(
) -> Result<(cpal::Host, cpal::Device, cpal::SupportedStreamConfig), anyhow::Error> {
    let host = cpal::default_host();
    let device = select_device(host.output_devices()?, "output")?;
    println!("Output device : {}", device.name()?);

    let config = device.default_output_config()?;
    println!("Default output config : {:?}", config);

    Ok((host, device, config))
}

/// Picks an output or input device generically — any audio interface,
/// not a hardcoded one: auto-selects if exactly one is available,
/// otherwise lists them and prompts via stdin. Mirrors
/// `midi::setup_midi`'s existing interactive port-selection pattern, so
/// picking "the ES-8" vs "the 2i2" (or any other interface) for either
/// direction is just a matter of what you type at startup.
fn select_device(
    devices: impl Iterator<Item = cpal::Device>,
    kind: &str,
) -> Result<cpal::Device, anyhow::Error> {
    let devices: Vec<cpal::Device> = devices.collect();
    match devices.len() {
        0 => Err(anyhow::Error::msg(format!("No {kind} device available"))),
        1 => {
            let name = devices[0].name().unwrap_or_else(|_| "<unnamed>".into());
            println!("Choosing the only available {kind} device: {name}");
            Ok(devices.into_iter().next().unwrap())
        }
        _ => {
            println!("\nAvailable {kind} devices:");
            for (i, d) in devices.iter().enumerate() {
                let name = d.name().unwrap_or_else(|_| "<unnamed>".into());
                println!("{i}: {name}");
            }
            print!("Please select {kind} device: ");
            stdout().flush()?;
            let mut input = String::new();
            stdin().read_line(&mut input)?;
            let index: usize = input.trim().parse()?;
            devices
                .into_iter()
                .nth(index)
                .ok_or_else(|| anyhow::Error::msg(format!("invalid {kind} device selected")))
        }
    }
}

/// Builds the (optional) hardware audio/CV input stream: captures
/// whatever the default input device provides and pushes it, deinterleaved
/// into up to `NUM_EXTERNAL_INPUT_CHANNELS` channels, through a pair of
/// lock-free single-producer/single-consumer ring buffers (one per
/// channel) to the output callback — audio callbacks must never
/// block/lock, and the input and output streams are independent
/// callbacks with independent timing, so a wait-free handoff is the
/// right tool here. A mono input device duplicates its one channel to
/// both; a device with more than 2 channels only captures the first 2.
fn setup_input_stream() -> Result<(cpal::Stream, [Consumer<f32>; NUM_EXTERNAL_INPUT_CHANNELS]), anyhow::Error> {
    let host = cpal::default_host();
    let device = select_device(host.input_devices()?, "input")?;
    println!("Input device : {}", device.name()?);

    let config = device.default_input_config()?;
    println!("Default input config : {:?}", config);
    let in_channels = config.channels() as usize;

    let (producer_left, consumer_left) = RingBuffer::<f32>::new(RING_BUFFER_FRAMES);
    let (producer_right, consumer_right) = RingBuffer::<f32>::new(RING_BUFFER_FRAMES);
    let mut producers = [producer_left, producer_right];

    let err_fn = |err| eprintln!("Error building input sound stream: {}", err);
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                push_captured_input(data, in_channels, &mut producers)
            },
            err_fn,
            None,
        )?,
        other => {
            return Err(anyhow::Error::msg(format!(
                "Unsupported input sample format '{other}' (only F32 input is currently supported)"
            )))
        }
    };

    Ok((stream, [consumer_left, consumer_right]))
}

/// Deinterleaves a captured input buffer and pushes each frame's L/R
/// samples into their ring buffer. Uses `try_push`-style semantics
/// (`Producer::push` never blocks): if the output side hasn't drained
/// enough yet, a sample is simply dropped rather than stalling this
/// (real-time) callback.
fn push_captured_input(
    data: &[f32],
    in_channels: usize,
    producers: &mut [Producer<f32>; NUM_EXTERNAL_INPUT_CHANNELS],
) {
    for frame in data.chunks(in_channels.max(1)) {
        let left = frame.first().copied().unwrap_or(0.0);
        let right = frame.get(1).copied().unwrap_or(left);
        let _ = producers[0].push(left);
        let _ = producers[1].push(right);
    }
}

/// Live-tunable calibration for the master hardware-out pitch/gate CV
/// channels (see `Io`'s doc comment for how to derive the right numbers
/// for real hardware, e.g. `pitch_scale = 0.1` for a ±10V-scale
/// DC-coupled interface). Tracked here (not read back from `Synth`) so a
/// scale-only or offset-only CC can update just its half of the pair
/// without needing a getter on `Synth::set_output_calibration`.
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

/// Maps a MIDI CC controller number + 0..127 value onto a `Synth`
/// parameter change. This is the "patch bay of the control surface":
/// extend this list to wire up more knobs.
fn apply_control_change(
    synth: &mut Synth,
    calibration: &mut OutputCalibration,
    controller: u8,
    value: u8,
) {
    let normalized = value as f32 / 127.0; // 0..1

    match controller {
        74 => {
            // Filter cutoff: 20 Hz .. ~10 kHz, exponential feel.
            let cutoff = 20.0 * (500.0f32).powf(normalized);
            synth.set_param(synth.modules.vcf, Vcf::PARAM_CUTOFF, cutoff);
        }
        71 => {
            // Filter resonance.
            synth.set_param(synth.modules.vcf, Vcf::PARAM_RESONANCE, normalized);
        }
        1 => {
            // Mod wheel: LFO-to-cutoff depth, in octaves.
            synth.set_param(
                synth.modules.lfo_depth,
                synth::modules::Attenuverter::PARAM_SCALE,
                normalized * 2.0,
            );
        }
        76 => {
            // LFO rate: 0.1 .. ~20 Hz, exponential feel.
            let rate = 0.1 * (200.0f32).powf(normalized);
            synth.set_param(synth.modules.lfo, Lfo::PARAM_RATE, rate);
        }
        72 => {
            // Amp envelope release time: 0.01 .. ~5s.
            let release = 0.01 + normalized * 5.0;
            synth.set_param(synth.modules.amp_env, Envelope::PARAM_RELEASE, release);
        }
        // Master output calibration — see `OutputCalibration`'s doc
        // comment and `Io`'s worked examples for how to derive these for
        // real hardware (e.g. an Expert Sleepers ES-8).
        20 => {
            calibration.pitch_scale = normalized; // 0..1
            synth.set_output_calibration(Synth::CHANNEL_PITCH_CV, calibration.pitch_scale, calibration.pitch_offset);
        }
        21 => {
            calibration.pitch_offset = normalized * 2.0 - 1.0; // -1..1
            synth.set_output_calibration(Synth::CHANNEL_PITCH_CV, calibration.pitch_scale, calibration.pitch_offset);
        }
        22 => {
            calibration.gate_scale = normalized; // 0..1
            synth.set_output_calibration(Synth::CHANNEL_GATE_CV, calibration.gate_scale, calibration.gate_offset);
        }
        23 => {
            calibration.gate_offset = normalized * 2.0 - 1.0; // -1..1
            synth.set_output_calibration(Synth::CHANNEL_GATE_CV, calibration.gate_scale, calibration.gate_offset);
        }
        _ => {}
    }
}

pub fn make_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    rx: Receiver<AppMessage>,
    input: Option<[Consumer<f32>; NUM_EXTERNAL_INPUT_CHANNELS]>,
) -> Result<cpal::Stream, anyhow::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let num_channels = config.channels as usize;
    let mut synth = Synth::new(config.sample_rate.0 as f32);
    let mut calibration = OutputCalibration::default();
    let mut input = input;
    // Reused every callback: popped-from-the-ring-buffer external input,
    // staged before each block — never (re)allocated on the audio thread.
    let mut external_scratch = [[0.0f32; MAX_BLOCK_FRAMES]; NUM_EXTERNAL_INPUT_CHANNELS];
    let err_fn = |err| eprintln!("Error building output sound stream: {}", err);

    let stream = device.build_output_stream(
        config,
        move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
            // Denormal tails (decaying envelopes, filter/feedback settling
            // toward silence) can otherwise stall the CPU badly enough to
            // miss the callback deadline; see `rt::flush_denormals_to_zero`.
            rt::flush_denormals_to_zero();

            while let Ok(app_message) = rx.try_recv() {
                match app_message {
                    AppMessage::MidiMessage(midi_message) => match midi_message {
                        MidiMessage::NoteOn(note, velocity) => synth.note_on(note, velocity),
                        MidiMessage::NoteOff(note, velocity) => synth.note_off(note, velocity),
                        MidiMessage::ProgramChange(_program) => {}
                        MidiMessage::ControlChange(controller, value) => {
                            apply_control_change(&mut synth, &mut calibration, controller, value)
                        }
                    },
                }
            }
            process_frame(
                output,
                &mut synth,
                num_channels,
                input.as_mut(),
                &mut external_scratch,
            )
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

/// Renders `output` (interleaved across `num_channels`) from the synth,
/// one block of up to `MAX_BLOCK_FRAMES` frames at a time — the graph
/// engine is built for a fixed maximum block size, so a callback buffer
/// bigger than that is simply split into sub-blocks here rather than
/// requiring the engine to grow its (preallocated) scratch storage.
///
/// Each physical output channel maps 1:1 to `Synth::output_channel`:
/// channel 0 is the mixed synth audio, 1/2 are pitch/gate CV-out, and
/// any channel beyond what the synth defines reads as silence — the
/// old "duplicate mono to every channel" behavior is gone, since
/// blindly copying a CV signal onto every output would be actively
/// harmful when other channels are wired to real Eurorack gear.
fn process_frame<SampleType>(
    output: &mut [SampleType],
    synth: &mut Synth,
    num_channels: usize,
    mut input: Option<&mut [Consumer<f32>; NUM_EXTERNAL_INPUT_CHANNELS]>,
    external_scratch: &mut [[f32; MAX_BLOCK_FRAMES]; NUM_EXTERNAL_INPUT_CHANNELS],
) where
    SampleType: Sample + FromSample<f32>,
{
    let total_frames = output.len() / num_channels;
    let mut done = 0;
    while done < total_frames {
        let chunk_frames = (total_frames - done).min(MAX_BLOCK_FRAMES);

        // Pull whatever external input is available for this chunk;
        // missing samples (input not yet caught up, or no input device)
        // read as silence rather than blocking this callback.
        for channel in 0..NUM_EXTERNAL_INPUT_CHANNELS {
            let scratch = &mut external_scratch[channel][..chunk_frames];
            match input.as_mut() {
                Some(consumers) => {
                    for sample in scratch.iter_mut() {
                        *sample = consumers[channel].pop().unwrap_or(0.0);
                    }
                }
                None => scratch.fill(0.0),
            }
            synth.write_external_input(channel, scratch);
        }

        synth.process_block(chunk_frames);

        for i in 0..chunk_frames {
            let frame_start = (done + i) * num_channels;
            for (channel, out) in output[frame_start..frame_start + num_channels].iter_mut().enumerate() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use wmidi::{Note, Velocity};

    fn synth_with_a_note_held() -> Synth {
        let mut synth = Synth::new(48_000.0);
        synth.note_on(Note::try_from(69).unwrap(), Velocity::try_from(100).unwrap());
        synth
    }

    fn no_external_scratch() -> [[f32; MAX_BLOCK_FRAMES]; NUM_EXTERNAL_INPUT_CHANNELS] {
        [[0.0; MAX_BLOCK_FRAMES]; NUM_EXTERNAL_INPUT_CHANNELS]
    }

    #[test]
    fn routes_each_physical_channel_to_its_own_synth_output_channel() {
        let mut synth = synth_with_a_note_held();
        let num_channels = 4;
        let frames = 8;
        let mut output = vec![0.0f32; frames * num_channels];
        let mut scratch = no_external_scratch();

        process_frame(&mut output, &mut synth, num_channels, None, &mut scratch);

        // Re-render the same held note separately and compare against
        // Synth's own per-channel output directly, frame by frame.
        let mut reference = synth_with_a_note_held();
        reference.process_block(frames);

        for (i, frame) in output.chunks_exact(num_channels).enumerate() {
            assert_eq!(frame[0], reference.output_channel(0)[i], "channel 0 (audio) mismatch");
            assert_eq!(frame[1], reference.output_channel(1)[i], "channel 1 (pitch CV) mismatch");
            assert_eq!(frame[2], reference.output_channel(2)[i], "channel 2 (gate CV) mismatch");
            assert_eq!(frame[3], 0.0, "channel 3 (aux) should be silent");
        }
    }

    #[test]
    fn channels_beyond_what_the_synth_defines_are_silent() {
        let mut synth = synth_with_a_note_held();
        let num_channels = 6; // more than Synth::NUM_OUTPUT_CHANNELS
        let frames = 4;
        let mut output = vec![1.0f32; frames * num_channels]; // sentinel, not 0.0
        let mut scratch = no_external_scratch();

        process_frame(&mut output, &mut synth, num_channels, None, &mut scratch);

        for frame in output.chunks_exact(num_channels) {
            for &sample in &frame[Synth::NUM_OUTPUT_CHANNELS..] {
                assert_eq!(sample, 0.0);
            }
        }
    }

    #[test]
    fn chunking_into_sub_blocks_does_not_change_the_rendered_signal() {
        // process_frame splits a callback buffer into sub-blocks of at
        // most MAX_BLOCK_FRAMES. The result should be identical no
        // matter how many chunks that takes — otherwise there's an
        // off-by-one/dropped-or-duplicated-sample bug at the boundaries.
        let num_channels = 1;
        let total_frames = MAX_BLOCK_FRAMES * 2 + 17; // forces multiple sub-blocks + a remainder

        let mut synth = synth_with_a_note_held();
        let mut output = vec![0.0f32; total_frames];
        let mut scratch = no_external_scratch();
        process_frame(&mut output, &mut synth, num_channels, None, &mut scratch);

        // Compare against directly driving Synth block-by-block at the
        // same chunk size process_frame uses internally.
        let mut reference = synth_with_a_note_held();
        let mut expected = Vec::with_capacity(total_frames);
        let mut done = 0;
        while done < total_frames {
            let chunk = (total_frames - done).min(MAX_BLOCK_FRAMES);
            reference.process_block(chunk);
            expected.extend_from_slice(reference.output_channel(0));
            done += chunk;
        }

        assert_eq!(output, expected, "chunk granularity changed the rendered signal");
    }

    #[test]
    fn every_output_frame_is_written_when_the_buffer_exceeds_max_block_frames() {
        let mut synth = synth_with_a_note_held();
        let num_channels = 1;
        let total_frames = MAX_BLOCK_FRAMES * 2 + 17;
        let mut output = vec![f32::NAN; total_frames]; // sentinel: must all be overwritten
        let mut scratch = no_external_scratch();

        process_frame(&mut output, &mut synth, num_channels, None, &mut scratch);

        assert!(
            output.iter().all(|s| !s.is_nan()),
            "process_frame left {} of {total_frames} samples unwritten",
            output.iter().filter(|s| s.is_nan()).count()
        );
    }
}
