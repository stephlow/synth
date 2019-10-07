extern crate coremidi;
extern crate cpal;
extern crate failure;

use std::sync::mpsc;

use cpal::traits::{DeviceTrait, EventLoopTrait, HostTrait};

mod oscillator;

const BASE_FREQUENCY: f32 = 440.0;

#[derive(Copy)]
enum Gate {
    High,
    Low,
}

impl Clone for Gate {
    fn clone(&self) -> Gate {
        *self
    }
}

fn main() -> Result<(), failure::Error> {
    let mut frequency = BASE_FREQUENCY;
    let mut velocity: f32 = 0.0;
    let mut gate = Gate::Low;

    let (fequency_sender, frequency_receiver) = mpsc::channel();
    let (velocity_sender, velocity_receiver) = mpsc::channel();
    let (gate_sender, gate_receiver) = mpsc::channel();

    let client = coremidi::Client::new("example-client").unwrap();

    let callback = move |packet_list: &coremidi::PacketList| {
        let frequency_sender = fequency_sender.clone();
        let velocity_sender = velocity_sender.clone();
        let gate_sender = gate_sender.clone();
        for packet in packet_list.iter() {
            match packet.data() {
                // Note off
                [128, _note, _velocity] => {
                    gate_sender.send(Gate::Low).unwrap();
                }
                // Note on
                [144, note, velocity] => {
                    let freq = BASE_FREQUENCY * (2.0f32).powf(f32::from(*note as i8 - 69) / 12.0);
                    frequency_sender.send(freq).unwrap();
                    let vel = f32::from(*velocity) / 128.0;
                    velocity_sender.send(vel).unwrap();
                    gate_sender.send(Gate::High).unwrap();
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

    let mut next_value = |frequency, gate, velocity| {
        sample_clock = (sample_clock + 1.0) % sample_rate;
        match gate {
            Gate::High => oscillator::sine(frequency, sample_clock, sample_rate) * velocity,
            Gate::Low => 0.0,
        }
    };

    event_loop.run(move |id, result| {
        match frequency_receiver.try_recv() {
            Ok(next_frequency) => frequency = next_frequency,
            _ => (),
        };

        match gate_receiver.try_recv() {
            Ok(next_gate) => gate = next_gate,
            _ => (),
        }

        match velocity_receiver.try_recv() {
            Ok(next_velocity) => velocity = next_velocity,
            _ => (),
        }

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
                    let value = ((next_value(frequency, gate, velocity) * 0.5 + 0.5)
                        * std::u16::MAX as f32) as u16;
                    for out in sample.iter_mut() {
                        *out = value;
                    }
                }
            }
            cpal::StreamData::Output {
                buffer: cpal::UnknownTypeOutputBuffer::I16(mut buffer),
            } => {
                for sample in buffer.chunks_mut(format.channels as usize) {
                    let value =
                        (next_value(frequency, gate, velocity) * std::i16::MAX as f32) as i16;
                    for out in sample.iter_mut() {
                        *out = value;
                    }
                }
            }
            cpal::StreamData::Output {
                buffer: cpal::UnknownTypeOutputBuffer::F32(mut buffer),
            } => {
                for sample in buffer.chunks_mut(format.channels as usize) {
                    let value = next_value(frequency, gate, velocity);
                    for out in sample.iter_mut() {
                        *out = value;
                    }
                }
            }
            _ => (),
        }
    });
}
