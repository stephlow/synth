use crate::{node::Node, oscillator::Waveform, voice::Voice};
use wmidi::{Note, Velocity};

pub struct Synth {
    pub voices: [Voice; 4],
    voices_map: [Option<(Note, Velocity)>; 4],
}

impl Synth {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            voices: [
                Voice::new(sample_rate, Waveform::Saw),
                Voice::new(sample_rate, Waveform::Square),
                Voice::new(sample_rate, Waveform::Triangle),
                Voice::new(sample_rate, Waveform::Sine),
            ],
            voices_map: [None, None, None, None],
        }
    }

    fn get_first_available_voice_map_index(&self) -> Option<usize> {
        if let Some((index, _)) = self
            .voices_map
            .iter()
            .enumerate()
            .find(|(_, voice)| voice.is_none())
        {
            return Some(index);
        }

        None
    }

    fn get_active_voice_map_index(&self, note: Note) -> Option<usize> {
        if let Some((index, _)) = self.voices_map.iter().enumerate().find(|(_, voice)| {
            if let Some((n, _)) = voice {
                return n.to_freq_f32() == note.to_freq_f32();
            }

            false
        }) {
            return Some(index);
        }

        None
    }

    pub fn note_on(&mut self, note: Note, velocity: Velocity) {
        if let Some(index) = self.get_first_available_voice_map_index() {
            if let Some(osc) = self.voices.get_mut(index) {
                let gain = u8::from(velocity) as f32 / 127.0;
                osc.note_on(note.to_freq_f32(), gain);
                self.voices_map[index] = Some((note, velocity));
            }
        }
    }

    pub fn note_off(&mut self, note: Note, _velocity: Velocity) {
        if let Some(index) = self.get_active_voice_map_index(note) {
            if let Some(osc) = self.voices.get_mut(index) {
                osc.note_off();
                self.voices_map[index] = None;
            }
        }
    }

    pub fn tick(&mut self) -> f32 {
        let mut output = 0.0;
        for osc in &mut self.voices {
            output += osc.tick();
        }

        output / self.voices.len() as f32
    }
}
