use crate::module::{BlockContext, Module};

/// A single patch cable: one source output port feeding one destination
/// input port. Multiple cables into the same input sum together.
struct Connection {
    src_mod: usize,
    src_port: usize,
    dst_mod: usize,
    dst_port: usize,
}

/// A patchable graph of modules, processed one **block** of frames at a
/// time (matching the audio callback's buffer size) rather than one
/// sample at a time. This amortizes the virtual `Module::process` call
/// over the whole block and lets each module's inner loop vectorize,
/// instead of paying dispatch + gather/scatter overhead on every sample.
///
/// Built once (add modules, add connections, then [`Graph::build`]) for a
/// fixed maximum block size; every `process_block` after that is
/// allocation-free. Every port has a fixed-stride storage region sized
/// to that maximum, so a block shorter than the maximum (e.g. a partial
/// final block) is handled by simply using less of each region.
///
/// A feedback cable (a connection whose source is later than its
/// destination in evaluation order) reads that source's output from the
/// *previous* block: the arena persists block to block, and within a
/// block modules run strictly in `order`, so a not-yet-re-evaluated
/// source is still holding last block's data. This is the block-rate
/// analogue of the sample-accurate delay a per-sample engine gives
/// feedback for free — here the delay is one block instead of one
/// sample.
pub struct Graph {
    modules: Vec<Box<dyn Module>>,
    connections: Vec<Connection>,

    /// Evaluation order (module indices), computed by `build`.
    order: Vec<usize>,
    /// Per-module starting "port slot" among all modules' output ports.
    output_port_base: Vec<usize>,
    /// Per-module starting "port slot" among all modules' input ports.
    input_port_base: Vec<usize>,

    /// Persistent, block-strided storage of every module's last-computed
    /// output block, one `max_frames`-wide region per output port. Read
    /// back to build each module's gathered inputs and to serve the tap.
    arena: Vec<f32>,
    /// Scratch, block-strided storage of every module's gathered inputs
    /// for the block currently being processed, one `max_frames`-wide
    /// region per input port.
    input_scratch: Vec<f32>,
    /// Scratch, densely packed (no unused stride) buffer a module writes
    /// its outputs into for the block currently being processed; scattered
    /// into `arena` right after. Sized to the widest module's outputs.
    output_scratch: Vec<f32>,
    /// Densely packed counterpart of `input_scratch`, handed to the
    /// module being processed (dense so `Module::process`'s port stride
    /// is always exactly `ctx.frames`, regardless of `max_frames`).
    dense_input_scratch: Vec<f32>,

    /// Per global input-port-slot: the global output-port-slots feeding
    /// it (summed together). Empty = unpatched (reads as silence).
    input_sources: Vec<Vec<usize>>,

