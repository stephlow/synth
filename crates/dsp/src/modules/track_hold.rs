use crate::module::{port, port_mut, BlockContext, Module};

/// Track & hold: follows `signal` while `gate` high; holds last value when low.
pub struct TrackHold {
    held: f32,
}

impl TrackHold {
    pub const IN_SIGNAL: usize = 0;
    pub const IN_GATE: usize = 1;

    pub const OUT: usize = 0;

    pub fn new() -> Self {
        Self { held: 0.0 }
    }
}

impl Default for TrackHold {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for TrackHold {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let signal = port(inputs, Self::IN_SIGNAL, ctx);
        let gate = port(inputs, Self::IN_GATE, ctx);
        let out = port_mut(outputs, Self::OUT, ctx);
        for i in 0..ctx.frames {
            if gate[i] > 0.0 {
                self.held = signal[i];
            }
            out[i] = self.held;
        }
    }
}
