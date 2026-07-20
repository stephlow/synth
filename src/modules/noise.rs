use crate::module::{port_mut, BlockContext, Module};

/// White noise source. Each instance owns its own xorshift32 generator
/// (seeded from a per-instance value) so voices don't share state or need
/// any global/thread-local RNG.
pub struct Noise {
    state: u32,
}

impl Noise {
    pub const OUT_WHITE: usize = 0;

    pub fn new(seed: u32) -> Self {
        Self {
            // xorshift32 requires a non-zero state.
            state: if seed == 0 { 0x9E3779B9 } else { seed },
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }
}

impl Module for Noise {
    fn input_count(&self) -> usize {
        0
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, _inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let out = port_mut(outputs, Self::OUT_WHITE, ctx);
        for sample in out.iter_mut() {
            // Normalize in f64 before the single cast down to f32, to
            // avoid rounding a full 32-bit sample through a 23-bit
            // mantissa twice.
            let unit = (self.next_u32() as f64 / f64::from(u32::MAX)) as f32; // 0..1
            *sample = unit * 2.0 - 1.0; // -1..1
        }
    }
}
