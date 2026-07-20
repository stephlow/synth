use crate::module::{port, port_mut, BlockContext, Module};

/// `out = in * scale + offset`. The utility "mod-depth" knob module: put
/// one between any modulation source and its destination to control how
/// much (and in which polarity) it affects the target, or to re-center a
/// bipolar signal into a unipolar range.
pub struct Attenuverter {
    scale: f32,
    offset: f32,
}

impl Attenuverter {
    pub const IN_SIGNAL: usize = 0;

    pub const OUT_SIGNAL: usize = 0;

    pub const PARAM_SCALE: usize = 0;
    pub const PARAM_OFFSET: usize = 1;

    pub fn new() -> Self {
        Self {
            scale: 1.0,
            offset: 0.0,
        }
    }
}

impl Default for Attenuverter {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Attenuverter {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let input = port(inputs, Self::IN_SIGNAL, ctx);
        let out = port_mut(outputs, Self::OUT_SIGNAL, ctx);
        let (scale, offset) = (self.scale, self.offset);
        for i in 0..ctx.frames {
            out[i] = input[i] * scale + offset;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_SCALE => self.scale = value,
            Self::PARAM_OFFSET => self.offset = value,
            _ => {}
        }
    }
}
