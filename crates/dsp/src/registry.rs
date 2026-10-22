//! Module type registry: factories and port/param metadata for compiling a
//! [`crate::patch::Patch`] into a graph and for control surfaces.
//!
//! Not used on the audio callback path — only at build / edit / describe time.

use crate::module::Module;
use crate::modules::{
    Abs, Attenuverter, Bernoulli, Bitcrush, Clip, ClockDiv, ClockMult, Comparator, Crossfade,
    Delay, Edge, Envelope, EnvFollow, Euclidean, Function, Io, Lfo, Link, Logic, Lpg, MinMax,
    MidiToCv, Mixer, Noise, Oscilloscope, Quantizer, RandomTrig, RingMod, SampleHold, Saturator,
    Sequencer, Slew, Switch, TrackHold, Vca, Vcf, Vco, Wavefolder,
};
use crate::modules::scale::{SCALE_INDEX_MAX, SCALE_MAJOR, SCALE_NAMES};
use crate::patch::ModuleInstance;

/// One named input or output port on a module type.
#[derive(Clone, Copy, Debug)]
pub struct PortMeta {
    pub name: &'static str,
    pub index: usize,
}

/// One named parameter (knob) on a module type.
#[derive(Clone, Copy, Debug)]
pub struct ParamMeta {
    pub name: &'static str,
    pub index: usize,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    /// Discrete value labels (index = rounded param). Empty = continuous.
    pub choices: &'static [&'static str],
}

/// Static description of a module kind available in the registry.
#[derive(Clone, Copy, Debug)]
pub struct ModuleTypeInfo {
    pub kind: &'static str,
    pub display_name: &'static str,
    pub inputs: &'static [PortMeta],
    pub outputs: &'static [PortMeta],
    pub params: &'static [ParamMeta],
}

type Factory = fn(sample_rate: f32, instance: &ModuleInstance) -> Box<dyn Module>;

struct Entry {
    info: ModuleTypeInfo,
    factory: Factory,
}

/// Look up a module kind's metadata, if registered.
pub fn type_info(kind: &str) -> Option<&'static ModuleTypeInfo> {
    entry(kind).map(|e| &e.info)
}

/// All registered module types (stable order for UI palettes).
pub fn all_types() -> Vec<&'static ModuleTypeInfo> {
    ENTRIES.iter().map(|e| &e.info).collect()
}

/// Instantiate a module from its document instance. Returns `None` if `kind`
/// is unknown.
pub fn create(sample_rate: f32, instance: &ModuleInstance) -> Option<Box<dyn Module>> {
    entry(&instance.kind).map(|e| (e.factory)(sample_rate, instance))
}

fn entry(kind: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.info.kind == kind)
}

// --- factories ---------------------------------------------------------------

fn create_midi_to_cv(_sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(MidiToCv::new())
}

fn create_vco(sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(Vco::new(sr))
}

fn create_vcf(sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(Vcf::new(sr))
}

fn create_vca(_sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(Vca::new())
}

fn create_envelope(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let a = i.param_value(Envelope::PARAM_ATTACK, 0.01);
    let d = i.param_value(Envelope::PARAM_DECAY, 0.1);
    let s = i.param_value(Envelope::PARAM_SUSTAIN, 0.7);
    let r = i.param_value(Envelope::PARAM_RELEASE, 0.3);
    Box::new(Envelope::new(sr, a, d, s, r))
}

fn create_lfo(sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(Lfo::new(sr))
}

fn create_attenuverter(_sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(Attenuverter::new())
}

fn create_mixer(_sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(Mixer::new())
}

fn create_noise(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    // Stable seed from instance id so rebuilds don't reshuffle noise.
    Box::new(Noise::new(i.id.wrapping_add(1)))
}

fn create_sample_hold(_sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(SampleHold::new())
}

fn create_quantizer(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let scale = i.param_value(Quantizer::PARAM_SCALE, SCALE_MAJOR as f32);
    let root = i.param_value(Quantizer::PARAM_ROOT, 0.0);
    Box::new(Quantizer::with_params(scale, root))
}

fn create_sequencer(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut seq = Sequencer::new(sr);
    let steps = i.param_value(Sequencer::PARAM_STEPS, 8.0);
    let gate_len = i.param_value(Sequencer::PARAM_GATE_LEN, 0.5);
    seq.set_param(Sequencer::PARAM_STEPS, steps);
    seq.set_param(Sequencer::PARAM_GATE_LEN, gate_len);
    for n in 0..Sequencer::MAX_STEPS {
        let p = Sequencer::param_step(n);
        // Defaults match Sequencer::new motif for the first 8, else 0.
        let default = match n {
            0 => 0.0 / 12.0,
            1 => 2.0 / 12.0,
            2 => 4.0 / 12.0,
            3 => 5.0 / 12.0,
            4 => 7.0 / 12.0,
            5 => 9.0 / 12.0,
            6 => 11.0 / 12.0,
            7 => 12.0 / 12.0,
            _ => 0.0,
        };
        let v = i.param_value(p, default);
        seq.set_param(p, v);
    }
    Box::new(seq)
}

fn create_oscilloscope(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Oscilloscope::new(sr);
    m.set_param(
        Oscilloscope::PARAM_TIME,
        i.param_value(Oscilloscope::PARAM_TIME, Oscilloscope::DEFAULT_TIME_MS),
    );
    m.set_param(
        Oscilloscope::PARAM_Y_SCALE,
        i.param_value(Oscilloscope::PARAM_Y_SCALE, Oscilloscope::DEFAULT_Y_SCALE),
    );
    Box::new(m)
}

