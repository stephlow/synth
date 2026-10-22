use crate::module::{port, port_mut, BlockContext, Module};

/// Euclidean rhythm: on each clock, advance step; short gate when step is a hit.
pub struct Euclidean {
    sample_rate: f32,
    steps: u32,
    fill: u32,
    rotate: u32,
    step: u32,
    prev_clock: bool,
    prev_reset: bool,
    pulse_left: u32,
}

impl Euclidean {
    pub const IN_CLOCK: usize = 0;
    pub const IN_RESET: usize = 1;

    pub const OUT_GATE: usize = 0;

    pub const PARAM_STEPS: usize = 0;
    pub const PARAM_FILL: usize = 1;
    pub const PARAM_ROTATE: usize = 2;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            steps: 16,
            fill: 4,
            rotate: 0,
            step: 0,
            prev_clock: false,
            prev_reset: false,
            pulse_left: 0,
        }
    }

    fn pulse_width(&self) -> u32 {
        ((self.sample_rate * 0.005).round() as u32).max(1)
    }
}

fn euclid_hit(step: u32, steps: u32, fill: u32, rotate: u32) -> bool {
    if steps == 0 || fill == 0 {
        return false;
    }
    let fill = fill.min(steps);
    let s = (step + rotate) % steps;
    (s * fill) % steps < fill
}

impl Module for Euclidean {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let clock = port(inputs, Self::IN_CLOCK, ctx);
        let reset = port(inputs, Self::IN_RESET, ctx);
        let out = port_mut(outputs, Self::OUT_GATE, ctx);
        let width = self.pulse_width();
        let steps = self.steps.max(1);
        let fill = self.fill;
        let rotate = self.rotate;

        for i in 0..ctx.frames {
            let r = reset[i] > 0.0;
            if r && !self.prev_reset {
                self.step = 0;
            }
            self.prev_reset = r;

            let c = clock[i] > 0.0;
            if c && !self.prev_clock {
                if euclid_hit(self.step, steps, fill, rotate) {
                    self.pulse_left = width;
                }
                self.step = (self.step + 1) % steps;
            }
            self.prev_clock = c;

            out[i] = if self.pulse_left > 0 {
                self.pulse_left -= 1;
                1.0
            } else {
                0.0
            };
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_STEPS => self.steps = value.round().clamp(1.0, 32.0) as u32,
            Self::PARAM_FILL => self.fill = value.round().clamp(0.0, 32.0) as u32,
            Self::PARAM_ROTATE => self.rotate = value.round().clamp(0.0, 31.0) as u32,
            _ => {}
        }
    }
}
