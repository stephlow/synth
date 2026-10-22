use crate::module::{port, port_mut, BlockContext, Module};

/// Hard clip to ±level (level from knob + CV).
pub struct Clip {
    level: f32,
}

impl Clip {
    pub const IN: usize = 0;
    pub const IN_LEVEL_CV: usize = 1;

    pub const OUT: usize = 0;

    pub const PARAM_LEVEL: usize = 0;

    pub fn new() -> Self {
        Self { level: 1.0 }
    }
}

impl Default for Clip {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Clip {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let input = port(inputs, Self::IN, ctx);
        let level_cv = port(inputs, Self::IN_LEVEL_CV, ctx);
        let out = port_mut(outputs, Self::OUT, ctx);
        for i in 0..ctx.frames {
            let lim = (self.level + level_cv[i]).abs().max(1e-4);
            out[i] = input[i].clamp(-lim, lim);
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_LEVEL {
            self.level = value.clamp(0.01, 2.0);
        }
    }
}
