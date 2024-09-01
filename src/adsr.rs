use crate::node::Node;

#[derive(Debug)]
pub struct Adsr {
    attack_time: f32,
    decay_time: f32,
    sustain_level: f32,
    release_time: f32,
    sample_rate: f32,
    state: AdsrState,
    current_amplitude: f32,
    current_time: f32,
}

#[derive(Debug, PartialEq)]
enum AdsrState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

impl Adsr {
    pub fn new(
        sample_rate: f32,
        attack_time: f32,
        decay_time: f32,
        sustain_level: f32,
        release_time: f32,
    ) -> Self {
        Self {
            attack_time,
            decay_time,
            sustain_level,
            release_time,
            sample_rate,
            state: AdsrState::Idle,
            current_amplitude: 0.0,
            current_time: 0.0,
        }
    }

    pub fn note_on(&mut self) {
        self.state = AdsrState::Attack;
        self.current_time = 0.0;
    }

    pub fn note_off(&mut self) {
        self.state = AdsrState::Release;
        self.current_time = 0.0;
    }
}

impl Node for Adsr {
    fn tick(&mut self) -> f32 {
        match self.state {
            AdsrState::Idle => self.current_amplitude = 0.0,
            AdsrState::Attack => {
                self.current_amplitude += 1.0 / (self.attack_time * self.sample_rate);
                if self.current_amplitude >= 1.0 {
                    self.current_amplitude = 1.0;
                    self.state = AdsrState::Decay;
                }
            }
            AdsrState::Decay => {
                self.current_amplitude -=
                    (1.0 - self.sustain_level) / (self.decay_time * self.sample_rate);
                if self.current_amplitude <= self.sustain_level {
                    self.current_amplitude = self.sustain_level;
                    self.state = AdsrState::Sustain;
                }
            }
            AdsrState::Sustain => {
                // Maintain sustain level
                self.current_amplitude = self.sustain_level;
            }
            AdsrState::Release => {
                self.current_amplitude -=
                    self.sustain_level / (self.release_time * self.sample_rate);
                if self.current_amplitude <= 0.0 {
                    self.current_amplitude = 0.0;
                    self.state = AdsrState::Idle;
                }
            }
        }

        self.current_time += 1.0;
        self.current_amplitude
    }
}
