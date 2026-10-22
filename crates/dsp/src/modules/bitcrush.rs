use crate::module::{port, port_mut, BlockContext, Module};

/// Bit depth + sample-rate reduction.
pub struct Bitcrush {
    bits: f32,
    rate: f32, // 0..1 fraction of sample rate held
    held: f32,
    phase: f32,
}

impl Bitcrush {
    pub const IN: usize = 0;

    pub const OUT: usize = 0;

    pub const PARAM_BITS: usize = 0;
    pub const PARAM_RATE: usize = 1;

    pub fn new() -> Self {
        Self {
            bits: 8.0,
            rate: 0.25,
            held: 0.0,
            phase: 0.0,
        }
    }
}

impl Default for Bitcrush {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Bitcrush {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let input = port(inputs, Self::IN, ctx);
        let out = port_mut(outputs, Self::OUT, ctx);
        let bits = self.bits.clamp(1.0, 16.0);
        let levels = (2f32.powf(bits) - 1.0).max(1.0);
        // rate param 0.01..1.0: fraction of full SR; phase step
        let step = self.rate.clamp(0.01, 1.0);

        for i in 0..ctx.frames {
            self.phase += step;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                // quantize to bit depth
                let x = input[i].clamp(-1.0, 1.0);
                self.held = (x * levels).round() / levels;
            }
            out[i] = self.held;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_BITS => self.bits = value.clamp(1.0, 16.0),
            Self::PARAM_RATE => self.rate = value.clamp(0.01, 1.0),
            _ => {}
        }
    }
}
