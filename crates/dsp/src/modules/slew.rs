use crate::module::{port, port_mut, BlockContext, Module};

/// Slew limiter / lag: smooths CV (or softens audio edges). Separate
/// rise and fall times (seconds to traverse a unit step of 1.0).
/// Classic use: portamento after a sequencer or MIDI pitch.
pub struct Slew {
    sample_rate: f32,
    rise_s: f32,
    fall_s: f32,
    state: f32,
}

impl Slew {
    pub const IN_SIGNAL: usize = 0;
    pub const IN_RATE_CV: usize = 1;

    pub const OUT_SIGNAL: usize = 0;

    pub const PARAM_RISE: usize = 0;
    pub const PARAM_FALL: usize = 1;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            rise_s: 0.05,
            fall_s: 0.05,
            state: 0.0,
        }
    }

    fn max_delta(&self, time_s: f32, rate_cv: f32) -> f32 {
        // Positive rate_cv shortens time (faster slew): time /= 2^cv.
        let t = (time_s * 2f32.powf(-rate_cv)).max(1e-4);
        1.0 / (t * self.sample_rate)
    }
}

impl Module for Slew {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let signal = port(inputs, Self::IN_SIGNAL, ctx);
        let rate_cv = port(inputs, Self::IN_RATE_CV, ctx);
        let out = port_mut(outputs, Self::OUT_SIGNAL, ctx);

        for i in 0..ctx.frames {
            let target = signal[i];
            let cv = rate_cv[i];
            let delta = target - self.state;
            let max_d = if delta >= 0.0 {
                self.max_delta(self.rise_s, cv)
            } else {
                self.max_delta(self.fall_s, cv)
            };
            self.state += delta.clamp(-max_d, max_d);
            out[i] = self.state;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_RISE => self.rise_s = value.clamp(0.001, 2.0),
            Self::PARAM_FALL => self.fall_s = value.clamp(0.001, 2.0),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_ramps_over_rise_time() {
        let mut s = Slew::new(1000.0);
        s.set_param(Slew::PARAM_RISE, 0.01); // 10 samples for unit step
        s.set_param(Slew::PARAM_FALL, 0.01);
        let frames = 20;
        let mut inputs = vec![0.0; frames * 2];
        for i in 0..frames {
            inputs[i] = 1.0; // signal
        }
        let mut outputs = vec![0.0; frames];
        let ctx = BlockContext {
            sample_rate: 1000.0,
            frames,
        };
        s.process(&inputs, &mut outputs, &ctx);
        assert!(outputs[0] > 0.0 && outputs[0] < 1.0);
        assert!((outputs[15] - 1.0).abs() < 0.05, "got {}", outputs[15]);
    }
}
