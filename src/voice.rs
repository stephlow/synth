use crate::{
    adsr::Adsr,
    node::Node,
    oscillator::{Oscillator, Waveform},
};

pub struct Voice {
    oscillator: Oscillator,
    adsr: Adsr,
}

impl Voice {
    pub fn new(sample_rate: f32, waveform: Waveform) -> Self {
        Self {
            oscillator: Oscillator::new(sample_rate, waveform),
            adsr: Adsr::new(sample_rate, 1.1, 0.1, 0.7, 2.1),
        }
    }

    pub fn note_on(&mut self, freq: f32, gain: f32) {
        self.oscillator.note_on(freq, gain);
        self.adsr.note_on();
    }

    pub fn note_off(&mut self) {
        self.oscillator.note_off();
        self.adsr.note_off();
    }
}

impl Node for Voice {
    fn tick(&mut self) -> f32 {
        self.oscillator.tick() * self.adsr.tick()
    }
}
