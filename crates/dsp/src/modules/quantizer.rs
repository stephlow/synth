use crate::module::{port, port_mut, BlockContext, Module};

use super::scale::{clamp_scale_index, quantize_to_scale, SCALES, SCALE_MAJOR};

/// Pitch quantizer: snaps continuous 1V/oct CV to a scale degree.
///
/// Patch any pitch-like signal (sequencer, LFO, S&H, noise) into `cv` and
/// take the quantized output into a VCO pitch input. Scale and root are
/// knobs; scale index selects a preset from [`SCALES`].
pub struct Quantizer {
    scale_idx: usize,
    /// Root offset in semitones (0 = unshifted 1V/oct frame).
    root_semitones: f32,
}

impl Quantizer {
    pub const IN_CV: usize = 0;
    pub const OUT_CV: usize = 0;

    pub const PARAM_SCALE: usize = 0;
    pub const PARAM_ROOT: usize = 1;

    pub fn new() -> Self {
        Self {
            scale_idx: SCALE_MAJOR,
            root_semitones: 0.0,
        }
    }

    pub fn with_params(scale: f32, root: f32) -> Self {
        let mut q = Self::new();
        q.set_param(Self::PARAM_SCALE, scale);
        q.set_param(Self::PARAM_ROOT, root);
        q
    }
}

impl Default for Quantizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Quantizer {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let input = port(inputs, Self::IN_CV, ctx);
        let out = port_mut(outputs, Self::OUT_CV, ctx);
        let mask = SCALES
            .get(self.scale_idx)
            .map(|s| s.mask)
            .unwrap_or(0x0FFF);
        let root = self.root_semitones;

        for i in 0..ctx.frames {
            out[i] = quantize_to_scale(input[i], root, mask);
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_SCALE => self.scale_idx = clamp_scale_index(value),
            Self::PARAM_ROOT => self.root_semitones = value.clamp(-12.0, 12.0),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::Module;

    #[test]
    fn quantizes_block_to_major() {
        let mut q = Quantizer::new();
        // Off-scale input ~1.4 semis → should snap to 2 semis in major.
        let frames = 4;
        let inputs = vec![1.4 / 12.0; frames];
        let mut outputs = vec![0.0; frames];
        let ctx = BlockContext {
            sample_rate: 48_000.0,
            frames,
        };
        q.process(&inputs, &mut outputs, &ctx);
        for &o in &outputs {
            assert!((o - 2.0 / 12.0).abs() < 1e-5, "got {o}");
        }
    }
}