fn create_slew(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Slew::new(sr);
    m.set_param(
        Slew::PARAM_RISE,
        i.param_value(Slew::PARAM_RISE, 0.05),
    );
    m.set_param(
        Slew::PARAM_FALL,
        i.param_value(Slew::PARAM_FALL, 0.05),
    );
    Box::new(m)
}

fn create_clock_div(sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(ClockDiv::new(sr))
}

fn create_delay(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Delay::new(sr);
    m.set_param(Delay::PARAM_TIME, i.param_value(Delay::PARAM_TIME, 250.0));
    m.set_param(
        Delay::PARAM_FEEDBACK,
        i.param_value(Delay::PARAM_FEEDBACK, 0.35),
    );
    m.set_param(Delay::PARAM_MIX, i.param_value(Delay::PARAM_MIX, 0.3));
    Box::new(m)
}

fn create_wavefolder(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Wavefolder::new();
    m.set_param(
        Wavefolder::PARAM_DRIVE,
        i.param_value(Wavefolder::PARAM_DRIVE, 1.5),
    );
    m.set_param(
        Wavefolder::PARAM_SYMMETRY,
        i.param_value(Wavefolder::PARAM_SYMMETRY, 0.0),
    );
    m.set_param(
        Wavefolder::PARAM_MIX,
        i.param_value(Wavefolder::PARAM_MIX, 1.0),
    );
    Box::new(m)
}

fn create_lpg(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Lpg::new(sr);
    m.set_param(
        Lpg::PARAM_OFFSET,
        i.param_value(Lpg::PARAM_OFFSET, 0.0),
    );
    Box::new(m)
}

fn create_function(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Function::new(sr);
    m.set_param(
        Function::PARAM_ATTACK,
        i.param_value(Function::PARAM_ATTACK, 0.01),
    );
    m.set_param(
        Function::PARAM_DECAY,
        i.param_value(Function::PARAM_DECAY, 0.2),
    );
    m.set_param(
        Function::PARAM_CYCLE,
        i.param_value(Function::PARAM_CYCLE, 0.0),
    );
    Box::new(m)
}

fn create_ring_mod(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = RingMod::new();
    m.set_param(
        RingMod::PARAM_LEVEL,
        i.param_value(RingMod::PARAM_LEVEL, 1.0),
    );
    Box::new(m)
}

fn create_comparator(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Comparator::new();
    m.set_param(
        Comparator::PARAM_THRESH,
        i.param_value(Comparator::PARAM_THRESH, 0.0),
    );
    Box::new(m)
}

fn create_logic(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Logic::new();
    m.set_param(Logic::PARAM_MODE, i.param_value(Logic::PARAM_MODE, 0.0));
    Box::new(m)
}

fn create_crossfade(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Crossfade::new();
    m.set_param(
        Crossfade::PARAM_FADE,
        i.param_value(Crossfade::PARAM_FADE, 0.5),
    );
    Box::new(m)
}

fn create_edge(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Edge::new(sr);
    m.set_param(
        Edge::PARAM_WIDTH,
        i.param_value(Edge::PARAM_WIDTH, 5.0),
    );
    Box::new(m)
}

fn create_bernoulli(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Bernoulli::new(sr, i.id.wrapping_add(17));
    m.set_param(
        Bernoulli::PARAM_CHANCE,
        i.param_value(Bernoulli::PARAM_CHANCE, 0.5),
    );
    Box::new(m)
}

fn create_euclidean(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Euclidean::new(sr);
    m.set_param(
        Euclidean::PARAM_STEPS,
        i.param_value(Euclidean::PARAM_STEPS, 16.0),
    );
    m.set_param(
        Euclidean::PARAM_FILL,
        i.param_value(Euclidean::PARAM_FILL, 4.0),
    );
    m.set_param(
        Euclidean::PARAM_ROTATE,
        i.param_value(Euclidean::PARAM_ROTATE, 0.0),
    );
    Box::new(m)
}

fn create_clock_mult(sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(ClockMult::new(sr))
}

fn create_link(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Link::new(sr);
    m.set_param(Link::PARAM_BPM, i.param_value(Link::PARAM_BPM, 120.0));
    m.set_param(
        Link::PARAM_QUANTUM,
        i.param_value(Link::PARAM_QUANTUM, 4.0),
    );
    m.set_param(Link::PARAM_DIV, i.param_value(Link::PARAM_DIV, 2.0)); // index 2 → div 4
    m.set_param(Link::PARAM_PLAY, i.param_value(Link::PARAM_PLAY, 1.0));
    Box::new(m)
}

fn create_random_trig(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = RandomTrig::new(sr, i.id.wrapping_add(99));
    m.set_param(
        RandomTrig::PARAM_RATE,
        i.param_value(RandomTrig::PARAM_RATE, 4.0),
    );
    Box::new(m)
}

fn create_track_hold(_sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(TrackHold::new())
}

fn create_switch(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Switch::new();
    m.set_param(
        Switch::PARAM_STEPS,
        i.param_value(Switch::PARAM_STEPS, 4.0),
    );
    Box::new(m)
}

fn create_minmax(_sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(MinMax::new())
}

fn create_abs(_sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(Abs::new())
}

fn create_clip(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Clip::new();
    m.set_param(Clip::PARAM_LEVEL, i.param_value(Clip::PARAM_LEVEL, 1.0));
    Box::new(m)
}

fn create_saturator(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Saturator::new();
    m.set_param(
        Saturator::PARAM_DRIVE,
        i.param_value(Saturator::PARAM_DRIVE, 2.0),
    );
    m.set_param(
        Saturator::PARAM_MIX,
        i.param_value(Saturator::PARAM_MIX, 1.0),
    );
    Box::new(m)
}

