use crate::module::{port, port_mut, BlockContext, Module};

/// West-coast wavefolder: folds a signal through a saturating transfer
/// as drive increases, generating bright odd harmonics. Patch a sine or
/// triangle from a VCO for classic Buchla-style timbres.
pub struct Wavefolder {
    drive: f32,
    symmetry: f32,
    mix: f32,
}

impl Wavefolder {
    pub const IN_AUDIO: usize = 0;
    pub const IN_DRIVE_CV: usize = 1;

    pub const OUT_AUDIO: usize = 0;

    pub const PARAM_DRIVE: usize = 0;
    pub const PARAM_SYMMETRY: usize = 1;
    pub const PARAM_MIX: usize = 2;

    pub fn new() -> Self {
        Self {
            drive: 1.5,
            symmetry: 0.0,
            mix: 1.0,
        }
    }

    /// Soft multi-fold: scale, offset by symmetry, fold into [-1,1] a few times.
    fn fold(mut x: f32) -> f32 {
        // A few absolute folds then soft clip.
        for _ in 0..4 {
            if x > 1.0 || x < -1.0 {
                x = x.signum() * 2.0 - x;
            } else {
                break;
            }
            // Reflect again if still outside after fold.
            if x > 1.0 {
                x = 2.0 - x;
            } else if x < -1.0 {
                x = -2.0 - x;
            }
        }
        x.clamp(-1.0, 1.0)
    }
}

impl Default for Wavefolder {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Wavefolder {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let audio = port(inputs, Self::IN_AUDIO, ctx);
        let drive_cv = port(inputs, Self::IN_DRIVE_CV, ctx);
        let out = port_mut(outputs, Self::OUT_AUDIO, ctx);
        let mix = self.mix.clamp(0.0, 1.0);

        for i in 0..ctx.frames {
            let drive = (self.drive * 2f32.powf(drive_cv[i])).clamp(0.1, 20.0);
            let x = audio[i] * drive + self.symmetry;
            let wet = Self::fold(x);
            out[i] = audio[i] * (1.0 - mix) + wet * mix;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_DRIVE => self.drive = value.clamp(0.1, 20.0),
            Self::PARAM_SYMMETRY => self.symmetry = value.clamp(-1.0, 1.0),
            Self::PARAM_MIX => self.mix = value.clamp(0.0, 1.0),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_hot_sine_peaks() {
        let mut w = Wavefolder::new();
        w.set_param(Wavefolder::PARAM_DRIVE, 4.0);
        w.set_param(Wavefolder::PARAM_MIX, 1.0);
        let frames = 8;
        let mut inputs = vec![0.0; frames * 2];
        inputs[0] = 0.8; // will exceed fold threshold when driven
        let mut outputs = vec![0.0; frames];
        let ctx = BlockContext {
            sample_rate: 48_000.0,
            frames,
        };
        w.process(&inputs, &mut outputs, &ctx);
        assert!(outputs[0].is_finite());
        assert!(outputs[0].abs() <= 1.0);
        // Folded result should differ from simple scaled input.
        assert!((outputs[0] - 0.8 * 4.0).abs() > 0.1);
    }
}
