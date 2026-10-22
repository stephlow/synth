use crate::module::{port, port_mut, BlockContext, Module};

/// Sequential switch: clock advances among 4 inputs; selected signal on out.
pub struct Switch {
    steps: u8,
    pos: u8,
    prev_clock: bool,
    prev_reset: bool,
}

impl Switch {
    pub const IN_1: usize = 0;
    pub const IN_2: usize = 1;
    pub const IN_3: usize = 2;
    pub const IN_4: usize = 3;
    pub const IN_CLOCK: usize = 4;
    pub const IN_RESET: usize = 5;

    pub const OUT: usize = 0;

    pub const PARAM_STEPS: usize = 0;

    pub fn new() -> Self {
        Self {
            steps: 4,
            pos: 0,
            prev_clock: false,
            prev_reset: false,
        }
    }
}

impl Default for Switch {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Switch {
    fn input_count(&self) -> usize {
        6
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let in1 = port(inputs, Self::IN_1, ctx);
        let in2 = port(inputs, Self::IN_2, ctx);
        let in3 = port(inputs, Self::IN_3, ctx);
        let in4 = port(inputs, Self::IN_4, ctx);
        let clock = port(inputs, Self::IN_CLOCK, ctx);
        let reset = port(inputs, Self::IN_RESET, ctx);
        let out = port_mut(outputs, Self::OUT, ctx);
        let n = self.steps.max(1) as u8;

        for i in 0..ctx.frames {
            let r = reset[i] > 0.0;
            if r && !self.prev_reset {
                self.pos = 0;
            }
            self.prev_reset = r;

            let c = clock[i] > 0.0;
            if c && !self.prev_clock {
                self.pos = (self.pos + 1) % n;
            }
            self.prev_clock = c;

            out[i] = match self.pos {
                0 => in1[i],
                1 => in2[i],
                2 => in3[i],
                _ => in4[i],
            };
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_STEPS {
            self.steps = value.round().clamp(2.0, 4.0) as u8;
        }
    }
}