fn create_bitcrush(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = Bitcrush::new();
    m.set_param(
        Bitcrush::PARAM_BITS,
        i.param_value(Bitcrush::PARAM_BITS, 8.0),
    );
    m.set_param(
        Bitcrush::PARAM_RATE,
        i.param_value(Bitcrush::PARAM_RATE, 0.25),
    );
    Box::new(m)
}

fn create_env_follow(sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let mut m = EnvFollow::new(sr);
    m.set_param(
        EnvFollow::PARAM_RISE,
        i.param_value(EnvFollow::PARAM_RISE, 0.01),
    );
    m.set_param(
        EnvFollow::PARAM_FALL,
        i.param_value(EnvFollow::PARAM_FALL, 0.1),
    );
    Box::new(m)
}

fn create_io(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let send = i.io_send_channels.unwrap_or(0);
    let capture = i.io_capture_channels.unwrap_or(2);
    Box::new(Io::new(send, capture))
}

// --- static tables -----------------------------------------------------------

/// Sequencer control metadata (Steps, Gate Len, Step 1..16).
static SEQUENCER_PARAMS: &[ParamMeta] = &[
    ParamMeta {
        name: "Steps",
        index: Sequencer::PARAM_STEPS,
        min: 1.0,
        max: 16.0,
        default: 8.0,
        choices: &[],
    },
    ParamMeta {
        name: "Gate Len",
        index: Sequencer::PARAM_GATE_LEN,
        min: 0.0,
        max: 1.0,
        default: 0.5,
        choices: &[],
    },
    ParamMeta {
        name: "Step 1",
        index: Sequencer::PARAM_STEP0,
        min: -2.0,
        max: 2.0,
        default: 0.0 / 12.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 2",
        index: Sequencer::PARAM_STEP0 + 1,
        min: -2.0,
        max: 2.0,
        default: 2.0 / 12.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 3",
        index: Sequencer::PARAM_STEP0 + 2,
        min: -2.0,
        max: 2.0,
        default: 4.0 / 12.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 4",
        index: Sequencer::PARAM_STEP0 + 3,
        min: -2.0,
        max: 2.0,
        default: 5.0 / 12.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 5",
        index: Sequencer::PARAM_STEP0 + 4,
        min: -2.0,
        max: 2.0,
        default: 7.0 / 12.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 6",
        index: Sequencer::PARAM_STEP0 + 5,
        min: -2.0,
        max: 2.0,
        default: 9.0 / 12.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 7",
        index: Sequencer::PARAM_STEP0 + 6,
        min: -2.0,
        max: 2.0,
        default: 11.0 / 12.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 8",
        index: Sequencer::PARAM_STEP0 + 7,
        min: -2.0,
        max: 2.0,
        default: 12.0 / 12.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 9",
        index: Sequencer::PARAM_STEP0 + 8,
        min: -2.0,
        max: 2.0,
        default: 0.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 10",
        index: Sequencer::PARAM_STEP0 + 9,
        min: -2.0,
        max: 2.0,
        default: 0.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 11",
        index: Sequencer::PARAM_STEP0 + 10,
        min: -2.0,
        max: 2.0,
        default: 0.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 12",
        index: Sequencer::PARAM_STEP0 + 11,
        min: -2.0,
        max: 2.0,
        default: 0.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 13",
        index: Sequencer::PARAM_STEP0 + 12,
        min: -2.0,
        max: 2.0,
        default: 0.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 14",
        index: Sequencer::PARAM_STEP0 + 13,
        min: -2.0,
        max: 2.0,
        default: 0.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 15",
        index: Sequencer::PARAM_STEP0 + 14,
        min: -2.0,
        max: 2.0,
        default: 0.0,
        choices: &[],
    },
    ParamMeta {
        name: "Step 16",
        index: Sequencer::PARAM_STEP0 + 15,
        min: -2.0,
        max: 2.0,
        default: 0.0,
        choices: &[],
    },
];

