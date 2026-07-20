//! Module type registry: factories and port/param metadata for compiling a
//! [`crate::patch::Patch`] into a graph and for control surfaces.
//!
//! Not used on the audio callback path — only at build / edit / describe time.

use crate::module::Module;
use crate::modules::{
    Attenuverter, Envelope, Io, Lfo, MidiToCv, Mixer, Noise, Oscilloscope, SampleHold, Vca, Vcf,
    Vco,
};
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

fn create_oscilloscope(_sr: f32, _i: &ModuleInstance) -> Box<dyn Module> {
    Box::new(Oscilloscope::new())
}

fn create_io(_sr: f32, i: &ModuleInstance) -> Box<dyn Module> {
    let send = i.io_send_channels.unwrap_or(0);
    let capture = i.io_capture_channels.unwrap_or(2);
    Box::new(Io::new(send, capture))
}

// --- static tables -----------------------------------------------------------

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
                },
                ParamMeta {
                    name: "Resonance",
                    index: Vcf::PARAM_RESONANCE,
                    min: 0.0,
                    max: 1.0,
                    default: 0.2,
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
                },
                ParamMeta {
                    name: "Decay",
                    index: Envelope::PARAM_DECAY,
                    min: 0.001,
                    max: 2.0,
                    default: 0.1,
                },
                ParamMeta {
                    name: "Sustain",
                    index: Envelope::PARAM_SUSTAIN,
                    min: 0.0,
                    max: 1.0,
                    default: 0.7,
                },
                ParamMeta {
                    name: "Release",
                    index: Envelope::PARAM_RELEASE,
                    min: 0.01,
                    max: 5.0,
                    default: 0.3,
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
                    name: "bipolar",
                    index: Lfo::OUT_BIPOLAR,
                },
                PortMeta {
                    name: "unipolar",
                    index: Lfo::OUT_UNIPOLAR,
                },
            ],
            params: &[ParamMeta {
                name: "Rate",
                index: Lfo::PARAM_RATE,
                min: 0.1,
                max: 20.0,
                default: 2.0,
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
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
                },
                ParamMeta {
                    name: "Offset",
                    index: Attenuverter::PARAM_OFFSET,
                    min: -1.0,
                    max: 1.0,
                    default: 0.0,
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
                },
                ParamMeta {
                    name: "Level 2",
                    index: Mixer::PARAM_LEVEL_2,
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
                },
                ParamMeta {
                    name: "Level 3",
                    index: Mixer::PARAM_LEVEL_3,
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
                },
                ParamMeta {
                    name: "Level 4",
                    index: Mixer::PARAM_LEVEL_4,
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
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
            outputs: &[PortMeta {
                name: "white",
                index: Noise::OUT_WHITE,
            }],
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
            params: &[],
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
                },
                ParamMeta {
                    name: "Ch1 Offset",
                    index: Io::param_capture_offset(0),
                    min: -1.0,
                    max: 1.0,
                    default: 0.0,
                },
                ParamMeta {
                    name: "Ch2 Scale",
                    index: Io::param_capture_scale(1),
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
                },
                ParamMeta {
                    name: "Ch2 Offset",
                    index: Io::param_capture_offset(1),
                    min: -1.0,
                    max: 1.0,
                    default: 0.0,
                },
            ],
        },
        factory: create_io,
    },
];
