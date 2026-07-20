use crate::module::{port, BlockContext, Module};

/// Clocked step sequencer: advances on rising edges of `clock`, holds a
/// pitch CV per step, and emits gate + trigger for envelope drivers.
///
/// No internal BPM clock — patch an LFO, square wave, or external pulse
/// into `clock`. Pitch steps are 1V/oct (same frame as VCO / MidiToCv).
pub struct Sequencer {
    sample_rate: f32,
    /// Active pattern length (1..=MAX_STEPS).
    steps_len: usize,
    /// Fraction of the previous clock period the gate stays high (0..1).
    gate_len: f32,
    /// Per-step pitch CV (volts / octaves).
    steps: [f32; Self::MAX_STEPS],

    step: usize,
    prev_clock_open: bool,
    prev_reset_open: bool,

    /// Samples elapsed since last clock (or reset that restarts timing).
    samples_since_clock: u32,
    /// Previous inter-clock period in samples (for gate length).
    last_period_samples: u32,
    /// Samples remaining in the trigger pulse (0 = idle).
    trigger_samples_left: u32,
}

impl Sequencer {
    pub const MAX_STEPS: usize = 16;

    pub const IN_CLOCK: usize = 0;
    pub const IN_RESET: usize = 1;

    pub const OUT_CV: usize = 0;
    pub const OUT_GATE: usize = 1;
    pub const OUT_TRIGGER: usize = 2;

    pub const PARAM_STEPS: usize = 0;
    pub const PARAM_GATE_LEN: usize = 1;
    /// First step-value param index; step `n` (0-based) is `PARAM_STEP0 + n`.
    pub const PARAM_STEP0: usize = 2;

    /// ~5 ms trigger pulse.
    const TRIGGER_MS: f32 = 5.0;
    /// Default period when no clock interval has been observed yet (~0.1 s).
    const DEFAULT_PERIOD_S: f32 = 0.1;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            steps_len: 8,
            gate_len: 0.5,
            // Simple ascending major-ish motif in 1V/oct (semitones / 12).
            steps: [
                0.0 / 12.0,
                2.0 / 12.0,
                4.0 / 12.0,
                5.0 / 12.0,
                7.0 / 12.0,
                9.0 / 12.0,
                11.0 / 12.0,
                12.0 / 12.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
            ],
            step: 0,
            prev_clock_open: false,
            prev_reset_open: false,
            samples_since_clock: 0,
            last_period_samples: (sample_rate.max(1.0) * Self::DEFAULT_PERIOD_S) as u32,
            trigger_samples_left: 0,
        }
    }

    pub fn param_step(n: usize) -> usize {
        Self::PARAM_STEP0 + n
    }

    fn trigger_width_samples(&self) -> u32 {
        ((self.sample_rate * Self::TRIGGER_MS / 1000.0).round() as u32).max(1)
    }

    fn fire_trigger(&mut self) {
        self.trigger_samples_left = self.trigger_width_samples();
    }

    fn on_reset(&mut self) {
        self.step = 0;
        self.samples_since_clock = 0;
        self.fire_trigger();
    }

    fn on_clock(&mut self) {
        // Period for the step we are leaving / gate we just finished.
        if self.samples_since_clock > 0 {
            self.last_period_samples = self.samples_since_clock;
        }
        self.steps_len = self.steps_len.clamp(1, Self::MAX_STEPS);
        self.step = (self.step + 1) % self.steps_len;
        self.samples_since_clock = 0;
        self.fire_trigger();
    }
}

impl Module for Sequencer {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        3
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let clock = port(inputs, Self::IN_CLOCK, ctx);
        let reset = port(inputs, Self::IN_RESET, ctx);

        let (cv_out, rest) = outputs.split_at_mut(ctx.frames);
        let (gate_out, trig_out) = rest.split_at_mut(ctx.frames);

        let steps_len = self.steps_len.clamp(1, Self::MAX_STEPS);
        // Keep step in range if Steps param shrank.
        if self.step >= steps_len {
            self.step = 0;
        }

        let gate_len = self.gate_len.clamp(0.0, 1.0);

