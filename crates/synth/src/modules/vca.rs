use crate::module::{port, port_mut, BlockContext, Module};

/// Voltage-controlled amplifier: `out = audio * level * cv`. The `level`
/// param is the static knob; `cv` is typically patched from an envelope
/// or LFO for dynamic amplitude shaping.
pub struct Vca {
    level: f32,
}

impl Vca {
    pub const IN_AUDIO: usize = 0;
    pub const IN_CV: usize = 1;

    pub const OUT_AUDIO: usize = 0;

    pub const PARAM_LEVEL: usize = 0;

    pub fn new() -> Self {
        Self { level: 1.0 }
    }
}

impl Default for Vca {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for Vca {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let audio = port(inputs, Self::IN_AUDIO, ctx);
        let cv = port(inputs, Self::IN_CV, ctx);
        let out = port_mut(outputs, Self::OUT_AUDIO, ctx);
        let level = self.level;
        for i in 0..ctx.frames {
            out[i] = audio[i] * level * cv[i];
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_LEVEL {
            self.level = value;
        }
    }
}
