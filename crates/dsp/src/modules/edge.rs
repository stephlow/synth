use crate::module::{port, port_mut, BlockContext, Module};

/// Rising-edge detector: emits a short trigger pulse on each 0→high transition.
pub struct Edge {
    sample_rate: f32,
    width_ms: f32,
    prev: bool,
    pulse_left: u32,
}

impl Edge {
    pub const IN: usize = 0;

    pub const OUT: usize = 0;

    pub const PARAM_WIDTH: usize = 0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            width_ms: 5.0,
            prev: false,
            pulse_left: 0,
        }
    }

    fn pulse_samples(&self) -> u32 {
        ((self.sample_rate * self.width_ms / 1000.0).round() as u32).max(1)
    }
}

impl Module for Edge {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let input = port(inputs, Self::IN, ctx);
        let out = port_mut(outputs, Self::OUT, ctx);
        let width = self.pulse_samples();

        for i in 0..ctx.frames {
            let high = input[i] > 0.0;
            if high && !self.prev {
                self.pulse_left = width;
            }
            self.prev = high;
            out[i] = if self.pulse_left > 0 {
                self.pulse_left -= 1;
                1.0
            } else {
                0.0
            };
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_WIDTH {
            self.width_ms = value.clamp(0.5, 50.0);
        }
    }
}
