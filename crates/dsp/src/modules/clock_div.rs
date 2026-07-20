use crate::module::{port, BlockContext, Module};

/// Clock divider: rising edges on `clock` advance a counter; emits short
/// trigger pulses on /2, /4, /8, and /3 outputs. `reset` clears the count.
pub struct ClockDiv {
    sample_rate: f32,
    counter: u32,
    prev_clock: bool,
    prev_reset: bool,
    /// Samples remaining for each div output pulse.
    pulse_left: [u32; 4],
}

impl ClockDiv {
    pub const IN_CLOCK: usize = 0;
    pub const IN_RESET: usize = 1;

    pub const OUT_DIV2: usize = 0;
    pub const OUT_DIV4: usize = 1;
    pub const OUT_DIV8: usize = 2;
    pub const OUT_DIV3: usize = 3;

    const PULSE_MS: f32 = 5.0;
    /// Counters wrap on LCM(2,3,4,8)=24.
    const MOD: u32 = 24;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            counter: 0,
            prev_clock: false,
            prev_reset: false,
            pulse_left: [0; 4],
        }
    }

    fn pulse_width(&self) -> u32 {
        ((self.sample_rate * Self::PULSE_MS / 1000.0).round() as u32).max(1)
    }

    fn fire(&mut self, out: usize) {
        self.pulse_left[out] = self.pulse_width();
    }
}

impl Module for ClockDiv {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        4
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let clock = port(inputs, Self::IN_CLOCK, ctx);
        let reset = port(inputs, Self::IN_RESET, ctx);

        let (d2, rest) = outputs.split_at_mut(ctx.frames);
        let (d4, rest) = rest.split_at_mut(ctx.frames);
        let (d8, d3) = rest.split_at_mut(ctx.frames);

        for i in 0..ctx.frames {
            let clock_open = clock[i] > 0.0;
            let reset_open = reset[i] > 0.0;

            if reset_open && !self.prev_reset {
                self.counter = 0;
                self.pulse_left = [0; 4];
            } else if clock_open && !self.prev_clock {
                self.counter = (self.counter + 1) % Self::MOD;
                let c = self.counter;
                // Fire on the first edge of each division period.
                if c % 2 == 0 {
                    self.fire(Self::OUT_DIV2);
                }
                if c % 4 == 0 {
                    self.fire(Self::OUT_DIV4);
                }
                if c % 8 == 0 {
                    self.fire(Self::OUT_DIV8);
                }
                if c % 3 == 0 {
                    self.fire(Self::OUT_DIV3);
                }
            }
            self.prev_clock = clock_open;
            self.prev_reset = reset_open;

            d2[i] = if self.pulse_left[0] > 0 { 1.0 } else { 0.0 };
            d4[i] = if self.pulse_left[1] > 0 { 1.0 } else { 0.0 };
            d8[i] = if self.pulse_left[2] > 0 { 1.0 } else { 0.0 };
            d3[i] = if self.pulse_left[3] > 0 { 1.0 } else { 0.0 };
            for p in &mut self.pulse_left {
                *p = p.saturating_sub(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn div2_fires_half_as_often() {
        let mut d = ClockDiv::new(48_000.0);
        // Alternate 0,1 for 16 edges of high.
        let frames = 32;
        let mut inputs = vec![0.0; frames * 2];
        for i in 0..frames {
            inputs[i] = if i % 2 == 1 { 1.0 } else { 0.0 };
        }
        let mut outputs = vec![0.0; frames * 4];
        let ctx = BlockContext {
            sample_rate: 48_000.0,
            frames,
        };
        d.process(&inputs, &mut outputs, &ctx);
        let div2: usize = outputs[..frames].iter().filter(|&&x| x > 0.5).count();
        let div4: usize = outputs[frames..frames * 2]
            .iter()
            .filter(|&&x| x > 0.5)
            .count();
        // 16 rising edges → div2 on even counts ≈ half the rising edges fire.
        assert!(div2 > 0 && div4 > 0);
        assert!(div2 >= div4, "div2={div2} div4={div4}");
    }
}
