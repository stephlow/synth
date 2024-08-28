use wmidi::{Note, Velocity};

use crate::oscillator::Oscillator;

#[derive(Debug)]
pub struct Synth {
    pub oscillators: [Oscillator; 4],
    voices: [Option<Note>; 4],
}

impl Synth {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            oscillators: [
                Oscillator::new(sample_rate),
                Oscillator::new(sample_rate),
                Oscillator::new(sample_rate),
                Oscillator::new(sample_rate),
            ],
            voices: [None, None, None, None],
        }
    }

    fn get_first_available_voice_index(&self) -> Option<usize> {
        if let Some((index, _)) = self
            .voices
            .iter()
            .enumerate()
            .find(|(_, voice)| voice.is_none())
        {
            return Some(index);
        }

        None
    }

    fn get_active_voice_index(&self, note: Note) -> Option<usize> {
        if let Some((index, _)) = self.voices.iter().enumerate().find(|(_, voice)| {
            if let Some(n) = voice {
                return n.to_freq_f32() == note.to_freq_f32();
            }

            return false;
        }) {
            return Some(index);
        }

        None
    }

    pub fn note_on(&mut self, note: Note) {
        if let Some(index) = self.get_first_available_voice_index() {
            if let Some(osc) = self.oscillators.get_mut(index) {
                osc.note_on(note.to_freq_f32());
                self.voices[index] = Some(note);
            }
        }
    }

    pub fn note_off(&mut self, note: Note) {
        if let Some(index) = self.get_active_voice_index(note) {
            if let Some(osc) = self.oscillators.get_mut(index) {
                osc.note_off();
                self.voices[index] = None;
            }
        }
    }

    pub fn tick(&mut self) -> f32 {
        // println!("{:?}", self.voices);

        let mut output = 0.0;
        for osc in &mut self.oscillators {
            output += osc.tick();
        }

        output / self.oscillators.len() as f32
    }
}
