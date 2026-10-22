use crate::module::{port, port_mut, BlockContext, Module};

/// An oscilloscope: a pass-through module that taps the signal at its
/// input and keeps a ring buffer for a live display (TUI scope tile, gRPC
/// module waveforms, ...). It copies its input straight to its output so
/// dropping it into a patch changes nothing about what flows downstream —
/// it's a probe, not a processor.
///
/// Display knobs (not in the audio path):
/// - **Time** — horizontal window length in ms
/// - **Y Scale** — vertical gain (`1` = ±1 fills full height at UI peak floor 1)
pub struct Oscilloscope {
    sample_rate: f32,
    /// Ring of captured input samples (fixed at create for max time).
    ring: Vec<f32>,
    write: usize,
    /// Total samples ever written (saturates at `ring.len()` for full flag).
    filled: usize,
    time_ms: f32,
    y_scale: f32,
}

impl Oscilloscope {
    /// One sample at 48 kHz (~20.8 µs). High notes still resolve to a cycle.
    pub const MIN_TIME_MS: f32 = 0.02;
    /// LFO / envelope window (unchanged — range is extended downward only).
    pub const MAX_TIME_MS: f32 = 10_000.0;
    /// ~1 cycle of ~400 Hz (standard mid VCO pitch) so a sine is readable
    /// out of the box without zooming.
    pub const DEFAULT_TIME_MS: f32 = 2.5;
    pub const DEFAULT_Y_SCALE: f32 = 1.0;

    pub const IN_AUDIO: usize = 0;
    pub const OUT_AUDIO: usize = 0;

    pub const PARAM_TIME: usize = 0;
    pub const PARAM_Y_SCALE: usize = 1;

    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let max_samples = ((Self::MAX_TIME_MS / 1000.0) * sr).ceil() as usize + 4;
        Self {
            sample_rate: sr,
            ring: vec![0.0; max_samples.max(8)],
            write: 0,
            filled: 0,
            time_ms: Self::DEFAULT_TIME_MS,
            y_scale: Self::DEFAULT_Y_SCALE,
        }
    }

    fn window_samples(&self) -> usize {
        let ms = self.time_ms.clamp(Self::MIN_TIME_MS, Self::MAX_TIME_MS);
        let n = ((ms / 1000.0) * self.sample_rate).ceil() as usize;
        n.clamp(1, self.ring.len())
    }

    /// Samples available in the ring (up to capacity).
    fn available(&self) -> usize {
        self.filled.min(self.ring.len())
    }

    /// Read sample at `age` samples before the write head (0 = most recent).
    fn read_age(&self, age: f32) -> f32 {
        let n = self.ring.len();
        let avail = self.available();
        if n == 0 || avail == 0 {
            return 0.0;
        }
        let max_age = (avail - 1) as f32;
        let age = age.clamp(0.0, max_age);
        let age0 = age.floor() as usize;
        let age1 = (age0 + 1).min(avail - 1);
        let frac = age - age0 as f32;
        // write points at next slot; most recent is write-1.
        let i0 = (self.write + n - 1 - age0) % n;
        let i1 = (self.write + n - 1 - age1) % n;
        self.ring[i0] * (1.0 - frac) + self.ring[i1] * frac
    }
}

impl Default for Oscilloscope {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl Module for Oscilloscope {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let audio = port(inputs, Self::IN_AUDIO, ctx);
        let out = port_mut(outputs, Self::OUT_AUDIO, ctx);
        out.copy_from_slice(audio);

        let n = self.ring.len();
        if n == 0 {
            return;
        }
        for &s in audio {
            self.ring[self.write] = s;
            self.write = (self.write + 1) % n;
            if self.filled < n {
                self.filled += 1;
            }
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_TIME => {
                self.time_ms = value.clamp(Self::MIN_TIME_MS, Self::MAX_TIME_MS);
            }
            Self::PARAM_Y_SCALE => {
                self.y_scale = value.clamp(0.1, 20.0);
            }
            _ => {}
        }
    }

