use crate::module::{port, port_mut, BlockContext, Module};

/// Soft saturator: tanh drive with dry/wet mix.
pub struct Saturator {
    drive: f32,
    mix: f32,
}

impl Saturator {
    pub const IN: usize = 0;
    pub const IN_DRIVE_CV: usize = 1;

    pub const OUT: usize = 0;

    pub const PARAM_DRIVE: usize = 0;
    pub const PARAM_MIX: usize = 1;

    pub fn new() -> Self {
        Self {
            drive: 2.0,
            mix: 1.0,
        }
    }
}

impl Default for Saturator {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Saturator {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let input = port(inputs, Self::IN, ctx);
        let drive_cv = port(inputs, Self::IN_DRIVE_CV, ctx);
        let out = port_mut(outputs, Self::OUT, ctx);
        let mix = self.mix.clamp(0.0, 1.0);

        for i in 0..ctx.frames {
            let d = (self.drive + drive_cv[i] * 4.0).max(0.1);
            let wet = (input[i] * d).tanh();
            out[i] = input[i] * (1.0 - mix) + wet * mix;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_DRIVE => self.drive = value.clamp(0.1, 20.0),
            Self::PARAM_MIX => self.mix = value.clamp(0.0, 1.0),
            _ => {}
        }
    }
}
