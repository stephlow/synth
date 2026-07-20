use crate::graph::Graph;
use crate::module::BlockContext;
use crate::modules::{Attenuverter, Envelope, Io, Lfo, MidiToCv, Oscilloscope, Vca, Vcf, Vco};
use crate::MAX_BLOCK_FRAMES;

/// Describes one parameter of a module a `Patch` placed in the graph, for
/// anything that wants to build a generic control surface (a gRPC descriptor
/// table, a MIDI CC map, a future patch-file format) without hardcoding
/// module/param indices itself.
pub struct ParamInfo {
    pub name: &'static str,
    pub param: usize,
    pub min: f32,
    pub max: f32,
    pub default_value: f32,
}

/// Describes one module a `Patch` placed in the graph: its display name and
/// style/grouping "kind" (the patch's choice — the same underlying module
/// type can mean something different in a different patch, e.g. an
/// `Attenuverter` used as "Mod Depth" in one patch vs. a general-purpose
/// utility in another), plus every parameter worth exposing on a control
/// surface.
pub struct ModuleInfo {
    pub module: usize,
    pub name: &'static str,
    pub kind: &'static str,
    pub params: Vec<ParamInfo>,
}

/// What `Patch::build` returns: the built graph, the handles `Voice` needs
/// to drive it from the outside, and a description of every module placed
/// in it. Module indices in `modules` are the same indices `graph` actually
/// assigned (captured as `build` calls `Graph::add_module`) — there's no
/// separate, hand-kept-in-sync index list to drift out of date.
pub struct BuiltPatch {
    pub graph: Graph,
    /// Module `Voice::note_on`/`note_off` target (a `MidiToCv`-shaped
    /// module: `PARAM_PITCH`/`PARAM_GATE`/`PARAM_VELOCITY`).
    pub note_source: usize,
    /// Module `Voice::is_active` reads — gates whether a voice is safe to
    /// steal (see `Module::is_active`).
    pub activity_gate: usize,
    /// Module `Voice::write_input` feeds externally-captured audio/CV into.
    pub external_input: usize,
    /// Tap index (see `Graph::add_tap`/`Graph::tap`) for this voice's
    /// rendered audio output.
    pub output_tap: usize,
    /// Every module worth exposing on a control surface, in a stable order.
    pub modules: Vec<ModuleInfo>,
}

/// A patch: owns building a voice graph and describing what it built, so
/// `Synth`/`Voice` aren't hardcoded to one voice architecture and a generic
/// control surface (gRPC descriptors, MIDI CC) can be derived from whatever
/// patch is actually running instead of re-describing it by hand.
/// `DefaultPatch` is the only implementation today; the trait is the seam
/// for future patches (a second built-in patch, eventually a patch file).
pub trait Patch {
    /// Builds the voice graph for `sample_rate`, ready for
    /// `Graph::process_block` once wrapped in a `Voice`.
    fn build(&self, sample_rate: f32) -> BuiltPatch;
}

/// The default patch: a classic subtractive voice.
///
///   MidiToCv.pitch --------------------------> Vco.pitch
///   Vco.saw ---------------------------------> Vcf.audio
///   Io.ch0 (external capture) ----------------> Vcf.audio  (summed with the above)
///   FilterEnv.cv ---------------------------> Vcf.cutoff_cv
///   Lfo.bipolar -> Attenuverter (depth) -----> Vcf.cutoff_cv  (sums with the above)
///   Vcf.lowpass ------------------------------> Vca.audio
///   AmpEnv.cv --------------------------------> Vca.cv
///   MidiToCv.gate -----------------------------> AmpEnv.gate, FilterEnv.gate
///   Vca.out ------------------------------------> Scope.in
///   tap = Scope.out
pub struct DefaultPatch;

