use crate::module::{port, port_mut, BlockContext, Module};
use crate::MAX_BLOCK_FRAMES;

/// Bridges signals across the boundary of this graph — send channels out
/// (patched in as inputs), capture channels in (fed externally via
/// `write_input`, available as outputs). What's actually on the other
/// side is up to whoever owns this instance: today that's a real audio
/// interface (`main.rs`), but it could just as well be another patch or
/// some other software boundary later — this module doesn't know or
/// care, and doesn't distinguish audio from CV (the engine already
/// treats them as the same signal, see `module.rs`).
///
/// A pure sink (only receiving from outside the graph) is
/// `Io::new(0, capture_channels)`; a pure source (only sending out) is
/// `Io::new(send_channels, 0)`; both directions can be used on one
/// instance if useful.
///
/// Every channel, in either direction, has an independent `scale`/
/// `offset` calibration (`out = in * scale + offset`, the same shape as
/// `Attenuverter`), defaulting to neutral (1.0/0.0 — unmodified
/// passthrough). Real hardware rarely uses this engine's internal
/// convention (a plain `f32`, `1.0` = full scale, 1V/oct pitch where
/// `1.0` = one octave) directly, so calibration is how you reconcile the
/// two — worked examples, not defaults, since the right numbers depend
/// entirely on whatever's actually plugged in:
/// - A **±10V full-scale, DC-coupled** interface (e.g. an Expert Sleepers
///   ES-8/ES-9) sending true 1V/oct pitch: `send_scale = 0.1`, so this
///   engine's `1.0` (one octave) becomes 1V at the jack, not 10V.
/// - The same interface sending a 0-5V gate: `send_scale = 0.5,
///   send_offset = 0.0` maps this engine's gate-high (`1.0`) to 5V.
/// - Reading a 0-10V external CV back in on a ±10V-scale input:
///   `capture_scale = 0.1` normalizes it back to this engine's `0.0..1.0`.
///
/// Most consumer audio interfaces (including plenty of otherwise great
/// ones, e.g. a Focusrite Scarlett) **AC-couple** their outputs, which
/// blocks true sustained/DC CV (a held pitch voltage, a long gate) no
/// matter what calibration is applied — fast-changing signals (LFO-rate/
/// audio-rate CV, short triggers) still pass through fine. Genuine
/// DC-coupled CV needs DC-coupled hardware; that's a hardware
/// limitation, not something calibration (or any software) can fix.
pub struct Io {
    send_channels: usize,
    capture_channels: usize,
    send_scale: Vec<f32>,
    send_offset: Vec<f32>,
    capture_scale: Vec<f32>,
    capture_offset: Vec<f32>,
    /// One `MAX_BLOCK_FRAMES`-long region per capture channel, holding
    /// whatever was last written via `write_input` (before calibration).
    captured: Vec<f32>,
}

impl Io {
    pub fn new(send_channels: usize, capture_channels: usize) -> Self {
        Self {
            send_channels,
            capture_channels,
            send_scale: vec![1.0; send_channels],
            send_offset: vec![0.0; send_channels],
            capture_scale: vec![1.0; capture_channels],
            capture_offset: vec![0.0; capture_channels],
            captured: vec![0.0; capture_channels * MAX_BLOCK_FRAMES],
        }
    }

    /// `set_param` index for send channel `channel`'s scale.
    pub const fn param_send_scale(channel: usize) -> usize {
        channel * 4
    }
    /// `set_param` index for send channel `channel`'s offset.
    pub const fn param_send_offset(channel: usize) -> usize {
        channel * 4 + 1
    }
    /// `set_param` index for capture channel `channel`'s scale.
    pub const fn param_capture_scale(channel: usize) -> usize {
        channel * 4 + 2
    }
    /// `set_param` index for capture channel `channel`'s offset.
    pub const fn param_capture_offset(channel: usize) -> usize {
        channel * 4 + 3
    }
}

impl Module for Io {
    fn input_count(&self) -> usize {
        self.send_channels
    }

