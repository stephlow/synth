use crate::module::{port, port_mut, BlockContext, Module};

/// Simple 4-input mixer with a per-channel level (attenuator), the
/// utility module for combining several signals before further patching.
pub struct Mixer {
    levels: [f32; 4],
}

impl Mixer {
    pub const IN_1: usize = 0;
    pub const IN_2: usize = 1;
    pub const IN_3: usize = 2;
    pub const IN_4: usize = 3;

    pub const OUT_MIX: usize = 0;

    pub const PARAM_LEVEL_1: usize = 0;
    pub const PARAM_LEVEL_2: usize = 1;
    pub const PARAM_LEVEL_3: usize = 2;
    pub const PARAM_LEVEL_4: usize = 3;

    pub fn new() -> Self {
        Self {
            levels: [1.0; 4],
        }
    }
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Mixer {
    fn input_count(&self) -> usize {
        4
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let out = port_mut(outputs, Self::OUT_MIX, ctx);
        out.fill(0.0);
        for (channel, &level) in self.levels.iter().enumerate() {
            let input = port(inputs, channel, ctx);
            for i in 0..ctx.frames {
                out[i] += input[i] * level;
            }
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param < self.levels.len() {
            self.levels[param] = value;
        }
    }
}
