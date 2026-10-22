use crate::module::{port, BlockContext, Module};

/// Clock multiplier: measures period between rising edges and emits ×2/×3/×4 pulses.
pub struct ClockMult {
    sample_rate: f32,
    samples_since: u32,
    period: u32,
    prev_clock: bool,
    // subdivision phase counters in samples within current period
    phase2: u32,
    phase3: u32,
    phase4: u32,
    pulse_left: [u32; 3],
    armed: bool,
}

impl ClockMult {
    pub const IN_CLOCK: usize = 0;

    pub const OUT_X2: usize = 0;
    pub const OUT_X3: usize = 1;
    pub const OUT_X4: usize = 2;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            samples_since: 0,
            period: 0,
            prev_clock: false,
            phase2: 0,
            phase3: 0,
            phase4: 0,
            pulse_left: [0; 3],
            armed: false,
        }
    }

    fn pulse_width(&self) -> u32 {
        ((self.sample_rate * 0.005).round() as u32).max(1)
    }
}

impl Module for ClockMult {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        3
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let clock = port(inputs, Self::IN_CLOCK, ctx);
        let (x2, rest) = outputs.split_at_mut(ctx.frames);
        let (x3, x4) = rest.split_at_mut(ctx.frames);
        let width = self.pulse_width();

        for i in 0..ctx.frames {
            let high = clock[i] > 0.0;
            if high && !self.prev_clock {
                if self.samples_since > 0 {
                    self.period = self.samples_since;
                    self.armed = true;
                    self.phase2 = 0;
                    self.phase3 = 0;
                    self.phase4 = 0;
                    // Master edge also fires all mults (downbeat).
                    self.pulse_left = [width, width, width];
                }
                self.samples_since = 0;
            } else {
                self.samples_since = self.samples_since.saturating_add(1);
            }
            self.prev_clock = high;

            if self.armed && self.period > 1 {
                let p = self.period;
                self.phase2 = self.phase2.saturating_add(1);
                self.phase3 = self.phase3.saturating_add(1);
                self.phase4 = self.phase4.saturating_add(1);

                // Intermediate edges only (master already pulsed on external clock).
                if self.phase2 >= p / 2 && p / 2 > 0 {
                    // one intermediate for ×2 between clocks
                    if self.phase2 == p / 2 {
                        self.pulse_left[0] = width;
                    }
                }
                if p / 3 > 0 {
                    if self.phase3 == p / 3 || self.phase3 == (2 * p) / 3 {
                        self.pulse_left[1] = width;
                    }
                }
                if p / 4 > 0 {
                    if self.phase4 == p / 4 || self.phase4 == p / 2 || self.phase4 == (3 * p) / 4 {
                        // skip p/2 if already covered — still fine to pulse
                        self.pulse_left[2] = width;
                    }
                }
            }

            x2[i] = if self.pulse_left[0] > 0 {
                self.pulse_left[0] -= 1;
                1.0
            } else {
                0.0
            };
            x3[i] = if self.pulse_left[1] > 0 {
                self.pulse_left[1] -= 1;
                1.0
            } else {
                0.0
            };
            x4[i] = if self.pulse_left[2] > 0 {
                self.pulse_left[2] -= 1;
                1.0
            } else {
                0.0
            };
        }
    }
}
