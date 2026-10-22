use crate::module::{port_mut, BlockContext, Module};

/// Free-running random triggers at approximately `rate` Hz (± jitter).
pub struct RandomTrig {
    sample_rate: f32,
    rate_hz: f32,
    state: u32,
    countdown: u32,
    pulse_left: u32,
}

impl RandomTrig {
    pub const OUT: usize = 0;

    pub const PARAM_RATE: usize = 0;

    pub fn new(sample_rate: f32, seed: u32) -> Self {
        let mut s = Self {
            sample_rate: sample_rate.max(1.0),
            rate_hz: 4.0,
            state: if seed == 0 { 0xC0FFEE } else { seed },
            countdown: 0,
            pulse_left: 0,
        };
        s.rearm();
        s
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    fn rearm(&mut self) {
        let base = (self.sample_rate / self.rate_hz.max(0.05)).max(1.0);
        // Jitter ±50%.
        let j = (self.next_u32() as f32 / u32::MAX as f32) * 1.0 + 0.5;
        self.countdown = (base * j).round() as u32;
    }

    fn pulse_width(&self) -> u32 {
        ((self.sample_rate * 0.005).round() as u32).max(1)
    }
}

impl Module for RandomTrig {
    fn input_count(&self) -> usize {
        0
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, _inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let out = port_mut(outputs, Self::OUT, ctx);
        let width = self.pulse_width();
        for sample in out.iter_mut() {
            if self.countdown == 0 {
                self.pulse_left = width;
                self.rearm();
            } else {
                self.countdown -= 1;
            }
            *sample = if self.pulse_left > 0 {
                self.pulse_left -= 1;
                1.0
            } else {
                0.0
            };
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_RATE {
            self.rate_hz = value.clamp(0.05, 40.0);
        }
    }
}
