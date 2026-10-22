use crate::module::{port, port_mut, BlockContext, Module};

/// Digital logic on gate-like signals (>0 = high). Mode: AND OR XOR NOT.
pub struct Logic {
    mode: u8,
}

impl Logic {
    pub const IN_A: usize = 0;
    pub const IN_B: usize = 1;

    pub const OUT: usize = 0;

    pub const PARAM_MODE: usize = 0;

    pub const MODE_AND: u8 = 0;
    pub const MODE_OR: u8 = 1;
    pub const MODE_XOR: u8 = 2;
    pub const MODE_NOT: u8 = 3;

    pub fn new() -> Self {
        Self { mode: Self::MODE_AND }
    }
}

impl Default for Logic {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Logic {
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
        let mode = self.mode;

        for i in 0..ctx.frames {
            let ha = a[i] > 0.0;
            let hb = b[i] > 0.0;
            let y = match mode {
                Self::MODE_OR => ha || hb,
                Self::MODE_XOR => ha ^ hb,
                Self::MODE_NOT => !ha,
                _ => ha && hb,
            };
            out[i] = if y { 1.0 } else { 0.0 };
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_MODE {
            self.mode = value.round().clamp(0.0, 3.0) as u8;
        }
    }
}
