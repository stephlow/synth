use midir::{Ignore, MidiInput, MidiInputConnection};
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::sync::mpsc;
use wmidi::{MidiMessage, Note, Velocity};

pub enum MidiEvent {
    NoteOn(Note, Velocity),
    NoteOff(Note, Velocity),
}

fn handle_midi_message(bytes: &[u8]) -> Option<MidiEvent> {
    if let Ok(message) = MidiMessage::try_from(bytes) {
        match message {
            MidiMessage::NoteOn(_, note, velocity) => Some(MidiEvent::NoteOn(note, velocity)),
            MidiMessage::NoteOff(_, note, velocity) => Some(MidiEvent::NoteOff(note, velocity)),
            _ => None,
        }
    } else {
        None
    }
}

pub fn setup_midi(tx: mpsc::Sender<MidiEvent>) -> Result<MidiInputConnection<()>, Box<dyn Error>> {
    let mut midi_in = MidiInput::new("midir reading input")?;
    midi_in.ignore(Ignore::None);

    // Get an input port (read from console if multiple are available)
    let in_ports = midi_in.ports();
    let in_port = match in_ports.len() {
        0 => return Err("no input port found".into()),
        1 => {
            println!(
                "Choosing the only available input port: {}",
                midi_in.port_name(&in_ports[0]).unwrap()
            );
            &in_ports[0]
        }
        _ => {
            println!("\nAvailable input ports:");
            for (i, p) in in_ports.iter().enumerate() {
                println!("{}: {}", i, midi_in.port_name(p).unwrap());
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

    let tx = tx.clone();
    let conn_in = midi_in.connect(
        in_port,
        "midir-read-input",
        move |_, bytes, _| {
            if let Some(message) = handle_midi_message(bytes) {
                tx.send(message).unwrap();
            }
        },
        (),
    )?;

    println!("Connection open, reading input from '{}'", in_port_name);

    Ok(conn_in)
}
