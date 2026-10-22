use crate::module::{port, port_mut, BlockContext, Module};

#[derive(Debug, PartialEq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// ADSR envelope generator, driven by a gate input (rather than direct
/// note_on/note_off calls): a rising edge (0 -> >0) starts attack, a
/// falling edge (>0 -> 0) starts release. This lets an `Envelope` be
/// patched to any gate source (MidiToCv, an LFO square out, a manual
/// trigger module, ...), the modular way.
pub struct Envelope {
    sample_rate: f32,
    attack_time: f32,
    decay_time: f32,
    sustain_level: f32,
    release_time: f32,

    stage: Stage,
    current_amplitude: f32,
    release_start_amplitude: f32,
    prev_gate_open: bool,
}

impl Envelope {
    pub const IN_GATE: usize = 0;
    pub const OUT_CV: usize = 0;

    pub const PARAM_ATTACK: usize = 0;
    pub const PARAM_DECAY: usize = 1;
    pub const PARAM_SUSTAIN: usize = 2;
    pub const PARAM_RELEASE: usize = 3;

    pub fn new(sample_rate: f32, attack_time: f32, decay_time: f32, sustain_level: f32, release_time: f32) -> Self {
        Self {
            sample_rate,
            attack_time,
            decay_time,
            sustain_level,
            release_time,
            stage: Stage::Idle,
            current_amplitude: 0.0,
            release_start_amplitude: 0.0,
            prev_gate_open: false,
        }
    }
}

impl Module for Envelope {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let gate = port(inputs, Self::IN_GATE, ctx);
        let out = port_mut(outputs, Self::OUT_CV, ctx);

        for i in 0..ctx.frames {
            let gate_open = gate[i] > 0.0;
            if gate_open && !self.prev_gate_open {
                self.stage = Stage::Attack;
            } else if !gate_open && self.prev_gate_open {
                self.stage = Stage::Release;
                self.release_start_amplitude = self.current_amplitude;
            }
            self.prev_gate_open = gate_open;

            match self.stage {
                Stage::Idle => self.current_amplitude = 0.0,
                Stage::Attack => {
                    self.current_amplitude += 1.0 / (self.attack_time * self.sample_rate);
                    if self.current_amplitude >= 1.0 {
                        self.current_amplitude = 1.0;
                        self.stage = Stage::Decay;
                    }
                }
                Stage::Decay => {
                    self.current_amplitude -=
                        (1.0 - self.sustain_level) / (self.decay_time * self.sample_rate);
                    if self.current_amplitude <= self.sustain_level {
                        self.current_amplitude = self.sustain_level;
                        self.stage = Stage::Sustain;
                    }
                }
                Stage::Sustain => {
                    self.current_amplitude = self.sustain_level;
                }
                Stage::Release => {
                    self.current_amplitude -=
                        self.release_start_amplitude / (self.release_time * self.sample_rate);
                    if self.current_amplitude <= 0.0 {
                        self.current_amplitude = 0.0;
                        self.stage = Stage::Idle;
                    }
                }
            }

            out[i] = self.current_amplitude;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_ATTACK => self.attack_time = value.max(0.001),
            Self::PARAM_DECAY => self.decay_time = value.max(0.001),
            Self::PARAM_SUSTAIN => self.sustain_level = value.clamp(0.0, 1.0),
            Self::PARAM_RELEASE => self.release_time = value.max(0.001),
            _ => {}
        }
    }

    fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> BlockContext {
        BlockContext {
            sample_rate: 100.0,
            frames: 1,
        }
    }

    #[test]
    fn idle_until_gate_opens() {
        let mut env = Envelope::new(100.0, 0.1, 0.1, 0.5, 0.1);
        let mut out = [0.0];
        env.process(&[0.0], &mut out, &ctx());
        assert_eq!(out[0], 0.0);
        assert!(!env.is_active());
    }

    #[test]
    fn attack_ramps_up_then_decays_to_sustain() {
        // attack_time=0.1s at 100Hz -> 10 samples to reach 1.0.
        let mut env = Envelope::new(100.0, 0.1, 0.1, 0.5, 0.1);
        let mut out = [0.0];
        for _ in 0..10 {
            env.process(&[1.0], &mut out, &ctx());
        }
        assert!((out[0] - 1.0).abs() < 1e-4, "expected peak ~1.0, got {}", out[0]);

        // Decay to sustain (0.5) over another 10 samples.
        for _ in 0..10 {
            env.process(&[1.0], &mut out, &ctx());
        }
        assert!((out[0] - 0.5).abs() < 1e-4, "expected sustain ~0.5, got {}", out[0]);
        assert!(env.is_active());
    }

    #[test]
    fn release_ramps_from_current_amplitude_to_zero_and_goes_idle() {
        let mut env = Envelope::new(100.0, 0.1, 0.1, 0.5, 0.1);
        let mut out = [0.0];
        // Hold long enough to reach sustain (0.5).
        for _ in 0..20 {
            env.process(&[1.0], &mut out, &ctx());
        }
        assert!((out[0] - 0.5).abs() < 1e-4);

        // Release: 10 samples should bring it to ~0, not overshoot or
        // undershoot the way a `sustain_level`-scaled step would if
        // release started from something other than the actual amplitude.
        for _ in 0..10 {
            env.process(&[0.0], &mut out, &ctx());
        }
        assert!(out[0].abs() < 1e-4, "expected released to ~0.0, got {}", out[0]);
        assert!(!env.is_active());
    }

    #[test]
    fn re_triggers_on_a_new_rising_edge() {
        let mut env = Envelope::new(100.0, 0.1, 0.1, 0.5, 0.1);
        let mut out = [0.0];
        env.process(&[1.0], &mut out, &ctx());
        env.process(&[0.0], &mut out, &ctx());
        assert!(!env.is_active() || out[0] > 0.0); // mid-release is fine
        env.process(&[1.0], &mut out, &ctx());
        assert!(env.is_active());
    }
}
