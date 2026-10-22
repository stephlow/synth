use crate::module::{port, port_mut, BlockContext, Module};

/// Linear crossfade: `out = a*(1-x) + b*x`, x from knob + CV (0..1).
pub struct Crossfade {
    fade: f32,
}

impl Crossfade {
    pub const IN_A: usize = 0;
    pub const IN_B: usize = 1;
    pub const IN_FADE_CV: usize = 2;

    pub const OUT: usize = 0;

    pub const PARAM_FADE: usize = 0;

    pub fn new() -> Self {
        Self { fade: 0.5 }
    }
}

impl Default for Crossfade {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Crossfade {
    fn input_count(&self) -> usize {
        3
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let a = port(inputs, Self::IN_A, ctx);
        let b = port(inputs, Self::IN_B, ctx);
        let fade_cv = port(inputs, Self::IN_FADE_CV, ctx);
        let out = port_mut(outputs, Self::OUT, ctx);

        for i in 0..ctx.frames {
            let x = (self.fade + fade_cv[i]).clamp(0.0, 1.0);
            out[i] = a[i] * (1.0 - x) + b[i] * x;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_FADE {
            self.fade = value.clamp(0.0, 1.0);
        }
    }
}
