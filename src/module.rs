/// Shared per-block processing context passed to every module.
pub struct BlockContext {
    pub sample_rate: f32,
    /// Number of frames (samples) in this block. `inputs`/`outputs`
    /// passed to `Module::process` are densely packed with this stride:
    /// port `p`'s block is `[p * frames .. (p + 1) * frames]`.
    pub frames: usize,
}

/// A single modular "unit": a black box with a fixed number of input and
/// output ports (all plain `f32` — audio and CV are the same kind of
/// signal) plus optional named parameters that can be tweaked live (knobs,
/// MIDI CC targets). Processing happens one block at a time; a module
/// typically implements `process` as a `for i in 0..ctx.frames` loop over
/// its own per-sample logic (per port, via `port(..)`/`port_mut(..)`),
/// carrying state (phase, filter memory, ...) across calls.
pub trait Module: Send {
    fn input_count(&self) -> usize;
    fn output_count(&self) -> usize;

    /// Compute this block: `inputs` holds `input_count()` port-blocks
    /// back to back (each already gathered by the graph — summed across
    /// every cable feeding it, silence if unpatched), each `ctx.frames`
    /// samples long. `outputs` must be fully written the same way, one
    /// `ctx.frames`-long block per output port.
    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext);

    /// Set a parameter (knob) by index. Default is a no-op so modules with
    /// no parameters don't need to implement this.
    #[allow(unused_variables)]
    fn set_param(&mut self, param: usize, value: f32) {}

    /// Whether this module is currently "doing something" that shouldn't
    /// be interrupted — used by the voice allocator to avoid stealing a
    /// voice whose envelope is still in its release tail. Modules with no
    /// notion of activity (oscillators, filters, ...) default to `true`
    /// (no special protection needed/possible).
    fn is_active(&self) -> bool {
        true
    }

    /// Feed a block of externally-captured samples (audio or CV, from
    /// outside this graph) into output `port`, to be emitted on the next
    /// `process` call. Default is a no-op; only modules that bridge
    /// external signals into the graph (`Io`) implement this — it's the
    /// generic mechanism for that, so those modules don't need their own
    /// bespoke API.
    #[allow(unused_variables)]
    fn write_input(&mut self, port: usize, samples: &[f32]) {}
}

/// Slices out port `port`'s block (`ctx.frames` samples) from a module's
/// densely packed `inputs`/`outputs` slice.
pub fn port<'a>(buf: &'a [f32], port: usize, ctx: &BlockContext) -> &'a [f32] {
    let base = port * ctx.frames;
    &buf[base..base + ctx.frames]
}

/// Mutable counterpart of [`port`].
pub fn port_mut<'a>(buf: &'a mut [f32], port: usize, ctx: &BlockContext) -> &'a mut [f32] {
    let base = port * ctx.frames;
    &mut buf[base..base + ctx.frames]
}
