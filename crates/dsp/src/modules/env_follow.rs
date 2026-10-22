use crate::module::{port, port_mut, BlockContext, Module};

/// Envelope follower: rectified audio with rise/fall smoothing → unipolar CV.
pub struct EnvFollow {
    sample_rate: f32,
    rise_s: f32,
    fall_s: f32,
    env: f32,
}

impl EnvFollow {
    pub const IN: usize = 0;

    pub const OUT: usize = 0;

    pub const PARAM_RISE: usize = 0;
    pub const PARAM_FALL: usize = 1;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            rise_s: 0.01,
            fall_s: 0.1,
            env: 0.0,
        }
    }
}

impl Module for EnvFollow {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let input = port(inputs, Self::IN, ctx);
        let out = port_mut(outputs, Self::OUT, ctx);
        let rise_c = 1.0 - (-1.0 / (self.rise_s.max(1e-4) * self.sample_rate)).exp();
        let fall_c = 1.0 - (-1.0 / (self.fall_s.max(1e-4) * self.sample_rate)).exp();

        for i in 0..ctx.frames {
            let target = input[i].abs();
            let c = if target > self.env { rise_c } else { fall_c };
            self.env += (target - self.env) * c;
            out[i] = self.env;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_RISE => self.rise_s = value.clamp(0.001, 2.0),
            Self::PARAM_FALL => self.fall_s = value.clamp(0.001, 5.0),
            _ => {}
        }
    }
}
