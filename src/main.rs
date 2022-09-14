extern crate coremidi;
extern crate cpal;
extern crate failure;
extern crate wmidi;

use coremidi::{Client, PacketList};
use oscillator::sine;
use std::sync::mpsc;
use wmidi::{MidiMessage, Note, Velocity};

use cpal::traits::{DeviceTrait, EventLoopTrait, HostTrait};
use cpal::{StreamData, UnknownTypeOutputBuffer};

mod gate;
mod oscillator;

use std::convert::TryFrom;

#[derive(Default, Clone, Debug)]
struct State {
    voices: Vec<(Note, Velocity)>,
}

impl State {
    fn add_voice(&mut self, note: Note, velocity: Velocity) {
        self.voices.push((note, velocity));
    }

    fn remove_voice(&mut self, note: Note, velocity: Velocity) {
        self.voices.retain(|(n, v)| !(n == &note && v == &velocity));
    }
}

fn handle_midi_message(state: &mut State, bytes: &[u8]) {
    if let Ok(message) = MidiMessage::try_from(bytes) {
        match message {
            MidiMessage::NoteOn(_, note, velocity) => {
                state.add_voice(note, velocity);
            }
            MidiMessage::NoteOff(_, note, velocity) => {
                state.remove_voice(note, velocity);
            }
            _ => {}
        }
    }
}

fn main() -> Result<(), failure::Error> {
    let mut state: State = Default::default();

    let (sender, receiver) = mpsc::channel();

    let client = Client::new("example-client").unwrap();

    let callback = move |packet_list: &PacketList| {
        for packet in packet_list.iter() {
            handle_midi_message(&mut state, packet.data());
            sender.send(state.clone()).unwrap();
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
    event_loop.play_stream(stream_id)?;

    let sample_rate = format.sample_rate.0 as f32;
    let mut sample_clock = 0f32;

    let mut next_value = |state: State| {
        sample_clock = (sample_clock + 1.0) % sample_rate;

        match state.voices.first() {
            Some((note, _frequency)) => sine(note.to_freq_f32(), sample_clock, sample_rate),
            _ => 0.0,
        }
    };

    event_loop.run(move |id, result| {
        let mut state: State = Default::default();

        if let Ok(new) = receiver.try_recv() {
            state = new;
        }

        let data = match result {
            Ok(data) => data,
            Err(err) => {
                eprintln!("an error occurred on stream {:?}: {}", id, err);
                return;
            }
        };

        match data {
            StreamData::Output {
                buffer: UnknownTypeOutputBuffer::U16(mut buffer),
            } => {
                for sample in buffer.chunks_mut(format.channels as usize) {
                    for out in sample.iter_mut() {
                        *out = next_value(state.clone()) as u16;
                    }
                }
            }
            StreamData::Output {
                buffer: UnknownTypeOutputBuffer::I16(mut buffer),
            } => {
                for sample in buffer.chunks_mut(format.channels as usize) {
                    for out in sample.iter_mut() {
                        *out = next_value(state.clone()) as i16;
                    }
                }
            }
            StreamData::Output {
                buffer: UnknownTypeOutputBuffer::F32(mut buffer),
            } => {
                for sample in buffer.chunks_mut(format.channels as usize) {
                    for out in sample.iter_mut() {
                        *out = next_value(state.clone()) as f32;
                    }
                }
            }
            _ => (),
        }
    });
}
