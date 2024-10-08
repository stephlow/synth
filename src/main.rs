use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SizedSample,
};
use cpal::{FromSample, Sample};
use midi::{setup_midi, MidiMessage};
use std::error::Error;
use std::sync::mpsc::{self, Receiver};
use synth::Synth;

mod adsr;
mod midi;
mod node;
mod oscillator;
mod synth;
mod voice;

pub enum AppMessage {
    MidiMessage(MidiMessage),
}

fn main() -> Result<(), Box<dyn Error>> {
    let (tx, rx) = mpsc::channel::<AppMessage>();

    // _conn_in needs to be a named parameter, because it needs to be kept alive until the end of the scope
    let _conn_in = setup_midi(tx)?;

    let stream = stream_setup_for(rx)?;
    stream.play()?;

    loop {
        // TODO:
        std::thread::sleep(std::time::Duration::from_millis(4000));
    }

    Ok(())
}

pub fn stream_setup_for(rx: Receiver<AppMessage>) -> Result<cpal::Stream, anyhow::Error>
where
{
    let (_host, device, config) = host_device_setup()?;

    match config.sample_format() {
        cpal::SampleFormat::I8 => make_stream::<i8>(&device, &config.into(), rx),
        cpal::SampleFormat::I16 => make_stream::<i16>(&device, &config.into(), rx),
        cpal::SampleFormat::I32 => make_stream::<i32>(&device, &config.into(), rx),
        cpal::SampleFormat::I64 => make_stream::<i64>(&device, &config.into(), rx),
        cpal::SampleFormat::U8 => make_stream::<u8>(&device, &config.into(), rx),
        cpal::SampleFormat::U16 => make_stream::<u16>(&device, &config.into(), rx),
        cpal::SampleFormat::U32 => make_stream::<u32>(&device, &config.into(), rx),
        cpal::SampleFormat::U64 => make_stream::<u64>(&device, &config.into(), rx),
        cpal::SampleFormat::F32 => make_stream::<f32>(&device, &config.into(), rx),
        cpal::SampleFormat::F64 => make_stream::<f64>(&device, &config.into(), rx),
        sample_format => Err(anyhow::Error::msg(format!(
            "Unsupported sample format '{sample_format}'"
        ))),
    }
}

pub fn host_device_setup(
) -> Result<(cpal::Host, cpal::Device, cpal::SupportedStreamConfig), anyhow::Error> {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::Error::msg("Default output device is not available"))?;
    println!("Output device : {}", device.name()?);

    let config = device.default_output_config()?;
    println!("Default output config : {:?}", config);

    Ok((host, device, config))
}

pub fn make_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    rx: Receiver<AppMessage>,
) -> Result<cpal::Stream, anyhow::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let num_channels = config.channels as usize;
    let mut synth = Synth::new(config.sample_rate.0 as f32);
    let err_fn = |err| eprintln!("Error building output sound stream: {}", err);

    let stream = device.build_output_stream(
        config,
        move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
            if let Ok(app_message) = rx.try_recv() {
                match app_message {
                    AppMessage::MidiMessage(midi_message) => match midi_message {
                        MidiMessage::NoteOn(note, velocity) => synth.note_on(note, velocity),
                        MidiMessage::NoteOff(note, velocity) => synth.note_off(note, velocity),
                    },
                }
            }
            process_frame(output, &mut synth, num_channels)
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn process_frame<SampleType>(output: &mut [SampleType], synth: &mut Synth, num_channels: usize)
where
    SampleType: Sample + FromSample<f32>,
{
    for frame in output.chunks_mut(num_channels) {
        let value: SampleType = SampleType::from_sample(synth.tick());

        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}
