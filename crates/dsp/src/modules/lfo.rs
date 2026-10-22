use crate::module::{port, BlockContext, Module};

/// Low-frequency oscillator: a modulation-source counterpart to [`crate::modules::Vco`],
/// running at sub-audio rates.
///
/// Frequency is absolute Hz (`rate_hz * 2^rate_cv`), not 1V/oct around 440.
/// All four classic waveforms are computed every sample and exposed on
/// separate bipolar (−1..1) outputs — same panel layout as the VCO, without
/// PolyBLEP (unnecessary below audio rates).
pub struct Lfo {
    sample_rate: f32,
    phase: f32,
    rate_hz: f32,
}

impl Lfo {
    pub const IN_RATE_CV: usize = 0;

    pub const OUT_SINE: usize = 0;
    pub const OUT_SAW: usize = 1;
    pub const OUT_SQUARE: usize = 2;
    pub const OUT_TRIANGLE: usize = 3;

    pub const PARAM_RATE: usize = 0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            phase: 0.0,
            rate_hz: 2.0,
        }
    }
}

impl Module for Lfo {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        4
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let rate_cv = port(inputs, Self::IN_RATE_CV, ctx);

        let (sine_out, rest) = outputs.split_at_mut(ctx.frames);
        let (saw_out, rest) = rest.split_at_mut(ctx.frames);
        let (square_out, triangle_out) = rest.split_at_mut(ctx.frames);

        let two_pi = 2.0 * std::f32::consts::PI;

        for i in 0..ctx.frames {
            let rate = (self.rate_hz * 2f32.powf(rate_cv[i])).max(0.0);
            self.phase = (self.phase + rate / self.sample_rate).fract();
            let phase = self.phase;

            sine_out[i] = (phase * two_pi).sin();
            saw_out[i] = 2.0 * phase - 1.0;
            square_out[i] = if phase < 0.5 { 1.0 } else { -1.0 };
            triangle_out[i] = if phase < 0.5 {
                4.0 * phase - 1.0
            } else {
                3.0 - 4.0 * phase
            };
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_RATE {
            self.rate_hz = value.max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(sample_rate: f32, frames: usize) -> BlockContext {
        BlockContext { sample_rate, frames }
    }

    #[test]
    fn all_outputs_bounded_and_finite() {
        let sample_rate = 48000.0;
        let frames = 2048;
        let mut lfo = Lfo::new(sample_rate);
        lfo.set_param(Lfo::PARAM_RATE, 4.0);

        let inputs = vec![0.0; frames];
        let mut outputs = vec![0.0; 4 * frames];
        lfo.process(&inputs, &mut outputs, &ctx(sample_rate, frames));

        for &s in &outputs {
            assert!(s.is_finite(), "LFO produced non-finite sample: {s}");
            assert!(s.abs() <= 1.0 + 1e-5, "LFO sample out of ±1: {s}");
        }
    }

    #[test]
    fn square_is_bipolar_half_cycle() {
        let sample_rate = 1000.0;
        let frames = 1000; // exactly one cycle at 1 Hz
        let mut lfo = Lfo::new(sample_rate);
        lfo.set_param(Lfo::PARAM_RATE, 1.0);

        let inputs = vec![0.0; frames];
        let mut outputs = vec![0.0; 4 * frames];
        lfo.process(&inputs, &mut outputs, &ctx(sample_rate, frames));

        let square = &outputs[2 * frames..3 * frames];
        let pos = square.iter().filter(|&&s| s > 0.0).count();
        let neg = square.iter().filter(|&&s| s < 0.0).count();
        // One full cycle: roughly half high, half low (± a sample at edges).
        assert!((pos as i32 - neg as i32).abs() <= 2, "pos={pos} neg={neg}");
    }
}