    /// Output taps: `(module, port)` pairs that can be read back after a
    /// `process_block` call via `tap(index)`. A graph can have any number
    /// of taps (e.g. one per physical output channel).
    taps: Vec<(usize, usize)>,
    /// Frame count from the most recent `process_block` call, so `tap`
    /// knows how much of each strided arena region to expose.
    last_frames: usize,
    max_frames: usize,
    built: bool,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
            connections: Vec::new(),
            order: Vec::new(),
            output_port_base: Vec::new(),
            input_port_base: Vec::new(),
            arena: Vec::new(),
            input_scratch: Vec::new(),
            output_scratch: Vec::new(),
            dense_input_scratch: Vec::new(),
            input_sources: Vec::new(),
            taps: Vec::new(),
            last_frames: 0,
            max_frames: 0,
            built: false,
        }
    }

    /// Add a module to the graph, returning its index for use in
    /// `connect` / `set_tap` / `module_mut`.
    pub fn add_module(&mut self, module: Box<dyn Module>) -> usize {
        self.built = false;
        self.modules.push(module);
        self.modules.len() - 1
    }

    /// Patch `src_mod`'s `src_port` output into `dst_mod`'s `dst_port` input.
    pub fn connect(&mut self, src_mod: usize, src_port: usize, dst_mod: usize, dst_port: usize) {
        self.built = false;
        self.connections.push(Connection {
            src_mod,
            src_port,
            dst_mod,
            dst_port,
        });
    }

    /// Registers a module output as readable after `process_block`,
    /// returning a tap index for use with `tap()`. A graph can have any
    /// number of taps.
    pub fn add_tap(&mut self, module: usize, port: usize) -> usize {
        self.taps.push((module, port));
        self.taps.len() - 1
    }

    pub fn module_mut(&mut self, index: usize) -> &mut dyn Module {
        self.modules[index].as_mut()
    }

    pub fn is_module_active(&self, index: usize) -> bool {
        self.modules[index].is_active()
    }

    pub fn set_param(&mut self, module: usize, param: usize, value: f32) {
        self.modules[module].set_param(param, value);
    }

    /// Feeds a block of externally-captured samples into `module`'s
    /// `port` (see `Module::write_input`). Call before `process_block`
    /// for the block being built.
    pub fn write_input(&mut self, module: usize, port: usize, samples: &[f32]) {
        self.modules[module].write_input(port, samples);
    }

    /// Finalize the graph for blocks of up to `max_frames` frames:
    /// compute evaluation order, lay out fixed-stride port storage, and
    /// precompute input gather lists. Must be called before
    /// `process_block`; safe to call again (e.g. after adding more
    /// modules/connections, or to change `max_frames`).
    pub fn build(&mut self, max_frames: usize) {
        self.max_frames = max_frames;
        let n = self.modules.len();

        // Assign each module a contiguous run of output/input "port
        // slots" among all modules' output/input ports respectively.
        self.output_port_base = Vec::with_capacity(n);
        let mut out_slots = 0;
        for m in &self.modules {
            self.output_port_base.push(out_slots);
            out_slots += m.output_count();
        }
        self.input_port_base = Vec::with_capacity(n);
        let mut in_slots = 0;
        for m in &self.modules {
            self.input_port_base.push(in_slots);
            in_slots += m.input_count();
        }

        self.arena = vec![0.0; out_slots * max_frames];
        self.input_scratch = vec![0.0; in_slots * max_frames];

        let max_inputs = self.modules.iter().map(|m| m.input_count()).max().unwrap_or(0);
        let max_outputs = self.modules.iter().map(|m| m.output_count()).max().unwrap_or(0);
        self.dense_input_scratch = vec![0.0; max_inputs * max_frames];
        self.output_scratch = vec![0.0; max_outputs * max_frames];

        // Kahn's algorithm topo-sort; on a cycle (feedback patch) append
        // whatever remains in index order — those back-edges read stale
        // (previous-block) values, which is the intended behavior.
        let mut in_degree = vec![0usize; n];
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];
        for c in &self.connections {
            in_degree[c.dst_mod] += 1;
            dependents[c.src_mod].push(c.dst_mod);
        }

        let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);
        let mut visited = vec![false; n];
        while let Some(m) = queue.pop() {
            if visited[m] {
                continue;
            }
            visited[m] = true;
            order.push(m);
            for &dep in &dependents[m] {
                if in_degree[dep] > 0 {
                    in_degree[dep] -= 1;
                    if in_degree[dep] == 0 {
                        queue.push(dep);
                    }
                }
            }
        }
        for (i, was_visited) in visited.iter().enumerate() {
            if !was_visited {
                order.push(i);
            }
        }
        self.order = order;

        // Precompute, per global input-port-slot, the global
        // output-port-slots to sum.
        self.input_sources = vec![Vec::new(); in_slots];
        for c in &self.connections {
            let src_slot = self.output_port_base[c.src_mod] + c.src_port;
            let dst_slot = self.input_port_base[c.dst_mod] + c.dst_port;
            self.input_sources[dst_slot].push(src_slot);
        }

        self.built = true;
    }

    /// Advance every module by one block. `build()` must have been
    /// called first with `max_frames >= ctx.frames`. Allocation-free.
    /// After this returns, any tap's block can be read via `tap(index)`.
    pub fn process_block(&mut self, ctx: &BlockContext) {
        debug_assert!(self.built, "Graph::build() must be called before process_block()");
        debug_assert!(ctx.frames <= self.max_frames);
        self.last_frames = ctx.frames;

        let stride = self.max_frames;
        let frames = ctx.frames;

        for &m in &self.order {
            let in_count = self.modules[m].input_count();
            let out_count = self.modules[m].output_count();
            let in_base = self.input_port_base[m];
            let out_base = self.output_port_base[m];

            // Gather this module's inputs for the block, densely packed
            // (stride == frames) so the module sees a plain, tightly
            // packed buffer regardless of the graph's strided storage.
            // Unpatched ports read as silence, a single source is a
            // straight copy, multiple sources are summed — all at block
            // granularity, not once per sample.
            for port in 0..in_count {
                let sources = &self.input_sources[in_base + port];
                let dst_start = port * frames;
                let dst = &mut self.dense_input_scratch[dst_start..dst_start + frames];
                match sources.as_slice() {
                    [] => dst.fill(0.0),
                    [single] => {
                        let src_start = single * stride;
                        dst.copy_from_slice(&self.arena[src_start..src_start + frames]);
                    }
                    many => {
                        dst.fill(0.0);
                        for &src in many {
                            let src_start = src * stride;
                            let src_region = &self.arena[src_start..src_start + frames];
                            for (d, s) in dst.iter_mut().zip(src_region) {
                                *d += s;
                            }
                        }
                    }
                }
            }

            let inputs = &self.dense_input_scratch[..in_count * frames];
            let outputs = &mut self.output_scratch[..out_count * frames];
            self.modules[m].process(inputs, outputs, ctx);

            // Scatter the densely packed outputs into their strided arena
            // regions (also block-rate, not per-sample).
            for port in 0..out_count {
                let src_start = port * frames;
                let dst_start = (out_base + port) * stride;
                self.arena[dst_start..dst_start + frames]
                    .copy_from_slice(&outputs[src_start..src_start + frames]);
            }
        }
    }

    /// Reads back tap `index`'s block from the most recent
    /// `process_block` call (`last_frames` samples long).
    pub fn tap(&self, index: usize) -> &[f32] {
        let (m, port) = self.taps[index];
        let start = (self.output_port_base[m] + port) * self.max_frames;
        &self.arena[start..start + self.last_frames]
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::{port, port_mut};

    /// Emits a fixed value every sample, ignoring its (nonexistent) inputs.
    struct Const(f32);
    impl Module for Const {
        fn input_count(&self) -> usize {
            0
        }
        fn output_count(&self) -> usize {
            1
        }
        fn process(&mut self, _inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
            port_mut(outputs, 0, ctx).fill(self.0);
        }
    }

    /// Multiplies its single input by a fixed gain — a minimal stand-in
    /// for a VCA in wiring tests.
    struct Gain(f32);
    impl Module for Gain {
        fn input_count(&self) -> usize {
            1
        }
        fn output_count(&self) -> usize {
            1
        }
        fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
            let gain = self.0;
            for (o, i) in port_mut(outputs, 0, ctx).iter_mut().zip(port(inputs, 0, ctx)) {
                *o = i * gain;
            }
        }
    }

    /// Outputs `input + 1.0` — used to prove that a self-feedback cable
    /// reads exactly the previous block's output, not the current one.
    struct Accumulator;
    impl Module for Accumulator {
        fn input_count(&self) -> usize {
            1
        }
        fn output_count(&self) -> usize {
            1
        }
        fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
            for (o, i) in port_mut(outputs, 0, ctx).iter_mut().zip(port(inputs, 0, ctx)) {
                *o = i + 1.0;
            }
        }
    }

    fn ctx(frames: usize) -> BlockContext {
        BlockContext {
            sample_rate: 48000.0,
            frames,
        }
    }

    #[test]
    fn evaluates_a_simple_chain_in_order() {
        let mut graph = Graph::new();
        let src = graph.add_module(Box::new(Const(2.0)));
        let gain = graph.add_module(Box::new(Gain(3.0)));
        graph.connect(src, 0, gain, 0);
        let tap = graph.add_tap(gain, 0);
        graph.build(64);

        graph.process_block(&ctx(4));
        assert_eq!(graph.tap(tap), [6.0; 4]);
        graph.process_block(&ctx(4));
        assert_eq!(graph.tap(tap), [6.0; 4]);
    }

    #[test]
    fn multiple_cables_into_one_input_sum() {
        let mut graph = Graph::new();
        let a = graph.add_module(Box::new(Const(2.0)));
        let b = graph.add_module(Box::new(Const(5.0)));
        let gain = graph.add_module(Box::new(Gain(1.0)));
        graph.connect(a, 0, gain, 0);
        graph.connect(b, 0, gain, 0);
        let tap = graph.add_tap(gain, 0);
        graph.build(64);

        graph.process_block(&ctx(4));
        assert_eq!(graph.tap(tap), [7.0; 4]);
    }

    #[test]
    fn feedback_cable_reads_the_previous_block() {
        let mut graph = Graph::new();
        let acc = graph.add_module(Box::new(Accumulator));
        // Feed the accumulator's own output back into its input.
        graph.connect(acc, 0, acc, 0);
        let tap = graph.add_tap(acc, 0);
        graph.build(64);

        // Block 0: input reads the initial (zero) stored output for the
        // whole block -> every frame is 0+1=1 (no per-sample feedback
        // within the block, since it wasn't re-evaluated yet).
        graph.process_block(&ctx(4));
        assert_eq!(graph.tap(tap), [1.0; 4]);
        // Block 1: input reads block 0's last-written value (1) for the
        // whole block -> 1+1=2, i.e. an exact one-*block* delay.
        graph.process_block(&ctx(4));
        assert_eq!(graph.tap(tap), [2.0; 4]);
        graph.process_block(&ctx(4));
        assert_eq!(graph.tap(tap), [3.0; 4]);
    }

    #[test]
    fn a_block_shorter_than_the_built_max_is_handled_without_resizing() {
        let mut graph = Graph::new();
        let src = graph.add_module(Box::new(Const(1.0)));
        let tap = graph.add_tap(src, 0);
        graph.build(64); // built for up to 64 frames...

        graph.process_block(&ctx(3)); // ...but this block only uses 3.
        assert_eq!(graph.tap(tap), [1.0; 3]);
    }

    #[test]
    fn multiple_taps_can_be_read_after_one_process_block_call() {
        let mut graph = Graph::new();
        let a = graph.add_module(Box::new(Const(2.0)));
        let b = graph.add_module(Box::new(Const(5.0)));
        let tap_a = graph.add_tap(a, 0);
        let tap_b = graph.add_tap(b, 0);
        graph.build(64);

        graph.process_block(&ctx(4));
        assert_eq!(graph.tap(tap_a), [2.0; 4]);
        assert_eq!(graph.tap(tap_b), [5.0; 4]);
    }

    #[test]
    fn write_input_reaches_the_target_module() {
        // A module that just emits whatever's fed via write_input, for
        // exercising Graph::write_input's forwarding.
        struct Passthrough(Vec<f32>);
        impl Module for Passthrough {
            fn input_count(&self) -> usize {
                0
            }
            fn output_count(&self) -> usize {
                1
            }
            fn process(&mut self, _inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
                port_mut(outputs, 0, ctx).copy_from_slice(&self.0[..ctx.frames]);
            }
            fn write_input(&mut self, port: usize, samples: &[f32]) {
                if port == 0 {
                    self.0[..samples.len()].copy_from_slice(samples);
                }
            }
        }

        let mut graph = Graph::new();
        let m = graph.add_module(Box::new(Passthrough(vec![0.0; 64])));
        let tap = graph.add_tap(m, 0);
        graph.build(64);

        graph.write_input(m, 0, &[9.0, 8.0, 7.0, 6.0]);
        graph.process_block(&ctx(4));
        assert_eq!(graph.tap(tap), [9.0, 8.0, 7.0, 6.0]);
    }
}
