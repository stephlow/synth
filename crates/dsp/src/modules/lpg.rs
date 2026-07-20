use crate::module::{port, port_mut, BlockContext, Module};

/// Low-pass gate (West-coast classic): a combined VCA + low-pass filter
/// controlled by one CV. High CV → open (bright + loud); low CV → dark
/// and quiet. Strike with a short envelope/pluck for Buchla-style hits.
pub struct Lpg {
    sample_rate: f32,
    /// Residual level when CV is 0 (vactrol-ish bleed / offset).
    offset: f32,
    /// LP state (one-pole).
    lp: f32,
}

impl Lpg {
    pub const IN_AUDIO: usize = 0;
    pub const IN_CV: usize = 1;

    pub const OUT_AUDIO: usize = 0;

    pub const PARAM_OFFSET: usize = 0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            offset: 0.0,
            lp: 0.0,
        }
    }
}

impl Module for Lpg {
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
        let offset = self.offset.clamp(0.0, 1.0);

        for i in 0..ctx.frames {
            let c = (cv[i] + offset).clamp(0.0, 1.5);
            // Cutoff from ~80 Hz (closed) to ~12 kHz (open).
            let cutoff = 80.0 * 150f32.powf(c.min(1.0));
            let cutoff = cutoff.min(self.sample_rate * 0.45);
            let coeff = 1.0 - (-2.0 * std::f32::consts::PI * cutoff / self.sample_rate).exp();
            self.lp += coeff * (audio[i] - self.lp);
            // Amplitude tracks CV (with slight curve).
            let amp = c.min(1.0).powf(0.7);
            out[i] = self.lp * amp;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_OFFSET {
            self.offset = value.clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_gate_is_quiet() {
        let mut g = Lpg::new(48_000.0);
        let frames = 64;
        let mut inputs = vec![0.0; frames * 2];
        for i in 0..frames {
            inputs[i] = 0.5; // audio
            // cv left 0
        }
        let mut outputs = vec![0.0; frames];
        let ctx = BlockContext {
            sample_rate: 48_000.0,
            frames,
        };
        g.process(&inputs, &mut outputs, &ctx);
        let peak = outputs.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        assert!(peak < 0.05, "closed LPG peak {peak}");
    }

    #[test]
    fn open_gate_passes_signal() {
        let mut g = Lpg::new(48_000.0);
        let frames = 256;
        let mut inputs = vec![0.0; frames * 2];
        for i in 0..frames {
            inputs[i] = if i % 2 == 0 { 0.5 } else { -0.5 };
            inputs[frames + i] = 1.0; // CV open
        }
        let mut outputs = vec![0.0; frames];
        let ctx = BlockContext {
            sample_rate: 48_000.0,
            frames,
        };
        g.process(&inputs, &mut outputs, &ctx);
        let peak = outputs.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        assert!(peak > 0.1, "open LPG peak {peak}");
    }
}
