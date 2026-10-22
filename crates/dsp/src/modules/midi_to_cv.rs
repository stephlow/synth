use crate::module::{port_mut, BlockContext, Module};

/// Converts a held MIDI note into continuous CV: 1V/oct pitch, a gate
/// (1.0 while held, 0.0 released) and velocity. One instance per voice;
/// its state is written through the ordinary `set_param` port like any
/// other module's knobs, rather than through a bespoke API.
pub struct MidiToCv {
    pitch: f32,
    gate: f32,
    velocity: f32,
}

impl MidiToCv {
    pub const OUT_PITCH: usize = 0;
    pub const OUT_GATE: usize = 1;
    pub const OUT_VELOCITY: usize = 2;

    /// Written with the 1V/oct pitch (see `set_param`), not a raw frequency.
    pub const PARAM_PITCH: usize = 0;
    /// Nonzero opens the gate, `0.0` closes it.
    pub const PARAM_GATE: usize = 1;
    pub const PARAM_VELOCITY: usize = 2;

    pub fn new() -> Self {
        Self {
            pitch: 0.0,
            gate: 0.0,
            velocity: 0.0,
        }
    }
}

impl Default for MidiToCv {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for MidiToCv {
    fn input_count(&self) -> usize {
        0
    }

    fn output_count(&self) -> usize {
        3
    }

    fn process(&mut self, _inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        // Constant for the whole block: params only change between blocks.
        port_mut(outputs, Self::OUT_PITCH, ctx).fill(self.pitch);
        port_mut(outputs, Self::OUT_GATE, ctx).fill(self.gate);
        port_mut(outputs, Self::OUT_VELOCITY, ctx).fill(self.velocity);
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_PITCH => self.pitch = value,
            Self::PARAM_GATE => self.gate = value,
            Self::PARAM_VELOCITY => self.velocity = value,
            _ => {}
        }
    }

    /// Held notes keep the voice non-stealable while the gate is open.
    fn is_active(&self) -> bool {
        self.gate > 0.0
    }
}
