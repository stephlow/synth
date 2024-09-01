#[derive(Debug)]
pub struct ADSR {
    attack_time: f32,
    decay_time: f32,
    sustain_level: f32,
    release_time: f32,
    sample_rate: f32,
    state: ADSRState,
    current_amplitude: f32,
    current_time: f32,
}

#[derive(Debug, PartialEq)]
enum ADSRState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

impl ADSR {
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
            state: ADSRState::Idle,
            current_amplitude: 0.0,
            current_time: 0.0,
        }
    }

    pub fn note_on(&mut self) {
        self.state = ADSRState::Attack;
        self.current_time = 0.0;
    }

    pub fn note_off(&mut self) {
        self.state = ADSRState::Release;
        self.current_time = 0.0;
    }

    pub fn tick(&mut self) -> f32 {
        match self.state {
            ADSRState::Idle => self.current_amplitude = 0.0,
            ADSRState::Attack => {
                self.current_amplitude += 1.0 / (self.attack_time * self.sample_rate);
                if self.current_amplitude >= 1.0 {
                    self.current_amplitude = 1.0;
                    self.state = ADSRState::Decay;
                }
            }
            ADSRState::Decay => {
                self.current_amplitude -=
                    (1.0 - self.sustain_level) / (self.decay_time * self.sample_rate);
                if self.current_amplitude <= self.sustain_level {
                    self.current_amplitude = self.sustain_level;
                    self.state = ADSRState::Sustain;
                }
            }
            ADSRState::Sustain => {
                // Maintain sustain level
                self.current_amplitude = self.sustain_level;
            }
            ADSRState::Release => {
                self.current_amplitude -=
                    self.sustain_level / (self.release_time * self.sample_rate);
                if self.current_amplitude <= 0.0 {
                    self.current_amplitude = 0.0;
                    self.state = ADSRState::Idle;
                }
            }
        }

        self.current_time += 1.0;
        self.current_amplitude
    }
}