    fn output_count(&self) -> usize {
        // Send-mirror ports, then capture ports, back to back.
        self.send_channels + self.capture_channels
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        // Calibrate and echo each send input through, so what's actually
        // being sent out (post-calibration) can still be read back via
        // the ordinary tap mechanism.
        for channel in 0..self.send_channels {
            let input = port(inputs, channel, ctx);
            let (scale, offset) = (self.send_scale[channel], self.send_offset[channel]);
            let out = port_mut(outputs, channel, ctx);
            for (o, &i) in out.iter_mut().zip(input) {
                *o = i * scale + offset;
            }
        }

        // Calibrate whatever was last captured via write_input (silence
        // if nothing's been written yet, e.g. no input device connected).
        for channel in 0..self.capture_channels {
            let base = channel * MAX_BLOCK_FRAMES;
            let (scale, offset) = (self.capture_scale[channel], self.capture_offset[channel]);
            let captured = &self.captured[base..base + ctx.frames];
            let out = port_mut(outputs, self.send_channels + channel, ctx);
            for (o, &c) in out.iter_mut().zip(captured) {
                *o = c * scale + offset;
            }
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        let channel = param / 4;
        match param % 4 {
            0 => {
                if let Some(s) = self.send_scale.get_mut(channel) {
                    *s = value;
                }
            }
            1 => {
                if let Some(o) = self.send_offset.get_mut(channel) {
                    *o = value;
                }
            }
            2 => {
                if let Some(s) = self.capture_scale.get_mut(channel) {
                    *s = value;
                }
            }
            3 => {
                if let Some(o) = self.capture_offset.get_mut(channel) {
                    *o = value;
                }
            }
            _ => unreachable!(),
        }
    }

    fn write_input(&mut self, port: usize, samples: &[f32]) {
        if port >= self.capture_channels {
            return;
        }
        let base = port * MAX_BLOCK_FRAMES;
        self.captured[base..base + samples.len()].copy_from_slice(samples);
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
    fn capture_reads_as_silence_until_written() {
        let mut io = Io::new(0, 2);
        let mut out = [0.0; 8]; // 2 channels * 4 frames
        io.process(&[], &mut out, &ctx(4));
        assert_eq!(out, [0.0; 8]);
    }

    #[test]
    fn capture_emits_the_last_written_block_per_channel() {
        let mut io = Io::new(0, 2);
        io.write_input(0, &[1.0, 2.0, 3.0, 4.0]);
        io.write_input(1, &[-1.0, -2.0, -3.0, -4.0]);

        let mut out = [0.0; 8];
        io.process(&[], &mut out, &ctx(4));
        assert_eq!(&out[0..4], &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(&out[4..8], &[-1.0, -2.0, -3.0, -4.0]);
    }

    #[test]
    fn send_passes_every_channel_straight_through_by_default() {
        let mut io = Io::new(2, 0);
        let inputs = [1.0, 2.0, 3.0, 4.0, -1.0, -2.0, -3.0, -4.0];
        let mut outputs = [0.0; 8];

        io.process(&inputs, &mut outputs, &ctx(4));
        assert_eq!(outputs, inputs);
    }

    #[test]
    fn send_and_capture_channels_are_independent_on_one_instance() {
        let mut io = Io::new(1, 1);
        io.write_input(0, &[9.0, 9.0]); // capture channel 0

        let inputs = [5.0, 5.0]; // send channel 0
        let mut outputs = [0.0; 4]; // send-mirror (1 ch) + capture (1 ch), 2 frames each

        io.process(&inputs, &mut outputs, &ctx(2));
        assert_eq!(&outputs[0..2], &[5.0, 5.0], "send-mirror should echo the send input");
        assert_eq!(&outputs[2..4], &[9.0, 9.0], "capture should be unaffected by the send side");
    }

    #[test]
    fn send_calibration_scales_and_offsets_the_mirrored_output() {
        let mut io = Io::new(1, 0);
        io.set_param(Io::param_send_scale(0), 0.1);
        io.set_param(Io::param_send_offset(0), 0.5);

        let inputs = [1.0, 1.0];
        let mut outputs = [0.0; 2];
        io.process(&inputs, &mut outputs, &ctx(2));
        // 1.0 * 0.1 + 0.5 = 0.6
        assert_eq!(outputs, [0.6, 0.6]);
    }

    #[test]
    fn capture_calibration_scales_and_offsets_the_captured_signal() {
        let mut io = Io::new(0, 1);
        io.set_param(Io::param_capture_scale(0), 0.1);
        io.set_param(Io::param_capture_offset(0), -1.0);
        io.write_input(0, &[10.0, 10.0]);

        let mut outputs = [0.0; 2];
        io.process(&[], &mut outputs, &ctx(2));
        // 10.0 * 0.1 - 1.0 = 0.0
        assert_eq!(outputs, [0.0, 0.0]);
    }
}
