use crate::module::{port, BlockContext, Module};

/// Voltage-controlled oscillator. Pitch is 1V/oct relative to 440 Hz;
/// `fm` is summed in as additional octaves (for through-zero-style FM
/// patches) and `tune` is a static octave offset knob. All four classic
/// waveforms are computed every sample and exposed on separate outputs,
/// like a real VCO panel — patch whichever shape(s) you want downstream.
///
/// Band-limited via PolyBLEP (a small correction added around each
/// discontinuity) rather than additive synthesis. This bounds the
/// per-sample cost to a small constant, independent of frequency — the
/// additive approach this replaced re-summed a harmonic series up to
/// Nyquist on *every sample* (hundreds of `sin()` calls for a low bass
/// note), which is exactly the kind of input-dependent worst case a
/// real-time audio callback can't afford: it must always finish well
/// inside the deadline, for every note, not just on average.
pub struct Vco {
    sample_rate: f32,
    phase: f32,
    tune: f32,
    /// Leaky integrator state for deriving the triangle from the square
    /// (integrating a band-limited square gives a band-limited triangle).
    triangle_integrator: f32,
}

impl Vco {
    pub const IN_PITCH: usize = 0;
    pub const IN_FM: usize = 1;

    pub const OUT_SINE: usize = 0;
    pub const OUT_SAW: usize = 1;
    pub const OUT_SQUARE: usize = 2;
    pub const OUT_TRIANGLE: usize = 3;

    pub const PARAM_TUNE: usize = 0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            phase: 0.0,
            tune: 0.0,
            triangle_integrator: 0.0,
        }
    }

    /// PolyBLEP correction for a phase discontinuity at `phase`, `dt` =
    /// phase increment per sample. Added at a rising edge (saw/square) to
    /// smooth what would otherwise be a hard, aliasing step.
    fn poly_blep(phase: f32, dt: f32) -> f32 {
        if phase < dt {
            let t = phase / dt;
            t + t - t * t - 1.0
        } else if phase > 1.0 - dt {
            let t = (phase - 1.0) / dt;
            t * t + t + t + 1.0
        } else {
            0.0
        }
    }
}

impl Module for Vco {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        4
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let pitch_in = port(inputs, Self::IN_PITCH, ctx);
        let fm_in = port(inputs, Self::IN_FM, ctx);

        let (sine_out, rest) = outputs.split_at_mut(ctx.frames);
        let (saw_out, rest) = rest.split_at_mut(ctx.frames);
        let (square_out, triangle_out) = rest.split_at_mut(ctx.frames);

        let tune = self.tune;
        let two_pi = 2.0 * std::f32::consts::PI;

        for i in 0..ctx.frames {
            let pitch = pitch_in[i] + fm_in[i] + tune;
            let freq = 440.0 * 2f32.powf(pitch);
            let dt = (freq / self.sample_rate).clamp(0.0, 0.5);

            sine_out[i] = (self.phase * two_pi).sin();

            // Naive saw (2*phase-1) plus a BLEP correction at its one
            // discontinuity (phase wrap) removes the aliasing a raw
            // sawtooth would otherwise produce above a few hundred Hz.
            let saw = 2.0 * self.phase - 1.0 - Self::poly_blep(self.phase, dt);
            saw_out[i] = saw;

            // Naive square plus BLEP corrections at both edges (phase 0
            // and phase 0.5, where it flips from +1 to -1 or back).
            let mut square = if self.phase < 0.5 { 1.0 } else { -1.0 };
            square += Self::poly_blep(self.phase, dt);
            let half_phase = (self.phase + 0.5).fract();
            square -= Self::poly_blep(half_phase, dt);
            square_out[i] = square;

            // Leaky-integrate the band-limited square into a triangle;
            // the leak (0.999) keeps DC/very-low-frequency drift bounded.
            self.triangle_integrator = 0.999 * self.triangle_integrator + dt * square * 4.0;
            triangle_out[i] = self.triangle_integrator;

            self.phase = (self.phase + dt).fract();
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_TUNE {
            self.tune = value;
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
    fn output_stays_bounded_and_finite_at_a_low_frequency() {
        // A low note is exactly the case that made the old additive
        // implementation's per-sample cost balloon; here it should just
        // produce a normal, bounded, finite signal like any other note.
        let sample_rate = 48000.0;
        let frames = 4096;
        let mut vco = Vco::new(sample_rate);
        // 20 Hz -> 1V/oct pitch relative to 440Hz.
        let pitch = (20.0f32 / 440.0).log2();

        let inputs = vec![pitch; frames]; // port 0 (pitch), port 1 (fm) omitted below
        let mut full_inputs = vec![0.0; 2 * frames];
        full_inputs[..frames].copy_from_slice(&inputs);
        let mut outputs = vec![0.0; 4 * frames];

        vco.process(&full_inputs, &mut outputs, &ctx(sample_rate, frames));

        for &s in &outputs {
            assert!(s.is_finite(), "VCO produced a non-finite sample: {s}");
            assert!(s.abs() <= 1.5, "VCO sample out of expected range: {s}");
        }
    }

    #[test]
    fn saw_output_is_periodic_at_the_requested_frequency() {
        let sample_rate = 48000.0f32;
        let freq = 100.0f32;
        let period_samples = (sample_rate / freq).round() as usize;
        let frames = period_samples * 4;

        let mut vco = Vco::new(sample_rate);
        let pitch = (freq / 440.0).log2();
        let mut full_inputs = vec![0.0; 2 * frames];
        full_inputs[..frames].fill(pitch);
        let mut outputs = vec![0.0; 4 * frames];

        vco.process(&full_inputs, &mut outputs, &ctx(sample_rate, frames));
        let saw = &outputs[frames..2 * frames];

        // Same phase one period apart should give (almost) the same
        // sample, modulo the small BLEP smoothing near the discontinuity.
        let a = saw[period_samples];
        let b = saw[2 * period_samples];
        assert!((a - b).abs() < 0.05, "saw not periodic: {a} vs {b}");
    }
}
