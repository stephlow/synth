use crate::module::{port, BlockContext, Module};

/// Gate when `signal` is above threshold (+ CV). `not` is inverted gate.
pub struct Comparator {
    thresh: f32,
}

impl Comparator {
    pub const IN_SIGNAL: usize = 0;
    pub const IN_THRESH_CV: usize = 1;

    pub const OUT_GATE: usize = 0;
    pub const OUT_NOT: usize = 1;

    pub const PARAM_THRESH: usize = 0;

    pub fn new() -> Self {
        Self { thresh: 0.0 }
    }
}

impl Default for Comparator {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Comparator {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        2
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let signal = port(inputs, Self::IN_SIGNAL, ctx);
        let thresh_cv = port(inputs, Self::IN_THRESH_CV, ctx);
        let (gate_out, not_out) = outputs.split_at_mut(ctx.frames);

        for i in 0..ctx.frames {
            let t = self.thresh + thresh_cv[i];
            let high = signal[i] > t;
            gate_out[i] = if high { 1.0 } else { 0.0 };
            not_out[i] = if high { 0.0 } else { 1.0 };
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_THRESH {
            self.thresh = value.clamp(-1.0, 1.0);
        }
    }
}
