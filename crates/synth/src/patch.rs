use crate::graph::Graph;
use crate::module::BlockContext;
use crate::modules::{Attenuverter, Envelope, Io, Lfo, MidiToCv, Vca, Vcf, Vco};
use crate::MAX_BLOCK_FRAMES;

/// A voice is one patched graph instance plus the handles it needs to
/// drive from the outside (its note source, and the envelope that gates
/// whether it's safe to steal). `Synth` owns a pool of these.
pub struct Voice {
    graph: Graph,
    midi_mod: usize,
    amp_env_mod: usize,
    io_mod: usize,
    tap: usize,
    sample_rate: f32,
}

/// Module indices within the default patch, exposed so callers (MIDI CC
/// routing in `main.rs`) can target specific parameters without having to
/// rebuild the patch to know its own shape.
pub struct DefaultPatchModules {
    pub midi_to_cv: usize,
    pub vco: usize,
    pub amp_env: usize,
    pub filter_env: usize,
    pub lfo: usize,
    pub lfo_depth: usize,
    pub vcf: usize,
    pub vca: usize,
    /// Bridges externally-captured audio/CV into this voice (see
    /// `Voice::write_input`); channel 0 is summed into the VCF alongside
    /// the VCO, so external audio can be processed through the same
    /// filter/envelope/VCA chain as the synth voice itself.
    pub io_in: usize,
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
///   tap = Vca.out
fn build_default_patch(sample_rate: f32) -> (Graph, DefaultPatchModules, usize) {
    let mut graph = Graph::new();

    let midi_to_cv = graph.add_module(Box::new(MidiToCv::new()));
    let vco = graph.add_module(Box::new(Vco::new(sample_rate)));
    let io_in = graph.add_module(Box::new(Io::new(0, 2))); // capture-only: 2 external-in channels
    let amp_env = graph.add_module(Box::new(Envelope::new(sample_rate, 0.01, 0.1, 0.7, 0.3)));
    let filter_env = graph.add_module(Box::new(Envelope::new(sample_rate, 0.05, 0.2, 0.3, 0.4)));
    let lfo = graph.add_module(Box::new(Lfo::new(sample_rate)));
    let lfo_depth = graph.add_module(Box::new(Attenuverter::new()));
    let vcf = graph.add_module(Box::new(Vcf::new(sample_rate)));
    let vca = graph.add_module(Box::new(Vca::new()));

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

    let tap = graph.add_tap(vca, Vca::OUT_AUDIO);

    // Mild default LFO-to-cutoff depth (in octaves) so filter modulation
    // is present but not overwhelming out of the box; CC can widen it.
    graph.set_param(lfo_depth, Attenuverter::PARAM_SCALE, 0.2);
    graph.set_param(vcf, Vcf::PARAM_CUTOFF, 1200.0);
    graph.set_param(vcf, Vcf::PARAM_RESONANCE, 0.15);

    graph.build(MAX_BLOCK_FRAMES);

    (
        graph,
        DefaultPatchModules {
            midi_to_cv,
            vco,
            amp_env,
            filter_env,
            lfo,
            lfo_depth,
            vcf,
            vca,
            io_in,
        },
        tap,
    )
}

impl Voice {
    pub fn new_default(sample_rate: f32) -> (Self, DefaultPatchModules) {
        let (graph, modules, tap) = build_default_patch(sample_rate);
        let voice = Self {
            graph,
            midi_mod: modules.midi_to_cv,
            amp_env_mod: modules.amp_env,
            io_mod: modules.io_in,
            tap,
            sample_rate,
        };
        (voice, modules)
    }

    pub fn note_on(&mut self, freq_hz: f32, velocity: f32) {
        let pitch_volts = (freq_hz / 440.0).log2();
        self.graph
            .set_param(self.midi_mod, MidiToCv::PARAM_PITCH, pitch_volts);
        self.graph
            .set_param(self.midi_mod, MidiToCv::PARAM_VELOCITY, velocity);
        self.graph.set_param(self.midi_mod, MidiToCv::PARAM_GATE, 1.0);
    }

    pub fn note_off(&mut self) {
        self.graph.set_param(self.midi_mod, MidiToCv::PARAM_GATE, 0.0);
    }

    pub fn is_active(&self) -> bool {
        self.graph.is_module_active(self.amp_env_mod)
    }

    /// Feeds a block of externally-captured audio/CV into this voice's
    /// `Io` module (see `DefaultPatchModules::io_in`).
    pub fn write_input(&mut self, port: usize, samples: &[f32]) {
        self.graph.write_input(self.io_mod, port, samples);
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
