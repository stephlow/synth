use crate::module::{port, port_mut, BlockContext, Module};

/// Sample & hold: latches `in` whenever `trigger` rises above 0.0,
/// otherwise holds its last value. Classic combined with `Noise` to
/// produce stepped random CV.
pub struct SampleHold {
    held: f32,
    prev_trigger_open: bool,
}

impl SampleHold {
    pub const IN_SIGNAL: usize = 0;
    pub const IN_TRIGGER: usize = 1;

    pub const OUT_HELD: usize = 0;

    pub fn new() -> Self {
        Self {
            held: 0.0,
            prev_trigger_open: false,
        }
    }
}

impl Default for SampleHold {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for SampleHold {
    fn input_count(&self) -> usize {
        2
    }

    fn output_count(&self) -> usize {
        1
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let signal = port(inputs, Self::IN_SIGNAL, ctx);
        let trigger = port(inputs, Self::IN_TRIGGER, ctx);
        let out = port_mut(outputs, Self::OUT_HELD, ctx);

        for i in 0..ctx.frames {
            let trigger_open = trigger[i] > 0.0;
            if trigger_open && !self.prev_trigger_open {
                self.held = signal[i];
            }
            self.prev_trigger_open = trigger_open;
            out[i] = self.held;
        }
    }
}