impl Patch for DefaultPatch {
    fn build(&self, sample_rate: f32) -> BuiltPatch {
        let mut graph = Graph::new();
        let mut modules = Vec::new();

        let midi_to_cv = graph.add_module(Box::new(MidiToCv::new()));
        // Not exposed as a control: driven by note events, not a knob.

        let vco = graph.add_module(Box::new(Vco::new(sample_rate)));
        modules.push(ModuleInfo {
            module: vco,
            name: "VCO",
            kind: "vco",
            params: vec![ParamInfo { name: "Tune", param: Vco::PARAM_TUNE, min: -2.0, max: 2.0, default_value: 0.0 }],
        });

        let io_in = graph.add_module(Box::new(Io::new(0, 2))); // capture-only: 2 external-in channels
        // Not exposed as a control: driven by hardware capture, not a knob.

        let amp_env = graph.add_module(Box::new(Envelope::new(sample_rate, 0.01, 0.1, 0.7, 0.3)));
        modules.push(ModuleInfo {
            module: amp_env,
            name: "Envelope",
            kind: "envelope",
            params: envelope_params([0.01, 0.1, 0.7, 0.3]),
        });

        let filter_env = graph.add_module(Box::new(Envelope::new(sample_rate, 0.05, 0.2, 0.3, 0.4)));
        modules.push(ModuleInfo {
            module: filter_env,
            name: "Envelope",
            kind: "envelope",
            params: envelope_params([0.05, 0.2, 0.3, 0.4]),
        });

        let lfo = graph.add_module(Box::new(Lfo::new(sample_rate)));
        modules.push(ModuleInfo {
            module: lfo,
            name: "LFO",
            kind: "lfo",
            params: vec![ParamInfo { name: "Rate", param: Lfo::PARAM_RATE, min: 0.1, max: 20.0, default_value: 2.0 }],
        });

        let lfo_depth = graph.add_module(Box::new(Attenuverter::new()));
        modules.push(ModuleInfo {
            module: lfo_depth,
            name: "Attenuverter",
            kind: "attenuverter",
            params: vec![
                ParamInfo { name: "Scale", param: Attenuverter::PARAM_SCALE, min: 0.0, max: 2.0, default_value: 0.2 },
                ParamInfo { name: "Offset", param: Attenuverter::PARAM_OFFSET, min: -1.0, max: 1.0, default_value: 0.0 },
            ],
        });

        let vcf = graph.add_module(Box::new(Vcf::new(sample_rate)));
        modules.push(ModuleInfo {
            module: vcf,
            name: "VCF",
            kind: "vcf",
            params: vec![
                ParamInfo { name: "Cutoff", param: Vcf::PARAM_CUTOFF, min: 20.0, max: 10_000.0, default_value: 1200.0 },
                ParamInfo { name: "Resonance", param: Vcf::PARAM_RESONANCE, min: 0.0, max: 1.0, default_value: 0.15 },
            ],
        });

        let vca = graph.add_module(Box::new(Vca::new()));
        modules.push(ModuleInfo {
            module: vca,
            name: "VCA",
            kind: "vca",
            params: vec![ParamInfo { name: "Level", param: Vca::PARAM_LEVEL, min: 0.0, max: 1.5, default_value: 1.0 }],
        });

        let scope = graph.add_module(Box::new(Oscilloscope::new()));
        // Not exposed as a control: it's a pass-through probe (see its
        // doc); its terminal node is the voice's output tap.

        graph.connect(midi_to_cv, MidiToCv::OUT_PITCH, vco, Vco::IN_PITCH);
        graph.connect(vco, Vco::OUT_SAW, vcf, Vcf::IN_AUDIO);
        graph.connect(io_in, 0, vcf, Vcf::IN_AUDIO);
        graph.connect(filter_env, Envelope::OUT_CV, vcf, Vcf::IN_CUTOFF_CV);
        graph.connect(lfo, Lfo::OUT_BIPOLAR, lfo_depth, Attenuverter::IN_SIGNAL);
        graph.connect(lfo_depth, Attenuverter::OUT_SIGNAL, vcf, Vcf::IN_CUTOFF_CV);
        graph.connect(vcf, Vcf::OUT_LOWPASS, vca, Vca::IN_AUDIO);
        graph.connect(amp_env, Envelope::OUT_CV, vca, Vca::IN_CV);
        graph.connect(midi_to_cv, MidiToCv::OUT_GATE, amp_env, Envelope::IN_GATE);
        graph.connect(midi_to_cv, MidiToCv::OUT_GATE, filter_env, Envelope::IN_GATE);
        graph.connect(vca, Vca::OUT_AUDIO, scope, Oscilloscope::IN_AUDIO);

        let output_tap = graph.add_tap(scope, Oscilloscope::OUT_AUDIO);

        // Mild default LFO-to-cutoff depth (in octaves) so filter modulation
        // is present but not overwhelming out of the box.
        graph.set_param(lfo_depth, Attenuverter::PARAM_SCALE, 0.2);
        graph.set_param(vcf, Vcf::PARAM_CUTOFF, 1200.0);
        graph.set_param(vcf, Vcf::PARAM_RESONANCE, 0.15);

        graph.build(MAX_BLOCK_FRAMES);

        BuiltPatch {
            graph,
            note_source: midi_to_cv,
            activity_gate: amp_env,
            external_input: io_in,
            output_tap,
            modules,
        }
    }
}