        for i in 0..ctx.frames {
            let clock_open = clock[i] > 0.0;
            let reset_open = reset[i] > 0.0;

            // Reset takes priority over clock on the same sample.
            if reset_open && !self.prev_reset_open {
                self.on_reset();
            } else if clock_open && !self.prev_clock_open {
                self.on_clock();
            }

            self.prev_clock_open = clock_open;
            self.prev_reset_open = reset_open;

            let step = self.step.min(steps_len - 1);
            cv_out[i] = self.steps[step];

            // Gate length is a fraction of the *previous* clock period
            // (updated inside on_clock), so recompute after edge handling.
            let gate_high = if gate_len >= 1.0 {
                true // legato: high until next clock
            } else {
                let period = self.last_period_samples.max(1) as f32;
                let gate_samples = (period * gate_len).round() as u32;
                self.samples_since_clock < gate_samples
            };
            gate_out[i] = if gate_high { 1.0 } else { 0.0 };

            let trig_high = self.trigger_samples_left > 0;
            trig_out[i] = if trig_high { 1.0 } else { 0.0 };
            if self.trigger_samples_left > 0 {
                self.trigger_samples_left -= 1;
            }

            // Count after emitting this sample so gate_len=0 still allows
            // a zero-length gate, and period tracks full intervals.
            self.samples_since_clock = self.samples_since_clock.saturating_add(1);
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_STEPS => {
                let n = value.round() as i32;
                self.steps_len = n.clamp(1, Self::MAX_STEPS as i32) as usize;
                if self.step >= self.steps_len {
                    self.step = 0;
                }
            }
            Self::PARAM_GATE_LEN => {
                self.gate_len = value.clamp(0.0, 1.0);
            }
            p if p >= Self::PARAM_STEP0 && p < Self::PARAM_STEP0 + Self::MAX_STEPS => {
                let idx = p - Self::PARAM_STEP0;
                self.steps[idx] = value.clamp(-2.0, 2.0);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::Module;

    fn process_block(seq: &mut Sequencer, clock: &[f32], reset: &[f32]) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        assert_eq!(clock.len(), reset.len());
        let frames = clock.len();
        let mut inputs = vec![0.0; frames * 2];
        inputs[..frames].copy_from_slice(clock);
        inputs[frames..].copy_from_slice(reset);
        let mut outputs = vec![0.0; frames * 3];
        let ctx = BlockContext {
            sample_rate: seq.sample_rate,
            frames,
        };
        seq.process(&inputs, &mut outputs, &ctx);
        let cv = outputs[..frames].to_vec();
        let gate = outputs[frames..frames * 2].to_vec();
        let trig = outputs[frames * 2..].to_vec();
        (cv, gate, trig)
    }

    #[test]
    fn clock_advances_step_and_cv() {
        let mut seq = Sequencer::new(48_000.0);
        seq.set_param(Sequencer::PARAM_STEPS, 4.0);
        seq.set_param(Sequencer::param_step(0), 0.0);
        seq.set_param(Sequencer::param_step(1), 0.25);
        seq.set_param(Sequencer::param_step(2), 0.5);
        seq.set_param(Sequencer::param_step(3), 0.75);

        // Start at step 0.
        let n = 8;
        let mut clock = vec![0.0; n];
        let reset = vec![0.0; n];
        let (cv, _, _) = process_block(&mut seq, &clock, &reset);
        assert!((cv[0] - 0.0).abs() < 1e-5);

        // Rising edge mid-block → advance to step 1.
        clock = vec![0.0; n];
        clock[4] = 1.0;
        clock[5] = 1.0;
        let (cv, _, trig) = process_block(&mut seq, &clock, &reset);
        assert!((cv[3] - 0.0).abs() < 1e-5);
        assert!((cv[4] - 0.25).abs() < 1e-5);
        assert!(trig[4] > 0.5);

        // Another edge → step 2.
        clock = vec![0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let (cv, _, _) = process_block(&mut seq, &clock, &reset);
        assert!((cv[1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn reset_returns_to_step_zero() {
        let mut seq = Sequencer::new(48_000.0);
        seq.set_param(Sequencer::PARAM_STEPS, 4.0);
        seq.set_param(Sequencer::param_step(0), 0.0);
        seq.set_param(Sequencer::param_step(1), 0.25);
        seq.set_param(Sequencer::param_step(2), 0.5);

        let reset = vec![0.0; 4];
        // Advance twice.
        for _ in 0..2 {
            let clock = vec![0.0, 1.0, 1.0, 0.0];
            process_block(&mut seq, &clock, &reset);
        }
        // Reset edge.
        let clock = vec![0.0; 4];
        let reset = vec![0.0, 1.0, 1.0, 0.0];
        let (cv, _, _) = process_block(&mut seq, &clock, &reset);
        assert!((cv[1] - 0.0).abs() < 1e-5);
    }

    #[test]
    fn gate_fraction_of_period() {
        let mut seq = Sequencer::new(1_000.0); // easy sample math
        seq.set_param(Sequencer::PARAM_GATE_LEN, 0.5);
        seq.set_param(Sequencer::PARAM_STEPS, 2.0);

        // Establish a 10-sample period with two clocks 10 samples apart.
        let mut clock = vec![0.0; 10];
        clock[0] = 1.0;
        let reset = vec![0.0; 10];
        process_block(&mut seq, &clock, &reset);

        clock = vec![0.0; 10];
        clock[0] = 1.0; // period observed = 10
        let (_, gate, _) = process_block(&mut seq, &clock, &reset);
        // gate_len 0.5 → high for 5 samples from clock.
        let high: usize = gate.iter().filter(|&&g| g > 0.5).count();
        assert!(high >= 4 && high <= 6, "high count {high}, gate={gate:?}");
    }
}
