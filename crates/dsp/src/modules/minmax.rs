use crate::module::{port, BlockContext, Module};

/// Simultaneous min and max of two signals.
pub struct MinMax;

impl MinMax {
    pub const IN_A: usize = 0;
    pub const IN_B: usize = 1;

    pub const OUT_MIN: usize = 0;
    pub const OUT_MAX: usize = 1;

    pub fn new() -> Self {
        Self
    }
}

impl Default for MinMax {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for MinMax {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        2
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let a = port(inputs, Self::IN_A, ctx);
        let b = port(inputs, Self::IN_B, ctx);
        let (min_out, max_out) = outputs.split_at_mut(ctx.frames);
        for i in 0..ctx.frames {
            min_out[i] = a[i].min(b[i]);
            max_out[i] = a[i].max(b[i]);
        }
    }
}
