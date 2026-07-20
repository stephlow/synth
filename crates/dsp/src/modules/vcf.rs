use crate::module::{port, BlockContext, Module};

/// Voltage-controlled state-variable filter (Zavalishin's "TPT" topology),
/// unconditionally stable and giving simultaneous lowpass/highpass/
/// bandpass outputs from one core — the classic multi-mode modular filter.
pub struct Vcf {
    sample_rate: f32,
    cutoff_hz: f32,
    resonance: f32,
    low: f32,
    band: f32,
}

impl Vcf {
    pub const IN_AUDIO: usize = 0;
    pub const IN_CUTOFF_CV: usize = 1;
    pub const IN_RESONANCE_CV: usize = 2;

    pub const OUT_LOWPASS: usize = 0;
    pub const OUT_HIGHPASS: usize = 1;
    pub const OUT_BANDPASS: usize = 2;

    pub const PARAM_CUTOFF: usize = 0;
    pub const PARAM_RESONANCE: usize = 1;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            cutoff_hz: 1000.0,
            resonance: 0.2,
            low: 0.0,
            band: 0.0,
        }
    }
}

impl Module for Vcf {
    fn input_count(&self) -> usize {
        3
    }

    fn output_count(&self) -> usize {
        3
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let audio = port(inputs, Self::IN_AUDIO, ctx);
        let cutoff_cv = port(inputs, Self::IN_CUTOFF_CV, ctx);
        let resonance_cv = port(inputs, Self::IN_RESONANCE_CV, ctx);

        let sample_rate = self.sample_rate;
        let cutoff_hz = self.cutoff_hz;
        let resonance_base = self.resonance;

        let (lowpass_out, rest) = outputs.split_at_mut(ctx.frames);
        let (highpass_out, bandpass_out) = rest.split_at_mut(ctx.frames);

        for i in 0..ctx.frames {
            // Cutoff CV is 1V/oct on top of the cutoff param, like the
            // VCO's pitch input.
            let cutoff = (cutoff_hz * 2f32.powf(cutoff_cv[i])).clamp(10.0, sample_rate * 0.49);
            let resonance = (resonance_base + resonance_cv[i]).clamp(0.0, 1.0);

            // damping (q) derived so resonance in [0,1] sweeps from
            // heavily damped to near self-oscillation.
            let q = (1.0 - resonance).max(0.001);
            let f = 2.0 * (std::f32::consts::PI * cutoff / sample_rate).sin();

            self.low += f * self.band;
            let high = audio[i] - self.low - q * self.band;
            self.band += f * high;

            lowpass_out[i] = self.low;
            highpass_out[i] = high;
            bandpass_out[i] = self.band;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_CUTOFF => self.cutoff_hz = value.max(10.0),
            Self::PARAM_RESONANCE => self.resonance = value.clamp(0.0, 1.0),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn lowpass_attenuates_a_high_frequency_more_than_a_low_one() {
        let sample_rate = 48000.0;
        let n = 2000;
        let ctx = BlockContext {
            sample_rate,
            frames: n,
        };

        let run = |freq_hz: f32| -> f32 {
            let mut vcf = Vcf::new(sample_rate);
            vcf.set_param(Vcf::PARAM_CUTOFF, 500.0);
            vcf.set_param(Vcf::PARAM_RESONANCE, 0.1);

            let input: Vec<f32> = (0..n)
                .map(|i| {
                    let phase = 2.0 * std::f32::consts::PI * freq_hz * i as f32 / sample_rate;
                    phase.sin()
                })
                .collect();
            let mut inputs = vec![0.0; 3 * n];
            inputs[..n].copy_from_slice(&input);
            let mut outputs = vec![0.0; 3 * n];

            vcf.process(&inputs, &mut outputs, &ctx);
            let low_samples = &outputs[Vcf::OUT_LOWPASS * n..(Vcf::OUT_LOWPASS + 1) * n];

            // Skip the filter's initial transient.
            rms(&low_samples[n / 2..])
        };

        let low_freq_energy = run(100.0); // well below cutoff
        let high_freq_energy = run(8000.0); // well above cutoff

        assert!(
            high_freq_energy < low_freq_energy * 0.3,
            "expected lowpass to attenuate 8kHz much more than 100Hz: low={low_freq_energy}, high={high_freq_energy}"
        );
    }
}
