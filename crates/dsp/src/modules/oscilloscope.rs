use crate::module::{port, port_mut, BlockContext, Module};
use crate::MAX_BLOCK_FRAMES;

/// An oscilloscope: a pass-through module that taps the signal at its
/// input and makes the most recent block available for inspection (a TUI
/// scope panel, a future analysis pass, ...). It copies its input
/// straight to its output so dropping it into a patch changes nothing
/// about what flows downstream — it's a probe, not a processor.
pub struct Oscilloscope {
    /// The most recent block's samples, captured from the input on every
    /// `process` call. Sized to the graph's max block so the copy never
    /// reallocates.
    last_block: Vec<f32>,
}

impl Oscilloscope {
    pub const IN_AUDIO: usize = 0;
    pub const OUT_AUDIO: usize = 0;

    pub fn new() -> Self {
        Self {
            last_block: vec![0.0; MAX_BLOCK_FRAMES],
        }
    }

    /// The samples captured from the most recent `process` call (the full
    /// `MAX_BLOCK_FRAMES`-long buffer, only the first `frames` of which
    /// were written on the last block).
    pub fn last_block(&self) -> &[f32] {
        &self.last_block
    }
}

impl Default for Oscilloscope {
    fn default() -> Self {
        Self::new()
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
        out.copy_from_slice(audio); // pass-through; see struct doc
        self.last_block[..ctx.frames].copy_from_slice(audio); // capture
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

    #[test]
    fn passes_input_through_unchanged() {
        let mut scope = Oscilloscope::new();
        let input = [0.1, -0.2, 0.3, -0.4];
        let mut output = [0.0; 4];
        scope.process(&input, &mut output, &ctx(4));
        assert_eq!(output, input);
    }

    #[test]
    fn captures_the_most_recent_block() {
        let mut scope = Oscilloscope::new();
        let first = [1.0, 2.0, 3.0, 4.0];
        let second = [-1.0, -2.0, -3.0, -4.0];
        let mut output = [0.0; 4];

        scope.process(&first, &mut output, &ctx(4));
        assert_eq!(&scope.last_block()[..4], &first);

        scope.process(&second, &mut output, &ctx(4));
        assert_eq!(&scope.last_block()[..4], &second);
    }

    #[test]
    fn capture_holds_across_a_shorter_block() {
        let mut scope = Oscilloscope::new();
        let block = [0.5; 8];
        let mut output = [0.0; 8];
        scope.process(&block, &mut output, &ctx(8));

        // A subsequent shorter block (2 frames) overwrites only its prefix;
        // we only ever read the last-written `frames`, so this is fine.
        let short = [0.25, 0.75];
        let mut short_out = [0.0; 2];
        scope.process(&short, &mut short_out, &ctx(2));
        assert_eq!(&scope.last_block()[..2], &short);
    }
}
