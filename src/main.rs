extern crate coremidi;
extern crate cpal;
extern crate failure;

use std::f32::consts::PI;
use std::sync::mpsc;

use cpal::traits::{DeviceTrait, EventLoopTrait, HostTrait};

const BASE_FREQUENCY: f32 = 440.0;

fn sine(frequency: f32, time: f32, sample_rate: f32) -> f32 {
    (frequency * time * 2.0 * PI / sample_rate).sin()
}

fn main() -> Result<(), failure::Error> {
    let (sender, receiver) = mpsc::channel();

    let client = coremidi::Client::new("example-client").unwrap();

    let callback = move |packet_list: &coremidi::PacketList| {
        let sender = sender.clone();
        for packet in packet_list.iter() {
            match packet.data() {
                // Note off
                [128, _note, _velocity] => (),
                // Note on
                [144, note, _velocity] => {
                    let freq = BASE_FREQUENCY * (2.0f32).powf(f32::from(*note as i8 - 69) / 12.0);
                    sender.send(freq).unwrap();
                }
                _ => (),
            }
        }
    };

    let _destination = client
        .virtual_destination("example-dest", callback)
        .unwrap();

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("failed to find a default output device");
    let format = device.default_output_format()?;
    let event_loop = host.event_loop();
    let stream_id = event_loop.build_output_stream(&device, &format)?;
    event_loop.play_stream(stream_id.clone())?;

    let sample_rate = format.sample_rate.0 as f32;
    let mut sample_clock = 0f32;

    let mut next_value = |frequency| {
        sample_clock = (sample_clock + 1.0) % sample_rate;

        sine(frequency, sample_clock, sample_rate)
    };

    let mut frequency = 440.0;

    event_loop.run(move |id, result| {
        match receiver.try_recv() {
            Ok(freq) => {
                frequency = freq;
            }
            _ => (),
        };

        let data = match result {
            Ok(data) => data,
            Err(err) => {
                eprintln!("an error occurred on stream {:?}: {}", id, err);
                return;
            }
        };

        match data {
            cpal::StreamData::Output {
                buffer: cpal::UnknownTypeOutputBuffer::U16(mut buffer),
            } => {
                for sample in buffer.chunks_mut(format.channels as usize) {
                    let value = ((next_value(frequency) * 0.5 + 0.5) * std::u16::MAX as f32) as u16;
                    for out in sample.iter_mut() {
                        *out = value;
                    }
                }
            }
            cpal::StreamData::Output {
                buffer: cpal::UnknownTypeOutputBuffer::I16(mut buffer),
            } => {
                for sample in buffer.chunks_mut(format.channels as usize) {
                    let value = (next_value(frequency) * std::i16::MAX as f32) as i16;
                    for out in sample.iter_mut() {
                        *out = value;
                    }
                }
            }
            cpal::StreamData::Output {
                buffer: cpal::UnknownTypeOutputBuffer::F32(mut buffer),
            } => {
                for sample in buffer.chunks_mut(format.channels as usize) {
                    let value = next_value(frequency);
                    for out in sample.iter_mut() {
                        *out = value;
                    }
                }
            }
            _ => (),
        }
    });
}