fn envelope_params(defaults: [f32; 4]) -> Vec<ParamInfo> {
    vec![
        ParamInfo { name: "Attack", param: Envelope::PARAM_ATTACK, min: 0.001, max: 2.0, default_value: defaults[0] },
        ParamInfo { name: "Decay", param: Envelope::PARAM_DECAY, min: 0.001, max: 2.0, default_value: defaults[1] },
        ParamInfo { name: "Sustain", param: Envelope::PARAM_SUSTAIN, min: 0.0, max: 1.0, default_value: defaults[2] },
        ParamInfo { name: "Release", param: Envelope::PARAM_RELEASE, min: 0.01, max: 5.0, default_value: defaults[3] },
    ]
}

/// A voice is one patched graph instance plus the handles it needs to
/// drive from the outside (its note source, and the activity gate that
/// tells the voice-allocator whether it's safe to steal). `Synth` owns a
/// pool of these, all built from the same `Patch`.
pub struct Voice {
    graph: Graph,
    note_source: usize,
    activity_gate: usize,
    external_input: usize,
    tap: usize,
    sample_rate: f32,
}

impl Voice {
    /// Builds a voice from `patch`, returning it alongside the patch's full
    /// module description (see `BuiltPatch::modules`) — callers that need
    /// only one voice's worth of description (e.g. `Synth::new`, which
    /// builds every voice from the same patch) can keep just the first.
    pub fn new(patch: &dyn Patch, sample_rate: f32) -> (Self, Vec<ModuleInfo>) {
        let built = patch.build(sample_rate);
        let voice = Self {
            graph: built.graph,
            note_source: built.note_source,
            activity_gate: built.activity_gate,
            external_input: built.external_input,
            tap: built.output_tap,
            sample_rate,
        };
        (voice, built.modules)
    }

    pub fn note_on(&mut self, freq_hz: f32, velocity: f32) {
        let pitch_volts = (freq_hz / 440.0).log2();
        self.graph
            .set_param(self.note_source, MidiToCv::PARAM_PITCH, pitch_volts);
        self.graph
            .set_param(self.note_source, MidiToCv::PARAM_VELOCITY, velocity);
        self.graph.set_param(self.note_source, MidiToCv::PARAM_GATE, 1.0);
    }

    pub fn note_off(&mut self) {
        self.graph.set_param(self.note_source, MidiToCv::PARAM_GATE, 0.0);
    }

    pub fn is_active(&self) -> bool {
        self.graph.is_module_active(self.activity_gate)
    }

    /// This voice's external-input module index (see
    /// `BuiltPatch::external_input`), for a caller (`Synth`) that needs to
    /// target it directly (e.g. calibration) rather than through
    /// `write_input`.
    pub fn external_input_module(&self) -> usize {
        self.external_input
    }

    /// Feeds a block of externally-captured audio/CV into this voice's
    /// external-input module (see `BuiltPatch::external_input`).
    pub fn write_input(&mut self, port: usize, samples: &[f32]) {
        self.graph.write_input(self.external_input, port, samples);
    }

    /// Renders this voice's patch for `out.len()` frames (must be
    /// `<= MAX_BLOCK_FRAMES`) into `out`, overwriting it.
    pub fn process_block(&mut self, out: &mut [f32]) {
        let ctx = BlockContext {
            sample_rate: self.sample_rate,
            frames: out.len(),
        };
        self.graph.process_block(&ctx);
        out.copy_from_slice(self.graph.tap(self.tap));
    }

    pub fn set_param(&mut self, module: usize, param: usize, value: f32) {
        self.graph.set_param(module, param, value);
    }
}
