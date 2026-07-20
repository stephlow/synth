use crate::graph::Graph;
use crate::module::BlockContext;
use crate::modules::{Attenuverter, Envelope, Io, Lfo, MidiToCv, Oscilloscope, Vca, Vcf, Vco};
use crate::MAX_BLOCK_FRAMES;

/// Stable endpoint id for the master hardware output (`Synth::master_io`).
pub const IO_ENDPOINT_AUDIO_OUT: &str = "audio_out";
/// Stable endpoint id for external capture into every voice's `Io`.
pub const IO_ENDPOINT_AUDIO_IN: &str = "audio_in";
/// Device name meaning "resolve the host default at open time".
pub const IO_DEVICE_DEFAULT: &str = "default";

/// Describes one parameter of a module a `Patch` placed in the graph, for
/// anything that wants to build a generic control surface (a gRPC descriptor
/// table, a MIDI CC map, a future patch-file format) without hardcoding
/// module/param indices itself.
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct ModuleInfo {
    pub module: usize,
    pub name: &'static str,
    pub kind: &'static str,
    pub params: Vec<ParamInfo>,
}

/// Direction of a hardware-facing IO endpoint declared by a patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoDirection {
    /// Capture from a device into the graph (`Module::write_input`).
    Input,
    /// Send graph signals out to a device (master `Io` send channels).
    Output,
}

/// Patch-owned binding for one hardware-facing `Io` endpoint. The engine
/// realizes `device` by opening the matching cpal stream; runtime device
/// changes mutate this binding (still patch state), then reopen hardware.
#[derive(Clone, Debug)]
pub struct IoEndpoint {
    /// Stable id within the patch (`audio_out`, `audio_in`).
    pub id: &'static str,
    /// Display name for control surfaces ("Audio Out").
    pub name: &'static str,
    /// Style/grouping key for clients; always `"io"` for these endpoints.
    pub kind: &'static str,
    pub direction: IoDirection,
    pub channels: usize,
    /// cpal device name, or [`IO_DEVICE_DEFAULT`] / empty for host default.
    pub device: String,
    /// First physical device channel to map to endpoint channel 0.
    pub device_channel_offset: usize,
    /// Voice-graph module index when this Io lives in each voice; `None`
    /// for master-level Io owned by `Synth` (e.g. audio out).
    pub module: Option<usize>,
    /// Calibration knobs (scale/offset) exposed on a control surface.
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
    /// Hardware IO endpoints this patch owns (device bindings + layout).
    /// Includes voice-graph capture and master-level send endpoints.
    pub io_endpoints: Vec<IoEndpoint>,
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
        // Device binding lives on `io_endpoints`; calibration knobs are
        // listed there (and as ModuleInfo) so the control surface can
        // show an "Audio In" IO module without hardcoding indices.

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

        // Capture calibration: one scale/offset pair per capture channel.
        let audio_in_params = vec![
            ParamInfo {
                name: "Ch1 Scale",
                param: Io::param_capture_scale(0),
                min: 0.0,
                max: 2.0,
                default_value: 1.0,
            },
            ParamInfo {
                name: "Ch1 Offset",
                param: Io::param_capture_offset(0),
                min: -1.0,
                max: 1.0,
                default_value: 0.0,
            },
            ParamInfo {
                name: "Ch2 Scale",
                param: Io::param_capture_scale(1),
                min: 0.0,
                max: 2.0,
                default_value: 1.0,
            },
            ParamInfo {
                name: "Ch2 Offset",
                param: Io::param_capture_offset(1),
                min: -1.0,
                max: 1.0,
                default_value: 0.0,
            },
        ];
        modules.push(ModuleInfo {
            module: io_in,
            name: "Audio In",
            kind: "io",
            params: audio_in_params.clone(),
        });

        // Master send calibration (applied by Synth::master_io, not a
        // voice-graph module — `module: None`). Channel 0 is mixed audio.
        let audio_out_params = vec![
            ParamInfo {
                name: "Audio Scale",
                param: Io::param_send_scale(0),
                min: 0.0,
                max: 2.0,
                default_value: 1.0,
            },
            ParamInfo {
                name: "Audio Offset",
                param: Io::param_send_offset(0),
                min: -1.0,
                max: 1.0,
                default_value: 0.0,
            },
            ParamInfo {
                name: "Pitch Scale",
                param: Io::param_send_scale(1),
                min: 0.0,
                max: 2.0,
                default_value: 1.0,
            },
            ParamInfo {
                name: "Pitch Offset",
                param: Io::param_send_offset(1),
                min: -1.0,
                max: 1.0,
                default_value: 0.0,
            },
            ParamInfo {
                name: "Gate Scale",
                param: Io::param_send_scale(2),
                min: 0.0,
                max: 2.0,
                default_value: 1.0,
            },
            ParamInfo {
                name: "Gate Offset",
                param: Io::param_send_offset(2),
                min: -1.0,
                max: 1.0,
                default_value: 0.0,
            },
        ];

        let io_endpoints = vec![
            IoEndpoint {
                id: IO_ENDPOINT_AUDIO_OUT,
                name: "Audio Out",
                kind: "io",
                direction: IoDirection::Output,
                channels: 4,
                device: IO_DEVICE_DEFAULT.to_string(),
                device_channel_offset: 0,
                module: None,
                params: audio_out_params,
            },
            IoEndpoint {
                id: IO_ENDPOINT_AUDIO_IN,
                name: "Audio In",
                kind: "io",
                direction: IoDirection::Input,
                channels: 2,
                device: IO_DEVICE_DEFAULT.to_string(),
                device_channel_offset: 0,
                module: Some(io_in),
                params: audio_in_params,
            },
        ];

        BuiltPatch {
            graph,
            note_source: midi_to_cv,
            activity_gate: amp_env,
            external_input: io_in,
            output_tap,
            modules,
            io_endpoints,
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
    /// module description and IO endpoints (see `BuiltPatch`) — callers that
    /// need only one voice's worth of description (e.g. `Synth::new`, which
    /// builds every voice from the same patch) can keep just the first.
    pub fn new(patch: &dyn Patch, sample_rate: f32) -> (Self, Vec<ModuleInfo>, Vec<IoEndpoint>) {
        let built = patch.build(sample_rate);
        let voice = Self {
            graph: built.graph,
            note_source: built.note_source,
            activity_gate: built.activity_gate,
            external_input: built.external_input,
            tap: built.output_tap,
            sample_rate,
        };
        (voice, built.modules, built.io_endpoints)
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
