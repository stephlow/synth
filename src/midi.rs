use crate::AppMessage;
use midir::{Ignore, MidiInput, MidiInputConnection};
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::sync::mpsc;
use wmidi::{Note, Velocity};

pub enum MidiMessage {
    NoteOn(Note, Velocity),
    NoteOff(Note, Velocity),
    ProgramChange(u8),
    ControlChange(u8, u8),
}

fn handle_midi_message(bytes: &[u8]) -> Option<MidiMessage> {
    wmidi::MidiMessage::try_from(bytes)
        .ok()
        .and_then(|message| match message {
            wmidi::MidiMessage::NoteOn(_, note, velocity) => {
                Some(MidiMessage::NoteOn(note, velocity))
            }
            wmidi::MidiMessage::NoteOff(_, note, velocity) => {
                Some(MidiMessage::NoteOff(note, velocity))
            }
            wmidi::MidiMessage::ProgramChange(_, program) => {
                Some(MidiMessage::ProgramChange(u8::from(program)))
            }
            wmidi::MidiMessage::ControlChange(_, controller, value) => {
                Some(MidiMessage::ControlChange(
                    u8::from(controller),
                    u8::from(value),
                ))
            }
            _ => None,
        })
}

pub fn setup_midi(tx: mpsc::Sender<AppMessage>) -> Result<MidiInputConnection<()>, Box<dyn Error>> {
    let mut midi_in = MidiInput::new("midir reading input")?;
    midi_in.ignore(Ignore::None);

    // Get an input port (read from console if multiple are available)
    let in_ports = midi_in.ports();
    let in_port = match in_ports.len() {
        0 => return Err("no input port found".into()),
        1 => {
            let port_name = midi_in.port_name(&in_ports[0]).unwrap();
            println!("Choosing the only available input port: {port_name}");
            &in_ports[0]
        }
        _ => {
            println!("\nAvailable input ports:");
            for (i, p) in in_ports.iter().enumerate() {
                let port_name = midi_in.port_name(p).unwrap();
                println!("{i}: {port_name}");
            }
            print!("Please select input port: ");
            stdout().flush()?;
            let mut input = String::new();
            stdin().read_line(&mut input)?;
            in_ports
                .get(input.trim().parse::<usize>()?)
                .ok_or("invalid input port selected")?
        }
    };

    println!("\nOpening connection");
    let in_port_name = midi_in.port_name(in_port)?;

    let conn_in = midi_in.connect(
        in_port,
        "midir-read-input",
        move |_, bytes, _| {
            if let Some(midi_message) = handle_midi_message(bytes) {
                let _ = tx.send(AppMessage::MidiMessage(midi_message));
            }
        },
        (),
    )?;

    println!("Connection open, reading input from '{in_port_name}'");

    Ok(conn_in)
}
