use crate::module::{port, port_mut, BlockContext, Module};

/// Full-wave rectifier: `out = |in|`.
pub struct Abs;

impl Abs {
    pub const IN: usize = 0;
    pub const OUT: usize = 0;

    pub fn new() -> Self {
        Self
    }
}

impl Default for Abs {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Abs {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let input = port(inputs, Self::IN, ctx);
        let out = port_mut(outputs, Self::OUT, ctx);
        for i in 0..ctx.frames {
            out[i] = input[i].abs();
        }
    }
}
