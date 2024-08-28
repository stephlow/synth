use std::sync::mpsc::{self, Receiver};

use coremidi::{Client, PacketList};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SizedSample,
};
use cpal::{FromSample, Sample};
use oscillator::Oscillator;
use wmidi::{MidiMessage, Note};

mod oscillator;

static MIDI_CLIENT_NAME: &str = "example-client";
static MIDI_DESTINATION_NAME: &str = "example-dest";

pub enum MidiEvent {
    NoteOn(Note),
    NoteOff,
}

fn handle_midi_message(bytes: &[u8]) -> Option<MidiEvent> {
    if let Ok(message) = MidiMessage::try_from(bytes) {
        match message {
            MidiMessage::NoteOn(_, note, _velocity) => Some(MidiEvent::NoteOn(note)),
            MidiMessage::NoteOff(_, _note, _velocity) => Some(MidiEvent::NoteOff),
            _ => None,
        }
    } else {
        None
    }
}

fn main() -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel();

    let midi_client = Client::new(MIDI_CLIENT_NAME).unwrap();

    let tx = tx.clone();
    let callback = move |packet_list: &PacketList| {
        for packet in packet_list.iter() {
            if let Some(event) = handle_midi_message(packet.data()) {
                tx.send(event).unwrap();
            }
        }
    };

    let _destination = midi_client
        .virtual_destination(MIDI_DESTINATION_NAME, callback)
        .unwrap();

    let stream = stream_setup_for(rx)?;
    stream.play()?;

    loop {
        std::thread::sleep(std::time::Duration::from_millis(4000));
    }

    Ok(())
}

pub fn stream_setup_for(rx: Receiver<MidiEvent>) -> Result<cpal::Stream, anyhow::Error>
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
    rx: Receiver<MidiEvent>,
) -> Result<cpal::Stream, anyhow::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let num_channels = config.channels as usize;
    let mut oscillator = Oscillator::new(config.sample_rate.0 as f32);
    let err_fn = |err| eprintln!("Error building output sound stream: {}", err);

    let time_at_start = std::time::Instant::now();
    println!("Time at start: {:?}", time_at_start);

    let stream = device.build_output_stream(
        config,
        move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
            if let Ok(midi_event) = rx.try_recv() {
                match midi_event {
                    MidiEvent::NoteOn(note) => {
                        oscillator.note_on(note.to_freq_f32());
                    }
                    MidiEvent::NoteOff => {
                        oscillator.note_off();
                    }
                }
            }
            process_frame(output, &mut oscillator, num_channels)
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn process_frame<SampleType>(
    output: &mut [SampleType],
    oscillator: &mut Oscillator,
    num_channels: usize,
) where
    SampleType: Sample + FromSample<f32>,
{
    for frame in output.chunks_mut(num_channels) {
        let value: SampleType = SampleType::from_sample(oscillator.tick());

        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}
