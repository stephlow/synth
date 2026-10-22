use crate::module::{port, port_mut, BlockContext, Module};

/// Mono digital delay with fixed max buffer (allocated at create time).
///
/// `time_cv` scales delay time (2^cv octaves toward longer/shorter).
/// `fb_cv` adds to feedback. Mix is dry/wet on the single output.
pub struct Delay {
    sample_rate: f32,
    buf: Vec<f32>,
    write: usize,
    time_ms: f32,
    feedback: f32,
    mix: f32,
}

impl Delay {
    pub const MAX_TIME_MS: f32 = 2000.0;

    pub const IN_AUDIO: usize = 0;
    pub const IN_TIME_CV: usize = 1;
    pub const IN_FB_CV: usize = 2;

    pub const OUT_MIX: usize = 0;

    pub const PARAM_TIME: usize = 0;
    pub const PARAM_FEEDBACK: usize = 1;
    pub const PARAM_MIX: usize = 2;

    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let max_samples = ((Self::MAX_TIME_MS / 1000.0) * sr).ceil() as usize + 4;
        Self {
            sample_rate: sr,
            buf: vec![0.0; max_samples.max(8)],
            write: 0,
            time_ms: 250.0,
            feedback: 0.35,
            mix: 0.3,
        }
    }

    fn read_frac(&self, delay_samples: f32) -> f32 {
        let n = self.buf.len() as f32;
        let d = delay_samples.clamp(1.0, n - 2.0);
        let pos = self.write as f32 - d;
        // Wrap into [0, n)
        let pos = pos.rem_euclid(n);
        let i0 = pos.floor() as usize % self.buf.len();
        let i1 = (i0 + 1) % self.buf.len();
        let frac = pos - pos.floor();
        self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac
    }
}

impl Module for Delay {
    fn input_count(&self) -> usize {
        3
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let audio = port(inputs, Self::IN_AUDIO, ctx);
        let time_cv = port(inputs, Self::IN_TIME_CV, ctx);
        let fb_cv = port(inputs, Self::IN_FB_CV, ctx);
        let out = port_mut(outputs, Self::OUT_MIX, ctx);

        let max_delay = (self.buf.len() - 2) as f32;

        for i in 0..ctx.frames {
            let time_ms = (self.time_ms * 2f32.powf(time_cv[i])).clamp(1.0, Self::MAX_TIME_MS);
            let delay_samples =
                (time_ms * 0.001 * self.sample_rate).clamp(1.0, max_delay);
            let fb = (self.feedback + fb_cv[i] * 0.5).clamp(0.0, 0.95);
            let wet = self.read_frac(delay_samples);
            let dry = audio[i];
            // Write input + feedback.
            let w = dry + wet * fb;
            self.buf[self.write] = w;
            self.write += 1;
            if self.write >= self.buf.len() {
                self.write = 0;
            }
            let mix = self.mix.clamp(0.0, 1.0);
            out[i] = dry * (1.0 - mix) + wet * mix;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_TIME => self.time_ms = value.clamp(1.0, Self::MAX_TIME_MS),
            Self::PARAM_FEEDBACK => self.feedback = value.clamp(0.0, 0.95),
            Self::PARAM_MIX => self.mix = value.clamp(0.0, 1.0),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_reappears_after_delay() {
        let mut d = Delay::new(1000.0);
        d.set_param(Delay::PARAM_TIME, 10.0); // 10 samples
        d.set_param(Delay::PARAM_FEEDBACK, 0.0);
        d.set_param(Delay::PARAM_MIX, 1.0); // fully wet
        let frames = 32;
        let mut inputs = vec![0.0; frames * 3];
        inputs[0] = 1.0; // impulse
        let mut outputs = vec![0.0; frames];
        let ctx = BlockContext {
            sample_rate: 1000.0,
            frames,
        };
        d.process(&inputs, &mut outputs, &ctx);
        // Wet path should peak near sample 10.
        let peak_i = outputs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            (peak_i as i32 - 10).abs() <= 2,
            "peak at {peak_i}, expected ~10, out={outputs:?}"
        );
    }
}