    fn fill_display_wave(&self, out: &mut [f32]) -> usize {
        if out.is_empty() || self.available() == 0 {
            return 0;
        }
        let win = self.window_samples().min(self.available()).max(1);
        let scale = self.y_scale;
        let n_out = out.len();
        let max_age = (win - 1) as f32;
        for k in 0..n_out {
            // Map left→right as oldest→newest within the window (fractional
            // age + lerp so audio cycles stay smooth at high display density).
            let age = if n_out <= 1 {
                0.0
            } else {
                let t = k as f32 / (n_out - 1) as f32;
                (1.0 - t) * max_age
            };
            out[k] = self.read_age(age) * scale;
        }
        n_out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(frames: usize) -> BlockContext {
        BlockContext {
            sample_rate: 48_000.0,
            frames,
        }
    }

    fn process_block(scope: &mut Oscilloscope, samples: &[f32]) -> Vec<f32> {
        let mut output = vec![0.0; samples.len()];
        scope.process(samples, &mut output, &ctx(samples.len()));
        output
    }

    #[test]
    fn passes_input_through_unchanged() {
        let mut scope = Oscilloscope::new(48_000.0);
        let input = [0.1, -0.2, 0.3, -0.4];
        let output = process_block(&mut scope, &input);
        assert_eq!(output.as_slice(), &input);
    }

    #[test]
    fn fill_display_wave_scales_constant() {
        let mut scope = Oscilloscope::new(48_000.0);
        scope.set_param(Oscilloscope::PARAM_TIME, 10.0);
        scope.set_param(Oscilloscope::PARAM_Y_SCALE, 2.0);
        // 480 samples at 48k = 10ms
        let block = vec![0.25f32; 480];
        let _ = process_block(&mut scope, &block);
        let mut wave = [0.0f32; 8];
        let n = scope.fill_display_wave(&mut wave);
        assert_eq!(n, 8);
        for s in &wave {
            assert!((s - 0.5).abs() < 1e-4, "got {s}");
        }
    }

    #[test]
    fn sub_ms_window_is_accepted() {
        let mut scope = Oscilloscope::new(48_000.0);
        scope.set_param(Oscilloscope::PARAM_TIME, 0.1); // 100 µs ≈ 5 samples
        let block = vec![0.0f32; 64];
        let _ = process_block(&mut scope, &block);
        let mut wave = [0.0f32; 8];
        assert_eq!(scope.fill_display_wave(&mut wave), 8);
    }

    #[test]
    fn default_window_resolves_audio_sine_shape() {
        // ~440 Hz sine, default 2.5 ms ≈ 1 cycle — display should swing both ways.
        let mut scope = Oscilloscope::new(48_000.0);
        let n = 512;
        let mut block = vec![0.0f32; n];
        for (i, s) in block.iter_mut().enumerate() {
            let t = i as f32 / 48_000.0;
            *s = (t * 440.0 * std::f32::consts::TAU).sin();
        }
        let _ = process_block(&mut scope, &block);
        let mut wave = [0.0f32; 128];
        assert_eq!(scope.fill_display_wave(&mut wave), 128);
        let min = wave.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = wave.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!(min < -0.5 && max > 0.5, "expected bipolar sine, got [{min}, {max}]");
        // Not a flat line or single spike: several zero crossings.
        let mut crossings = 0;
        for w in wave.windows(2) {
            if w[0] == 0.0 || (w[0] < 0.0) != (w[1] < 0.0) {
                crossings += 1;
            }
        }
        assert!(crossings >= 1, "expected zero crossings, got {crossings}");
    }

    #[test]
    fn y_scale_does_not_change_passthrough() {
        let mut scope = Oscilloscope::new(48_000.0);
        scope.set_param(Oscilloscope::PARAM_Y_SCALE, 10.0);
        let input = [0.3f32; 16];
        let output = process_block(&mut scope, &input);
        assert_eq!(output.as_slice(), &input);
    }

    #[test]
    fn longer_time_window_includes_older_history() {
        let mut scope = Oscilloscope::new(1_000.0); // 1 kHz for easy math
        // 1 sample = 1 ms
        scope.set_param(Oscilloscope::PARAM_Y_SCALE, 1.0);

        // Write 20 samples: first half 0.0, second half 1.0
        let mut block = vec![0.0f32; 10];
        block.extend(std::iter::repeat(1.0).take(10));
        let _ = process_block(&mut scope, &block);

        // Short window (5 ms = 5 newest samples) → all ~1.0
        scope.set_param(Oscilloscope::PARAM_TIME, 5.0);
        let mut short = [0.0f32; 4];
        assert_eq!(scope.fill_display_wave(&mut short), 4);
        assert!(
            short.iter().all(|&s| s > 0.9),
            "short window should be recent 1s: {short:?}"
        );

        // Long window (20 ms) → starts near 0, ends near 1
        scope.set_param(Oscilloscope::PARAM_TIME, 20.0);
        let mut long = [0.0f32; 8];
        assert_eq!(scope.fill_display_wave(&mut long), 8);
        assert!(
            long[0] < 0.25 && long[long.len() - 1] > 0.75,
            "long window should span history: {long:?}"
        );
    }

    #[test]
    fn empty_ring_returns_zero_len() {
        let scope = Oscilloscope::new(48_000.0);
        let mut wave = [0.0f32; 8];
        assert_eq!(scope.fill_display_wave(&mut wave), 0);
    }
}
