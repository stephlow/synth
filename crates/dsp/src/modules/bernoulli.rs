use crate::module::{port, BlockContext, Module};

/// On each rising trigger, route to A or B with probability `chance` (+ CV).
pub struct Bernoulli {
    sample_rate: f32,
    chance: f32,
    state: u32,
    prev_trig: bool,
    pulse_left_a: u32,
    pulse_left_b: u32,
}

impl Bernoulli {
    pub const IN_TRIG: usize = 0;
    pub const IN_CHANCE_CV: usize = 1;

    pub const OUT_A: usize = 0;
    pub const OUT_B: usize = 1;

    pub const PARAM_CHANCE: usize = 0;

    pub fn new(sample_rate: f32, seed: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            chance: 0.5,
            state: if seed == 0 { 0xA5A5_1234 } else { seed },
            prev_trig: false,
            pulse_left_a: 0,
            pulse_left_b: 0,
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    fn pulse_width(&self) -> u32 {
        ((self.sample_rate * 0.005).round() as u32).max(1)
    }
}

impl Module for Bernoulli {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        2
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let trig = port(inputs, Self::IN_TRIG, ctx);
        let chance_cv = port(inputs, Self::IN_CHANCE_CV, ctx);
        let (out_a, out_b) = outputs.split_at_mut(ctx.frames);
        let width = self.pulse_width();

        for i in 0..ctx.frames {
            let high = trig[i] > 0.0;
            if high && !self.prev_trig {
                let p = (self.chance + chance_cv[i]).clamp(0.0, 1.0);
                let roll = (self.next_u32() as f32) / (u32::MAX as f32);
                if roll < p {
                    self.pulse_left_a = width;
                } else {
                    self.pulse_left_b = width;
                }
            }
            self.prev_trig = high;

            out_a[i] = if self.pulse_left_a > 0 {
                self.pulse_left_a -= 1;
                1.0
            } else {
                0.0
            };
            out_b[i] = if self.pulse_left_b > 0 {
                self.pulse_left_b -= 1;
                1.0
            } else {
                0.0
            };
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_CHANCE {
            self.chance = value.clamp(0.0, 1.0);
        }
    }
}