static ENTRIES: &[Entry] = &[
    Entry {
        info: ModuleTypeInfo {
            kind: "midi_to_cv",
            display_name: "MIDI to CV",
            inputs: &[],
            outputs: &[
                PortMeta {
                    name: "pitch",
                    index: MidiToCv::OUT_PITCH,
                },
                PortMeta {
                    name: "gate",
                    index: MidiToCv::OUT_GATE,
                },
                PortMeta {
                    name: "velocity",
                    index: MidiToCv::OUT_VELOCITY,
                },
            ],
            params: &[],
        },
        factory: create_midi_to_cv,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "vco",
            display_name: "VCO",
            inputs: &[
                PortMeta {
                    name: "pitch",
                    index: Vco::IN_PITCH,
                },
                PortMeta {
                    name: "fm",
                    index: Vco::IN_FM,
                },
            ],
            outputs: &[
                PortMeta {
                    name: "sine",
                    index: Vco::OUT_SINE,
                },
                PortMeta {
                    name: "saw",
                    index: Vco::OUT_SAW,
                },
                PortMeta {
                    name: "square",
                    index: Vco::OUT_SQUARE,
                },
                PortMeta {
                    name: "triangle",
                    index: Vco::OUT_TRIANGLE,
                },
            ],
            params: &[ParamMeta {
                name: "Tune",
                index: Vco::PARAM_TUNE,
                min: -2.0,
                max: 2.0,
                default: 0.0,
        choices: &[],
    }],
        },
        factory: create_vco,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "vcf",
            display_name: "VCF",
            inputs: &[
                PortMeta {
                    name: "audio",
                    index: Vcf::IN_AUDIO,
                },
                PortMeta {
                    name: "cutoff_cv",
                    index: Vcf::IN_CUTOFF_CV,
                },
                PortMeta {
                    name: "resonance_cv",
                    index: Vcf::IN_RESONANCE_CV,
                },
            ],
            outputs: &[
                PortMeta {
                    name: "lowpass",
                    index: Vcf::OUT_LOWPASS,
                },
                PortMeta {
                    name: "highpass",
                    index: Vcf::OUT_HIGHPASS,
                },
                PortMeta {
                    name: "bandpass",
                    index: Vcf::OUT_BANDPASS,
                },
            ],
            params: &[
                ParamMeta {
                    name: "Cutoff",
                    index: Vcf::PARAM_CUTOFF,
                    min: 20.0,
                    max: 10_000.0,
                    default: 1000.0,
        choices: &[],
    },
                ParamMeta {
                    name: "Resonance",
                    index: Vcf::PARAM_RESONANCE,
                    min: 0.0,
                    max: 1.0,
                    default: 0.2,
        choices: &[],
    },
            ],
        },
        factory: create_vcf,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "vca",
            display_name: "VCA",
            inputs: &[
                PortMeta {
                    name: "audio",
                    index: Vca::IN_AUDIO,
                },
                PortMeta {
                    name: "cv",
                    index: Vca::IN_CV,
                },
            ],
            outputs: &[PortMeta {
                name: "audio",
                index: Vca::OUT_AUDIO,
            }],
            params: &[ParamMeta {
                name: "Level",
                index: Vca::PARAM_LEVEL,
                min: 0.0,
                max: 1.5,
                default: 1.0,
        choices: &[],
    }],
        },
        factory: create_vca,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "envelope",
            display_name: "Envelope",
            inputs: &[PortMeta {
                name: "gate",
                index: Envelope::IN_GATE,
            }],
            outputs: &[PortMeta {
                name: "cv",
                index: Envelope::OUT_CV,
            }],
            params: &[
                ParamMeta {
                    name: "Attack",
                    index: Envelope::PARAM_ATTACK,
                    min: 0.001,
                    max: 2.0,
                    default: 0.01,
        choices: &[],
    },
                ParamMeta {
                    name: "Decay",
                    index: Envelope::PARAM_DECAY,
                    min: 0.001,
                    max: 2.0,
                    default: 0.1,
        choices: &[],
    },
                ParamMeta {
                    name: "Sustain",
                    index: Envelope::PARAM_SUSTAIN,
                    min: 0.0,
                    max: 1.0,
                    default: 0.7,
        choices: &[],
    },
                ParamMeta {
                    name: "Release",
                    index: Envelope::PARAM_RELEASE,
                    min: 0.01,
                    max: 5.0,
                    default: 0.3,
        choices: &[],
    },
            ],
        },
        factory: create_envelope,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "lfo",
            display_name: "LFO",
            inputs: &[PortMeta {
                name: "rate_cv",
                index: Lfo::IN_RATE_CV,
            }],
            outputs: &[
                PortMeta {
                    name: "sine",
                    index: Lfo::OUT_SINE,
                },
                PortMeta {
                    name: "saw",
                    index: Lfo::OUT_SAW,
                },
                PortMeta {
                    name: "square",
                    index: Lfo::OUT_SQUARE,
                },
                PortMeta {
                    name: "triangle",
                    index: Lfo::OUT_TRIANGLE,
                },
            ],
            params: &[ParamMeta {
                name: "Rate",
                index: Lfo::PARAM_RATE,
                min: 0.1,
                max: 20.0,
                default: 2.0,
                choices: &[],
            }],
        },
        factory: create_lfo,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "attenuverter",
            display_name: "Attenuverter",
            inputs: &[PortMeta {
                name: "signal",
                index: Attenuverter::IN_SIGNAL,
            }],
            outputs: &[PortMeta {
                name: "signal",
                index: Attenuverter::OUT_SIGNAL,
            }],
            params: &[
                ParamMeta {
                    name: "Scale",
                    index: Attenuverter::PARAM_SCALE,
                    // Full bipolar attenuverter range (mod depth + invert).
                    min: -2.0,
                    max: 2.0,
                    default: 1.0,
        choices: &[],
    },
                ParamMeta {
                    name: "Offset",
                    index: Attenuverter::PARAM_OFFSET,
                    min: -1.0,
                    max: 1.0,
                    default: 0.0,
        choices: &[],
    },
            ],
        },
        factory: create_attenuverter,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "mixer",
            display_name: "Mixer",
            inputs: &[
                PortMeta {
                    name: "in1",
                    index: Mixer::IN_1,
                },
                PortMeta {
                    name: "in2",
                    index: Mixer::IN_2,
                },
                PortMeta {
                    name: "in3",
                    index: Mixer::IN_3,
                },
                PortMeta {
                    name: "in4",
                    index: Mixer::IN_4,
                },
            ],
            outputs: &[PortMeta {
                name: "mix",
                index: Mixer::OUT_MIX,
            }],
            params: &[
                ParamMeta {
                    name: "Level 1",
                    index: Mixer::PARAM_LEVEL_1,
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
        choices: &[],
    },
                ParamMeta {
                    name: "Level 2",
                    index: Mixer::PARAM_LEVEL_2,
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
        choices: &[],
    },
                ParamMeta {
                    name: "Level 3",
                    index: Mixer::PARAM_LEVEL_3,
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
        choices: &[],
    },
                ParamMeta {
                    name: "Level 4",
                    index: Mixer::PARAM_LEVEL_4,
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
        choices: &[],
    },
            ],
        },
        factory: create_mixer,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "noise",
            display_name: "Noise",
            inputs: &[],
            outputs: &[
                PortMeta {
                    name: "white",
                    index: Noise::OUT_WHITE,
                },
                PortMeta {
                    name: "pink",
                    index: Noise::OUT_PINK,
                },
                PortMeta {
                    name: "red",
                    index: Noise::OUT_RED,
                },
            ],
            params: &[],
        },
        factory: create_noise,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "sample_hold",
            display_name: "Sample & Hold",
            inputs: &[
                PortMeta {
                    name: "signal",
                    index: SampleHold::IN_SIGNAL,
                },
                PortMeta {
                    name: "trigger",
                    index: SampleHold::IN_TRIGGER,
                },
            ],
            outputs: &[PortMeta {
                name: "held",
                index: SampleHold::OUT_HELD,
            }],
            params: &[],
        },
        factory: create_sample_hold,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "quantizer",
            display_name: "Quantizer",
            inputs: &[PortMeta {
                name: "cv",
                index: Quantizer::IN_CV,
            }],
            outputs: &[PortMeta {
                name: "cv",
                index: Quantizer::OUT_CV,
            }],
            params: &[
                ParamMeta {
                    name: "Scale",
                    index: Quantizer::PARAM_SCALE,
                    min: 0.0,
                    max: SCALE_INDEX_MAX,
                    default: SCALE_MAJOR as f32,
                    choices: SCALE_NAMES,
                },
                ParamMeta {
                    name: "Root",
                    index: Quantizer::PARAM_ROOT,
                    min: -12.0,
                    max: 12.0,
                    default: 0.0,
                    choices: &[],
                },
            ],
        },
        factory: create_quantizer,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "sequencer",
            display_name: "Sequencer",
            inputs: &[
                PortMeta {
                    name: "clock",
                    index: Sequencer::IN_CLOCK,
                },
                PortMeta {
                    name: "reset",
                    index: Sequencer::IN_RESET,
                },
            ],
            outputs: &[
                PortMeta {
                    name: "cv",
                    index: Sequencer::OUT_CV,
                },
                PortMeta {
                    name: "gate",
                    index: Sequencer::OUT_GATE,
                },
                PortMeta {
                    name: "trigger",
                    index: Sequencer::OUT_TRIGGER,
                },
            ],
            params: SEQUENCER_PARAMS,
        },
        factory: create_sequencer,
    },
    // --- East / West coast utilities ---
    Entry {
        info: ModuleTypeInfo {
            kind: "slew",
            display_name: "Slew",
            inputs: &[
                PortMeta {
                    name: "signal",
                    index: Slew::IN_SIGNAL,
                },
                PortMeta {
                    name: "rate_cv",
                    index: Slew::IN_RATE_CV,
                },
            ],
            outputs: &[PortMeta {
                name: "signal",
                index: Slew::OUT_SIGNAL,
            }],
            params: &[
                ParamMeta {
                    name: "Rise",
                    index: Slew::PARAM_RISE,
                    min: 0.001,
                    max: 2.0,
                    default: 0.05,
                    choices: &[],
                },
                ParamMeta {
                    name: "Fall",
                    index: Slew::PARAM_FALL,
                    min: 0.001,
                    max: 2.0,
                    default: 0.05,
                    choices: &[],
                },
            ],
        },
        factory: create_slew,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "clock_div",
            display_name: "Clock Div",
            inputs: &[
                PortMeta {
                    name: "clock",
                    index: ClockDiv::IN_CLOCK,
                },
                PortMeta {
                    name: "reset",
                    index: ClockDiv::IN_RESET,
                },
            ],
            outputs: &[
                PortMeta {
                    name: "div2",
                    index: ClockDiv::OUT_DIV2,
                },
                PortMeta {
                    name: "div4",
                    index: ClockDiv::OUT_DIV4,
                },
                PortMeta {
                    name: "div8",
                    index: ClockDiv::OUT_DIV8,
                },
                PortMeta {
                    name: "div3",
                    index: ClockDiv::OUT_DIV3,
                },
            ],
            params: &[],
        },
        factory: create_clock_div,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "delay",
            display_name: "Delay",
            inputs: &[
                PortMeta {
                    name: "audio",
                    index: Delay::IN_AUDIO,
                },
                PortMeta {
                    name: "time_cv",
                    index: Delay::IN_TIME_CV,
                },
                PortMeta {
                    name: "fb_cv",
                    index: Delay::IN_FB_CV,
                },
            ],
            outputs: &[PortMeta {
                name: "mix",
                index: Delay::OUT_MIX,
            }],
            params: &[
                ParamMeta {
                    name: "Time",
                    index: Delay::PARAM_TIME,
                    min: 1.0,
                    max: Delay::MAX_TIME_MS,
                    default: 250.0,
                    choices: &[],
                },
                ParamMeta {
                    name: "Feedback",
                    index: Delay::PARAM_FEEDBACK,
                    min: 0.0,
                    max: 0.95,
                    default: 0.35,
                    choices: &[],
                },
                ParamMeta {
                    name: "Mix",
                    index: Delay::PARAM_MIX,
                    min: 0.0,
                    max: 1.0,
                    default: 0.3,
                    choices: &[],
                },
            ],
        },
        factory: create_delay,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "wavefolder",
            display_name: "Wavefolder",
            inputs: &[
                PortMeta {
                    name: "audio",
                    index: Wavefolder::IN_AUDIO,
                },
                PortMeta {
                    name: "drive_cv",
                    index: Wavefolder::IN_DRIVE_CV,
                },
            ],
            outputs: &[PortMeta {
                name: "audio",
                index: Wavefolder::OUT_AUDIO,
            }],
            params: &[
                ParamMeta {
                    name: "Drive",
                    index: Wavefolder::PARAM_DRIVE,
                    min: 0.1,
                    max: 20.0,
                    default: 1.5,
                    choices: &[],
                },
                ParamMeta {
                    name: "Symmetry",
                    index: Wavefolder::PARAM_SYMMETRY,
                    min: -1.0,
                    max: 1.0,
                    default: 0.0,
                    choices: &[],
                },
                ParamMeta {
                    name: "Mix",
                    index: Wavefolder::PARAM_MIX,
                    min: 0.0,
                    max: 1.0,
                    default: 1.0,
                    choices: &[],
                },
            ],
        },
        factory: create_wavefolder,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "lpg",
            display_name: "LPG",
            inputs: &[
                PortMeta {
                    name: "audio",
                    index: Lpg::IN_AUDIO,
                },
                PortMeta {
                    name: "cv",
                    index: Lpg::IN_CV,
                },
            ],
            outputs: &[PortMeta {
                name: "audio",
                index: Lpg::OUT_AUDIO,
            }],
            params: &[ParamMeta {
                name: "Offset",
                index: Lpg::PARAM_OFFSET,
                min: 0.0,
                max: 1.0,
                default: 0.0,
                choices: &[],
            }],
        },
        factory: create_lpg,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "function",
            display_name: "Function",
            inputs: &[PortMeta {
                name: "trig",
                index: Function::IN_TRIG,
            }],
            outputs: &[
                PortMeta {
                    name: "cv",
                    index: Function::OUT_CV,
                },
                PortMeta {
                    name: "eoc",
                    index: Function::OUT_EOC,
                },
            ],
            params: &[
                ParamMeta {
                    name: "Attack",
                    index: Function::PARAM_ATTACK,
                    min: 0.001,
                    max: 5.0,
                    default: 0.01,
                    choices: &[],
                },
                ParamMeta {
                    name: "Decay",
                    index: Function::PARAM_DECAY,
                    min: 0.001,
                    max: 5.0,
                    default: 0.2,
                    choices: &[],
                },
                ParamMeta {
                    name: "Cycle",
                    index: Function::PARAM_CYCLE,
                    min: 0.0,
                    max: 1.0,
                    default: 0.0,
                    choices: &["Off", "On"],
                },
            ],
        },
        factory: create_function,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "ring_mod",
            display_name: "Ring Mod",
            inputs: &[
                PortMeta {
                    name: "a",
                    index: RingMod::IN_A,
                },
                PortMeta {
                    name: "b",
                    index: RingMod::IN_B,
                },
            ],
            outputs: &[PortMeta {
                name: "mix",
                index: RingMod::OUT,
            }],
            params: &[ParamMeta {
                name: "Level",
                index: RingMod::PARAM_LEVEL,
                min: 0.0,
                max: 2.0,
                default: 1.0,
                choices: &[],
            }],
        },
        factory: create_ring_mod,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "oscilloscope",
            display_name: "Scope",
            inputs: &[PortMeta {
                name: "audio",
                index: Oscilloscope::IN_AUDIO,
            }],
            outputs: &[PortMeta {
                name: "audio",
                index: Oscilloscope::OUT_AUDIO,
            }],
            params: &[
                ParamMeta {
                    name: "Time",
                    index: Oscilloscope::PARAM_TIME,
                    min: Oscilloscope::MIN_TIME_MS,
                    max: Oscilloscope::MAX_TIME_MS,
                    default: Oscilloscope::DEFAULT_TIME_MS,
                    choices: &[],
                },
                ParamMeta {
                    name: "Y Scale",
                    index: Oscilloscope::PARAM_Y_SCALE,
                    min: 0.1,
                    max: 20.0,
                    default: Oscilloscope::DEFAULT_Y_SCALE,
                    choices: &[],
                },
            ],
        },
        factory: create_oscilloscope,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "io",
            display_name: "IO",
            // Ports depend on channel counts; document uses fixed capture layout
            // for the default voice. Metadata lists the common capture outs.
            inputs: &[],
            outputs: &[
                PortMeta {
                    name: "ch0",
                    index: 0,
                },
                PortMeta {
                    name: "ch1",
                    index: 1,
                },
            ],
            params: &[
                ParamMeta {
                    name: "Ch1 Scale",
                    index: Io::param_capture_scale(0),
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
        choices: &[],
    },
                ParamMeta {
                    name: "Ch1 Offset",
                    index: Io::param_capture_offset(0),
                    min: -1.0,
                    max: 1.0,
                    default: 0.0,
        choices: &[],
    },
                ParamMeta {
                    name: "Ch2 Scale",
                    index: Io::param_capture_scale(1),
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
        choices: &[],
    },
                ParamMeta {
                    name: "Ch2 Offset",
                    index: Io::param_capture_offset(1),
                    min: -1.0,
                    max: 1.0,
                    default: 0.0,
        choices: &[],
    },
            ],
        },
        factory: create_io,
    },
    // --- Utility / logic ---
    Entry {
        info: ModuleTypeInfo {
            kind: "comparator",
            display_name: "Comparator",
            inputs: &[
                PortMeta {
                    name: "signal",
                    index: Comparator::IN_SIGNAL,
                },
                PortMeta {
                    name: "thresh_cv",
                    index: Comparator::IN_THRESH_CV,
                },
            ],
            outputs: &[
                PortMeta {
                    name: "gate",
                    index: Comparator::OUT_GATE,
                },
                PortMeta {
                    name: "not",
                    index: Comparator::OUT_NOT,
                },
            ],
            params: &[ParamMeta {
                name: "Thresh",
                index: Comparator::PARAM_THRESH,
                min: -1.0,
                max: 1.0,
                default: 0.0,
                choices: &[],
            }],
        },
        factory: create_comparator,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "logic",
            display_name: "Logic",
            inputs: &[
                PortMeta {
                    name: "a",
                    index: Logic::IN_A,
                },
                PortMeta {
                    name: "b",
                    index: Logic::IN_B,
                },
            ],
            outputs: &[PortMeta {
                name: "out",
                index: Logic::OUT,
            }],
            params: &[ParamMeta {
                name: "Mode",
                index: Logic::PARAM_MODE,
                min: 0.0,
                max: 3.0,
                default: 0.0,
                choices: &["AND", "OR", "XOR", "NOT"],
            }],
        },
        factory: create_logic,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "crossfade",
            display_name: "Crossfade",
            inputs: &[
                PortMeta {
                    name: "a",
                    index: Crossfade::IN_A,
                },
                PortMeta {
                    name: "b",
                    index: Crossfade::IN_B,
                },
                PortMeta {
                    name: "fade_cv",
                    index: Crossfade::IN_FADE_CV,
                },
            ],
            outputs: &[PortMeta {
                name: "mix",
                index: Crossfade::OUT,
            }],
            params: &[ParamMeta {
                name: "Fade",
                index: Crossfade::PARAM_FADE,
                min: 0.0,
                max: 1.0,
                default: 0.5,
                choices: &[],
            }],
        },
        factory: create_crossfade,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "edge",
            display_name: "Edge",
            inputs: &[PortMeta {
                name: "in",
                index: Edge::IN,
            }],
            outputs: &[PortMeta {
                name: "trig",
                index: Edge::OUT,
            }],
            params: &[ParamMeta {
                name: "Width",
                index: Edge::PARAM_WIDTH,
                min: 0.5,
                max: 50.0,
                default: 5.0,
                choices: &[],
            }],
        },
        factory: create_edge,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "minmax",
            display_name: "MinMax",
            inputs: &[
                PortMeta {
                    name: "a",
                    index: MinMax::IN_A,
                },
                PortMeta {
                    name: "b",
                    index: MinMax::IN_B,
                },
            ],
            outputs: &[
                PortMeta {
                    name: "min",
                    index: MinMax::OUT_MIN,
                },
                PortMeta {
                    name: "max",
                    index: MinMax::OUT_MAX,
                },
            ],
            params: &[],
        },
        factory: create_minmax,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "abs",
            display_name: "Abs",
            inputs: &[PortMeta {
                name: "in",
                index: Abs::IN,
            }],
            outputs: &[PortMeta {
                name: "out",
                index: Abs::OUT,
            }],
            params: &[],
        },
        factory: create_abs,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "clip",
            display_name: "Clip",
            inputs: &[
                PortMeta {
                    name: "in",
                    index: Clip::IN,
                },
                PortMeta {
                    name: "level_cv",
                    index: Clip::IN_LEVEL_CV,
                },
            ],
            outputs: &[PortMeta {
                name: "out",
                index: Clip::OUT,
            }],
            params: &[ParamMeta {
                name: "Level",
                index: Clip::PARAM_LEVEL,
                min: 0.01,
                max: 2.0,
                default: 1.0,
                choices: &[],
            }],
        },
        factory: create_clip,
    },
    // --- Rhythm / random ---
    Entry {
        info: ModuleTypeInfo {
            kind: "bernoulli",
            display_name: "Bernoulli",
            inputs: &[
                PortMeta {
                    name: "trig",
                    index: Bernoulli::IN_TRIG,
                },
                PortMeta {
                    name: "chance_cv",
                    index: Bernoulli::IN_CHANCE_CV,
                },
            ],
            outputs: &[
                PortMeta {
                    name: "a",
                    index: Bernoulli::OUT_A,
                },
                PortMeta {
                    name: "b",
                    index: Bernoulli::OUT_B,
                },
            ],
            params: &[ParamMeta {
                name: "Chance",
                index: Bernoulli::PARAM_CHANCE,
                min: 0.0,
                max: 1.0,
                default: 0.5,
                choices: &[],
            }],
        },
        factory: create_bernoulli,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "euclidean",
            display_name: "Euclidean",
            inputs: &[
                PortMeta {
                    name: "clock",
                    index: Euclidean::IN_CLOCK,
                },
                PortMeta {
                    name: "reset",
                    index: Euclidean::IN_RESET,
                },
            ],
            outputs: &[PortMeta {
                name: "gate",
                index: Euclidean::OUT_GATE,
            }],
            params: &[
                ParamMeta {
                    name: "Steps",
                    index: Euclidean::PARAM_STEPS,
                    min: 1.0,
                    max: 32.0,
                    default: 16.0,
                    choices: &[],
                },
                ParamMeta {
                    name: "Fill",
                    index: Euclidean::PARAM_FILL,
                    min: 0.0,
                    max: 32.0,
                    default: 4.0,
                    choices: &[],
                },
                ParamMeta {
                    name: "Rotate",
                    index: Euclidean::PARAM_ROTATE,
                    min: 0.0,
                    max: 31.0,
                    default: 0.0,
                    choices: &[],
                },
            ],
        },
        factory: create_euclidean,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "clock_mult",
            display_name: "Clock Mult",
            inputs: &[PortMeta {
                name: "clock",
                index: ClockMult::IN_CLOCK,
            }],
            outputs: &[
                PortMeta {
                    name: "x2",
                    index: ClockMult::OUT_X2,
                },
                PortMeta {
                    name: "x3",
                    index: ClockMult::OUT_X3,
                },
                PortMeta {
                    name: "x4",
                    index: ClockMult::OUT_X4,
                },
            ],
            params: &[],
        },
        factory: create_clock_mult,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "link",
            display_name: "Link",
            inputs: &[],
            outputs: &[
                PortMeta {
                    name: "clock",
                    index: Link::OUT_CLOCK,
                },
                PortMeta {
                    name: "phase",
                    index: Link::OUT_PHASE,
                },
                PortMeta {
                    name: "play",
                    index: Link::OUT_PLAY,
                },
            ],
            params: &[
                ParamMeta {
                    name: "BPM",
                    index: Link::PARAM_BPM,
                    min: 20.0,
                    max: 300.0,
                    default: 120.0,
                    choices: &[],
                },
                ParamMeta {
                    name: "Quantum",
                    index: Link::PARAM_QUANTUM,
                    min: 1.0,
                    max: 16.0,
                    default: 4.0,
                    choices: &[],
                },
                ParamMeta {
                    name: "Div",
                    index: Link::PARAM_DIV,
                    min: 0.0,
                    max: 3.0,
                    default: 2.0, // index → 4 clocks/beat
                    choices: &["1", "2", "4", "8"],
                },
                ParamMeta {
                    name: "Play",
                    index: Link::PARAM_PLAY,
                    min: 0.0,
                    max: 1.0,
                    default: 1.0,
                    choices: &["Off", "On"],
                },
            ],
        },
        factory: create_link,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "random_trig",
            display_name: "Random Trig",
            inputs: &[],
            outputs: &[PortMeta {
                name: "trig",
                index: RandomTrig::OUT,
            }],
            params: &[ParamMeta {
                name: "Rate",
                index: RandomTrig::PARAM_RATE,
                min: 0.05,
                max: 40.0,
                default: 4.0,
                choices: &[],
            }],
        },
        factory: create_random_trig,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "track_hold",
            display_name: "Track & Hold",
            inputs: &[
                PortMeta {
                    name: "signal",
                    index: TrackHold::IN_SIGNAL,
                },
                PortMeta {
                    name: "gate",
                    index: TrackHold::IN_GATE,
                },
            ],
            outputs: &[PortMeta {
                name: "held",
                index: TrackHold::OUT,
            }],
            params: &[],
        },
        factory: create_track_hold,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "switch",
            display_name: "Switch",
            inputs: &[
                PortMeta {
                    name: "in1",
                    index: Switch::IN_1,
                },
                PortMeta {
                    name: "in2",
                    index: Switch::IN_2,
                },
                PortMeta {
                    name: "in3",
                    index: Switch::IN_3,
                },
                PortMeta {
                    name: "in4",
                    index: Switch::IN_4,
                },
                PortMeta {
                    name: "clock",
                    index: Switch::IN_CLOCK,
                },
                PortMeta {
                    name: "reset",
                    index: Switch::IN_RESET,
                },
            ],
            outputs: &[PortMeta {
                name: "out",
                index: Switch::OUT,
            }],
            params: &[ParamMeta {
                name: "Steps",
                index: Switch::PARAM_STEPS,
                min: 2.0,
                max: 4.0,
                default: 4.0,
                choices: &[],
            }],
        },
        factory: create_switch,
    },
    // --- Processors ---
    Entry {
        info: ModuleTypeInfo {
            kind: "saturator",
            display_name: "Saturator",
            inputs: &[
                PortMeta {
                    name: "audio",
                    index: Saturator::IN,
                },
                PortMeta {
                    name: "drive_cv",
                    index: Saturator::IN_DRIVE_CV,
                },
            ],
            outputs: &[PortMeta {
                name: "audio",
                index: Saturator::OUT,
            }],
            params: &[
                ParamMeta {
                    name: "Drive",
                    index: Saturator::PARAM_DRIVE,
                    min: 0.1,
                    max: 20.0,
                    default: 2.0,
                    choices: &[],
                },
                ParamMeta {
                    name: "Mix",
                    index: Saturator::PARAM_MIX,
                    min: 0.0,
                    max: 1.0,
                    default: 1.0,
                    choices: &[],
                },
            ],
        },
        factory: create_saturator,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "bitcrush",
            display_name: "Bitcrush",
            inputs: &[PortMeta {
                name: "audio",
                index: Bitcrush::IN,
            }],
            outputs: &[PortMeta {
                name: "audio",
                index: Bitcrush::OUT,
            }],
            params: &[
                ParamMeta {
                    name: "Bits",
                    index: Bitcrush::PARAM_BITS,
                    min: 1.0,
                    max: 16.0,
                    default: 8.0,
                    choices: &[],
                },
                ParamMeta {
                    name: "Rate",
                    index: Bitcrush::PARAM_RATE,
                    min: 0.01,
                    max: 1.0,
                    default: 0.25,
                    choices: &[],
                },
            ],
        },
        factory: create_bitcrush,
    },
    Entry {
        info: ModuleTypeInfo {
            kind: "env_follow",
            display_name: "Env Follow",
            inputs: &[PortMeta {
                name: "audio",
                index: EnvFollow::IN,
            }],
            outputs: &[PortMeta {
                name: "cv",
                index: EnvFollow::OUT,
            }],
            params: &[
                ParamMeta {
                    name: "Rise",
                    index: EnvFollow::PARAM_RISE,
                    min: 0.001,
                    max: 2.0,
                    default: 0.01,
                    choices: &[],
                },
                ParamMeta {
                    name: "Fall",
                    index: EnvFollow::PARAM_FALL,
                    min: 0.001,
                    max: 5.0,
                    default: 0.1,
                    choices: &[],
                },
            ],
        },
        factory: create_env_follow,
    },
];
