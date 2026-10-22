use crate::module::{port, port_mut, BlockContext, Module};

/// Ring modulator / four-quadrant multiplier: `out = a * b * level`.
/// Patch two audio signals for metallic tones, or audio × CV for AM.
pub struct RingMod {
    level: f32,
}

impl RingMod {
    pub const IN_A: usize = 0;
    pub const IN_B: usize = 1;

    pub const OUT: usize = 0;

    pub const PARAM_LEVEL: usize = 0;

    pub fn new() -> Self {
        Self { level: 1.0 }
    }
}

impl Default for RingMod {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for RingMod {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let a = port(inputs, Self::IN_A, ctx);
        let b = port(inputs, Self::IN_B, ctx);
        let out = port_mut(outputs, Self::OUT, ctx);
        let level = self.level;
        for i in 0..ctx.frames {
            out[i] = a[i] * b[i] * level;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_LEVEL {
            self.level = value.clamp(0.0, 2.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplies_inputs() {
        let mut r = RingMod::new();
        let frames = 4;
        let mut inputs = vec![0.0; frames * 2];
        for i in 0..frames {
            inputs[i] = 0.5;
            inputs[frames + i] = 0.5;
        }
        let mut outputs = vec![0.0; frames];
        let ctx = BlockContext {
            sample_rate: 48_000.0,
            frames,
        };
        r.process(&inputs, &mut outputs, &ctx);
        for &o in &outputs {
            assert!((o - 0.25).abs() < 1e-5);
        }
    }
}
