//! Serializable patch document and voice wrapper.
//!
//! [`Patch`] is the edit/serialize source of truth. Compiling it via
//! [`Patch::build`] produces a [`BuiltPatch`] (a ready `Graph` plus role
//! handles). The audio callback only ever touches the compiled graph — never
//! the document, registry, or JSON.

use serde::{Deserialize, Serialize};

use crate::graph::Graph;
use crate::module::BlockContext;
use crate::modules::{
    Envelope, Io, Lfo, MidiToCv, Mixer, Noise, Oscilloscope, Quantizer, Sequencer, Vca, Vcf, Vco,
};
use crate::modules::scale::SCALES;
use crate::registry;
use crate::MAX_BLOCK_FRAMES;

/// Stable endpoint id for the master hardware output (`Synth::master_io`).
pub const IO_ENDPOINT_AUDIO_OUT: &str = "audio_out";
/// Stable endpoint id for external capture into every voice's `Io`.
pub const IO_ENDPOINT_AUDIO_IN: &str = "audio_in";
/// Device name meaning "resolve the host default at open time".
pub const IO_DEVICE_DEFAULT: &str = "default";

/// Patch document format version written by [`Patch::to_json`].
pub const PATCH_FORMAT_VERSION: u32 = 1;

/// Errors from patch edit, load, or compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchError {
    UnknownKind(String),
    UnknownModule(u32),
    UnknownCable(u32),
    RoleModule(u32),
    InvalidPort { module: u32, port: usize, what: &'static str },
    DuplicateCable,
    Build(String),
    Json(String),
}

impl std::fmt::Display for PatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownKind(k) => write!(f, "unknown module kind '{k}'"),
            Self::UnknownModule(id) => write!(f, "unknown module id {id}"),
            Self::UnknownCable(id) => write!(f, "unknown cable id {id}"),
            Self::RoleModule(id) => write!(f, "module {id} is assigned a patch role and cannot be removed"),
            Self::InvalidPort { module, port, what } => {
                write!(f, "invalid {what} port {port} on module {module}")
            }
            Self::DuplicateCable => write!(f, "cable already exists"),
            Self::Build(msg) => write!(f, "build failed: {msg}"),
            Self::Json(msg) => write!(f, "json error: {msg}"),
        }
    }
}

impl std::error::Error for PatchError {}

/// Describes one parameter for a control surface.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParamInfo {
    pub name: String,
    pub param: usize,
    pub min: f32,
    pub max: f32,
    pub default_value: f32,
}

/// Describes one module instance for a control surface.
///
/// `module` is the **stable document id**, not a dense graph index.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub module: u32,
    pub name: String,
    pub kind: String,
    pub params: Vec<ParamInfo>,
}

/// Direction of a hardware-facing IO endpoint declared by a patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IoDirection {
    /// Capture from a device into the graph (`Module::write_input`).
    Input,
    /// Send graph signals out to a device (master `Io` send channels).
    Output,
}

/// Patch-owned binding for one hardware-facing `Io` endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IoEndpoint {
    /// Stable id within the patch (`audio_out`, `audio_in`).
    pub id: String,
    /// Display name for control surfaces ("Audio Out").
    pub name: String,
    /// Style/grouping key for clients; always `"io"` for these endpoints.
    pub kind: String,
    pub direction: IoDirection,
    pub channels: usize,
    /// cpal device name, or [`IO_DEVICE_DEFAULT`] / empty for host default.
    pub device: String,
    /// First physical device channel to map to endpoint channel 0.
    pub device_channel_offset: usize,
    /// Voice-graph module **stable id** when this Io lives in each voice;
    /// `None` for master-level Io owned by `Synth` (e.g. audio out).
    pub module: Option<u32>,
    /// Calibration knobs (scale/offset) exposed on a control surface.
    pub params: Vec<ParamInfo>,
    /// Lattice position for rack / other UIs (required; ignored by DSP).
    #[serde(default)]
    pub coordinates: ModuleCoordinates,
}

/// Lattice coordinates for a module on a control surface (ignored by DSP).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleCoordinates {
    pub x: u32,
    pub y: u32,
}

impl ModuleCoordinates {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// Preferred control-surface canvas size (cell counts; ignored by DSP).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchCanvas {
    pub width: u32,
    pub height: u32,
}

/// Standard Audio Out (master) + Audio In (voice Io) endpoint bindings.
fn standard_io_endpoints(
    io_in_module: u32,
    audio_in_coords: ModuleCoordinates,
    audio_out_coords: ModuleCoordinates,
) -> Vec<IoEndpoint> {
    let audio_in_params = vec![
        ParamInfo {
            name: "Ch1 Scale".into(),
            param: Io::param_capture_scale(0),
            min: 0.0,
            max: 2.0,
            default_value: 1.0,
        },
        ParamInfo {
            name: "Ch1 Offset".into(),
            param: Io::param_capture_offset(0),
            min: -1.0,
            max: 1.0,
            default_value: 0.0,
        },
        ParamInfo {
            name: "Ch2 Scale".into(),
            param: Io::param_capture_scale(1),
            min: 0.0,
            max: 2.0,
            default_value: 1.0,
        },
        ParamInfo {
            name: "Ch2 Offset".into(),
            param: Io::param_capture_offset(1),
            min: -1.0,
            max: 1.0,
            default_value: 0.0,
        },
    ];
    // Master Io: L, R, pitch CV, gate CV.
    let audio_out_params = vec![
        ParamInfo {
            name: "Audio L Scale".into(),
            param: Io::param_send_scale(0),
            min: 0.0,
            max: 2.0,
            default_value: 1.0,
        },
        ParamInfo {
            name: "Audio L Offset".into(),
            param: Io::param_send_offset(0),
            min: -1.0,
            max: 1.0,
            default_value: 0.0,
        },
        ParamInfo {
            name: "Audio R Scale".into(),
            param: Io::param_send_scale(1),
            min: 0.0,
            max: 2.0,
            default_value: 1.0,
        },
        ParamInfo {
            name: "Audio R Offset".into(),
            param: Io::param_send_offset(1),
            min: -1.0,
            max: 1.0,
            default_value: 0.0,
        },
        ParamInfo {
            name: "Pitch Scale".into(),
            param: Io::param_send_scale(2),
            min: 0.0,
            max: 2.0,
            default_value: 1.0,
        },
        ParamInfo {
            name: "Pitch Offset".into(),
            param: Io::param_send_offset(2),
            min: -1.0,
            max: 1.0,
            default_value: 0.0,
        },
        ParamInfo {
            name: "Gate Scale".into(),
            param: Io::param_send_scale(3),
            min: 0.0,
            max: 2.0,
            default_value: 1.0,
        },
        ParamInfo {
            name: "Gate Offset".into(),
            param: Io::param_send_offset(3),
            min: -1.0,
            max: 1.0,
            default_value: 0.0,
        },
    ];
    vec![
        IoEndpoint {
            id: IO_ENDPOINT_AUDIO_OUT.into(),
            name: "Audio Out".into(),
            kind: "io".into(),
            direction: IoDirection::Output,
            channels: 4,
            device: IO_DEVICE_DEFAULT.into(),
            device_channel_offset: 0,
            module: None,
            params: audio_out_params,
            coordinates: audio_out_coords,
        },
        IoEndpoint {
            id: IO_ENDPOINT_AUDIO_IN.into(),
            name: "Audio In".into(),
            kind: "io".into(),
            direction: IoDirection::Input,
            channels: 2,
            device: IO_DEVICE_DEFAULT.into(),
            device_channel_offset: 0,
            module: Some(io_in_module),
            params: audio_in_params,
            coordinates: audio_in_coords,
        },
    ]
}

/// One module placed in the patch document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleInstance {
    pub id: u32,
    pub kind: String,
    pub name: String,
    /// Sparse param values: (param index, value). Missing params use registry defaults.
    #[serde(default)]
    pub params: Vec<(usize, f32)>,
    /// For `kind == "io"`: send channel count (default 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_send_channels: Option<usize>,
    /// For `kind == "io"`: capture channel count (default 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_capture_channels: Option<usize>,
    /// Lattice position for rack / other UIs (required; ignored by DSP).
    /// Missing in old JSON deserializes as `(0,0)`.
    #[serde(default)]
    pub coordinates: ModuleCoordinates,
}

impl ModuleInstance {
    pub fn param_value(&self, param: usize, default: f32) -> f32 {
        self.params
            .iter()
            .find(|(p, _)| *p == param)
            .map(|(_, v)| *v)
            .unwrap_or(default)
    }

    pub fn set_param(&mut self, param: usize, value: f32) {
        if let Some(slot) = self.params.iter_mut().find(|(p, _)| *p == param) {
            slot.1 = value;
        } else {
            self.params.push((param, value));
        }
    }
}

/// One patch cable in the document (stable module ids + port indices).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cable {
    pub id: u32,
    pub src_module: u32,
    pub src_port: usize,
    pub dst_module: u32,
    pub dst_port: usize,
}

/// Fixed roles the voice allocator / Synth need after compile.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatchRoles {
    pub note_source: u32,
    pub activity_gate: u32,
    pub external_input: u32,
    /// Mono voice output tap (hardware L/R both get this sum).
    pub output_tap_module: u32,
    pub output_tap_port: usize,
}

/// Wire-format wrapper so JSON can version the document.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PatchFile {
    version: u32,
    #[serde(flatten)]
    patch: PatchBody,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PatchBody {
    next_id: u32,
    next_cable_id: u32,
    modules: Vec<ModuleInstance>,
    cables: Vec<Cable>,
    roles: PatchRoles,
    io_endpoints: Vec<IoEndpoint>,
    /// Preferred control-surface canvas size (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canvas: Option<PatchCanvas>,
}

/// Serializable patch document: topology, params, roles, and IO bindings.
///
/// Edit and serialize this; compile with [`Patch::build`] for audio. Steady-state
/// audio never touches this type.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "PatchFile", into = "PatchFile")]
pub struct Patch {
    pub next_id: u32,
    pub next_cable_id: u32,
    pub modules: Vec<ModuleInstance>,
    pub cables: Vec<Cable>,
    pub roles: PatchRoles,
    pub io_endpoints: Vec<IoEndpoint>,
    /// Preferred control-surface canvas size (optional).
    pub canvas: Option<PatchCanvas>,
}

impl From<PatchFile> for Patch {
    fn from(f: PatchFile) -> Self {
        Self {
            next_id: f.patch.next_id,
            next_cable_id: f.patch.next_cable_id,
            modules: f.patch.modules,
            cables: f.patch.cables,
            roles: f.patch.roles,
            io_endpoints: f.patch.io_endpoints,
            canvas: f.patch.canvas,
        }
    }
}

impl From<Patch> for PatchFile {
    fn from(p: Patch) -> Self {
        Self {
            version: PATCH_FORMAT_VERSION,
            patch: PatchBody {
                next_id: p.next_id,
                next_cable_id: p.next_cable_id,
                modules: p.modules,
                cables: p.cables,
                roles: p.roles,
                io_endpoints: p.io_endpoints,
                canvas: p.canvas,
            },
        }
    }
}

/// Compiled voice graph plus handles. Produced by [`Patch::build`].
pub struct BuiltPatch {
    pub graph: Graph,
    /// Dense graph index for note_on/note_off (MidiToCv-shaped).
    pub note_source: usize,
    /// Dense graph index for voice steal protection.
    pub activity_gate: usize,
    /// Dense graph index for external capture Io.
    pub external_input: usize,
    /// Tap index for this voice's mono audio.
    pub output_tap: usize,
    /// Stable id → dense graph index (for set_param routing).
    pub id_to_index: Vec<(u32, usize)>,
    /// Control-surface modules (stable ids).
    pub modules: Vec<ModuleInfo>,
    /// Hardware IO endpoints (stable module ids in `module` field).
    pub io_endpoints: Vec<IoEndpoint>,
}

impl BuiltPatch {
    pub fn graph_index(&self, module_id: u32) -> Option<usize> {
        self.id_to_index
            .iter()
            .find(|(id, _)| *id == module_id)
            .map(|(_, idx)| *idx)
    }
}

impl Default for Patch {
    /// Empty starter — see [`Patch::empty`].
    fn default() -> Self {
        Self::empty()
    }
}

impl Patch {
    /// Minimal document: voice **Audio In** Io + master **Audio Out**, plus the
    /// one non-IO role the engine still needs for MIDI (`midi_to_cv`).
    ///
    /// - `external_input` / output tap → the Io (capture ch0 is the voice bus;
    ///   silence until something is captured or you patch into the graph).
    /// - `note_source` / `activity_gate` → same MidiToCv (gate drives `is_active`).
    ///
    /// No cables. Lattice shows In + Out endpoints and the MIDI module.
    pub fn empty() -> Self {
        let mut next_id = 0u32;
        let mut alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };
        let at = ModuleCoordinates::new;

        let midi_to_cv = alloc();
        let io_in = alloc();

        let modules = vec![
            ModuleInstance {
                id: midi_to_cv,
                kind: "midi_to_cv".into(),
                name: "MIDI to CV".into(),
                params: vec![],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(0, 0),
            },
            ModuleInstance {
                id: io_in,
                kind: "io".into(),
                name: "Audio In".into(),
                params: vec![
                    (Io::param_capture_scale(0), 1.0),
                    (Io::param_capture_offset(0), 0.0),
                    (Io::param_capture_scale(1), 1.0),
                    (Io::param_capture_offset(1), 0.0),
                ],
                io_send_channels: Some(0),
                io_capture_channels: Some(2),
                coordinates: at(0, 1),
            },
        ];

        Self {
            next_id,
            next_cable_id: 0,
            modules,
            cables: vec![],
            roles: PatchRoles {
                note_source: midi_to_cv,
                activity_gate: midi_to_cv,
                external_input: io_in,
                // Tap capture ch0: master audio is whatever is written into Io
                // (silence by default). Patch audio into the graph when building.
                output_tap_module: io_in,
                output_tap_port: 0,
            },
            io_endpoints: standard_io_endpoints(io_in, at(0, 1), at(1, 1)),
            canvas: Some(PatchCanvas {
                width: 8,
                height: 6,
            }),
        }
    }

    /// Example groove: sequenced bass + kick + snare + MIDI lead.
    /// Shipped as `patches/groove.json` in the repo.
    ///
    /// Lattice:
    /// ```text
    ///   Clock  BassSeq Quant BassVCO BassVCF BassEnv BassVCA
    ///   SnClk  KickSeq      KickVCO        KickEnv KickVCA
    ///          SnareSeq Noise              SnEnv   SnVCA
    ///   MIDI   LeadVCO LeadVCF LeadEnvA LeadEnvF LeadVCA  Mix Scope
    ///   In     Out
    /// ```
    pub fn example_groove() -> Self {
        let mut next_id = 0u32;
        let mut alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let at = ModuleCoordinates::new;
        let semi = |s: f32| s / 12.0;
        let minor_pent = SCALES
            .iter()
            .position(|s| s.name == "Minor Pentatonic")
            .unwrap_or(11) as f32;

        // --- roles / shared ---
        let midi_to_cv = alloc();
        let clock = alloc();
        let snare_clock = alloc();
        let mixer = alloc();
        let scope = alloc();
        let io_in = alloc();

        // --- bassline ---
        let bass_seq = alloc();
        let bass_quant = alloc();
        let bass_vco = alloc();
        let bass_vcf = alloc();
        let bass_env = alloc();
        let bass_vca = alloc();

        // --- kick ---
        let kick_seq = alloc();
        let kick_vco = alloc();
        let kick_env = alloc();
        let kick_vca = alloc();

        // --- snare ---
        let snare_seq = alloc();
        let snare_noise = alloc();
        let snare_env = alloc();
        let snare_vca = alloc();

        // --- lead (MIDI) ---
        let lead_vco = alloc();
        let lead_vcf = alloc();
        let lead_env_amp = alloc();
        let lead_env_filt = alloc();
        let lead_vca = alloc();

        let modules = vec![
            // Shared control / clock
            ModuleInstance {
                id: midi_to_cv,
                kind: "midi_to_cv".into(),
                name: "MIDI to CV".into(),
                params: vec![],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(0, 3),
            },
            ModuleInstance {
                id: clock,
                kind: "lfo".into(),
                name: "Clock".into(),
                // ~4 Hz → 16th-ish feel for bass/kick sequencers.
                params: vec![(Lfo::PARAM_RATE, 4.0)],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(0, 0),
            },
            ModuleInstance {
                id: snare_clock,
                kind: "lfo".into(),
                name: "Snare Clock".into(),
                // Half rate → backbeat-ish snare against the master clock.
                params: vec![(Lfo::PARAM_RATE, 2.0)],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(0, 1),
            },
            // Bass
            ModuleInstance {
                id: bass_seq,
                kind: "sequencer".into(),
                name: "Bass Seq".into(),
                params: vec![
                    // 16 steps ≈ 4 bars of 4: A A (same) then B C (progression).
                    (Sequencer::PARAM_STEPS, 16.0),
                    (Sequencer::PARAM_GATE_LEN, 0.4),
                    // Bar 1 — home groove (minor pent, octave below).
                    (Sequencer::param_step(0), semi(-12.0)), // root
                    (Sequencer::param_step(1), semi(-12.0)),
                    (Sequencer::param_step(2), semi(-10.0)), // b3
                    (Sequencer::param_step(3), semi(-7.0)),  // 5
                    // Bar 2 — same as bar 1.
                    (Sequencer::param_step(4), semi(-12.0)),
                    (Sequencer::param_step(5), semi(-12.0)),
                    (Sequencer::param_step(6), semi(-10.0)),
                    (Sequencer::param_step(7), semi(-7.0)),
                    // Bar 3 — lift / climb.
                    (Sequencer::param_step(8), semi(-10.0)),
                    (Sequencer::param_step(9), semi(-8.0)),  // 4
                    (Sequencer::param_step(10), semi(-7.0)),
                    (Sequencer::param_step(11), semi(-5.0)), // b7
                    // Bar 4 — peak then drop back home.
                    (Sequencer::param_step(12), semi(-3.0)), // 9 / 2 up
                    (Sequencer::param_step(13), semi(-5.0)),
                    (Sequencer::param_step(14), semi(-7.0)),
                    (Sequencer::param_step(15), semi(-12.0)), // root
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(1, 0),
            },
            ModuleInstance {
                id: bass_quant,
                kind: "quantizer".into(),
                name: "Bass Quant".into(),
                params: vec![
                    (Quantizer::PARAM_SCALE, minor_pent),
                    (Quantizer::PARAM_ROOT, 0.0),
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(2, 0),
            },
            ModuleInstance {
                id: bass_vco,
                kind: "vco".into(),
                name: "Bass VCO".into(),
                params: vec![(Vco::PARAM_TUNE, -1.0)],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(3, 0),
            },
            ModuleInstance {
                id: bass_vcf,
                kind: "vcf".into(),
                name: "Bass VCF".into(),
                params: vec![
                    (Vcf::PARAM_CUTOFF, 480.0),
                    (Vcf::PARAM_RESONANCE, 0.35),
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(4, 0),
            },
            ModuleInstance {
                id: bass_env,
                kind: "envelope".into(),
                name: "Bass Env".into(),
                params: vec![
                    (Envelope::PARAM_ATTACK, 0.002),
                    (Envelope::PARAM_DECAY, 0.18),
                    (Envelope::PARAM_SUSTAIN, 0.15),
                    (Envelope::PARAM_RELEASE, 0.08),
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(5, 0),
            },
            ModuleInstance {
                id: bass_vca,
                kind: "vca".into(),
                name: "Bass VCA".into(),
                params: vec![(Vca::PARAM_LEVEL, 1.0)],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(6, 0),
            },
            // Kick — 4-on-the-floor via short gate + low sine.
            ModuleInstance {
                id: kick_seq,
                kind: "sequencer".into(),
                name: "Kick Seq".into(),
                params: vec![
                    (Sequencer::PARAM_STEPS, 4.0),
                    (Sequencer::PARAM_GATE_LEN, 0.12),
                    (Sequencer::param_step(0), 0.0),
                    (Sequencer::param_step(1), 0.0),
                    (Sequencer::param_step(2), 0.0),
                    (Sequencer::param_step(3), 0.0),
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(1, 1),
            },
            ModuleInstance {
                id: kick_vco,
                kind: "vco".into(),
                name: "Kick VCO".into(),
                // ~55 Hz ballpark (A1-ish relative to 440).
                params: vec![(Vco::PARAM_TUNE, -3.0)],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(3, 1),
            },
            ModuleInstance {
                id: kick_env,
                kind: "envelope".into(),
                name: "Kick Env".into(),
                params: vec![
                    (Envelope::PARAM_ATTACK, 0.001),
                    (Envelope::PARAM_DECAY, 0.22),
                    (Envelope::PARAM_SUSTAIN, 0.0),
                    (Envelope::PARAM_RELEASE, 0.08),
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(5, 1),
            },
            ModuleInstance {
                id: kick_vca,
                kind: "vca".into(),
                name: "Kick VCA".into(),
                params: vec![(Vca::PARAM_LEVEL, 1.1)],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(6, 1),
            },
            // Snare — noise burst on half-rate clock.
            ModuleInstance {
                id: snare_seq,
                kind: "sequencer".into(),
                name: "Snare Seq".into(),
                params: vec![
                    (Sequencer::PARAM_STEPS, 2.0),
                    (Sequencer::PARAM_GATE_LEN, 0.1),
                    (Sequencer::param_step(0), 0.0),
                    (Sequencer::param_step(1), 0.0),
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(1, 2),
            },
            ModuleInstance {
                id: snare_noise,
                kind: "noise".into(),
                name: "Snare Noise".into(),
                params: vec![],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(3, 2),
            },
            ModuleInstance {
                id: snare_env,
                kind: "envelope".into(),
                name: "Snare Env".into(),
                params: vec![
                    (Envelope::PARAM_ATTACK, 0.001),
                    (Envelope::PARAM_DECAY, 0.12),
                    (Envelope::PARAM_SUSTAIN, 0.0),
                    (Envelope::PARAM_RELEASE, 0.06),
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(5, 2),
            },
            ModuleInstance {
                id: snare_vca,
                kind: "vca".into(),
                name: "Snare VCA".into(),
                params: vec![(Vca::PARAM_LEVEL, 0.85)],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(6, 2),
            },
            // Lead — keyboard / MIDI.
            ModuleInstance {
                id: lead_vco,
                kind: "vco".into(),
                name: "Lead VCO".into(),
                params: vec![(Vco::PARAM_TUNE, 0.0)],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(1, 3),
            },
            ModuleInstance {
                id: lead_vcf,
                kind: "vcf".into(),
                name: "Lead VCF".into(),
                params: vec![
                    (Vcf::PARAM_CUTOFF, 1800.0),
                    (Vcf::PARAM_RESONANCE, 0.22),
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(2, 3),
            },
            ModuleInstance {
                id: lead_env_amp,
                kind: "envelope".into(),
                name: "Lead Amp".into(),
                params: vec![
                    (Envelope::PARAM_ATTACK, 0.01),
                    (Envelope::PARAM_DECAY, 0.15),
                    (Envelope::PARAM_SUSTAIN, 0.65),
                    (Envelope::PARAM_RELEASE, 0.25),
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(3, 3),
            },
            ModuleInstance {
                id: lead_env_filt,
                kind: "envelope".into(),
                name: "Lead Filt".into(),
                params: vec![
                    (Envelope::PARAM_ATTACK, 0.02),
                    (Envelope::PARAM_DECAY, 0.25),
                    (Envelope::PARAM_SUSTAIN, 0.3),
                    (Envelope::PARAM_RELEASE, 0.3),
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(4, 3),
            },
            ModuleInstance {
                id: lead_vca,
                kind: "vca".into(),
                name: "Lead VCA".into(),
                params: vec![(Vca::PARAM_LEVEL, 1.0)],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(5, 3),
            },
            // Mono mix bus (engine duals this to hardware L+R).
            ModuleInstance {
                id: mixer,
                kind: "mixer".into(),
                name: "Mix".into(),
                // Free paths modest so after 4-voice average they sit near lead/4 * hot.
                // Lead hot so held notes cut through the groove.
                params: vec![
                    (Mixer::PARAM_LEVEL_1, 0.45), // bass
                    (Mixer::PARAM_LEVEL_2, 0.55), // kick
                    (Mixer::PARAM_LEVEL_3, 0.4),  // snare
                    (Mixer::PARAM_LEVEL_4, 2.0),  // lead
                ],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(6, 3),
            },
            ModuleInstance {
                id: scope,
                kind: "oscilloscope".into(),
                name: "Scope".into(),
                params: vec![],
                io_send_channels: None,
                io_capture_channels: None,
                coordinates: at(7, 3),
            },
            ModuleInstance {
                id: io_in,
                kind: "io".into(),
                name: "Audio In".into(),
                params: vec![
                    (Io::param_capture_scale(0), 1.0),
                    (Io::param_capture_offset(0), 0.0),
                    (Io::param_capture_scale(1), 1.0),
                    (Io::param_capture_offset(1), 0.0),
                ],
                io_send_channels: Some(0),
                io_capture_channels: Some(2),
                coordinates: at(0, 4),
            },
        ];

        let mut next_cable_id = 0u32;
        let mut cable =
            |src_module: u32, src_port: usize, dst_module: u32, dst_port: usize| -> Cable {
                let id = next_cable_id;
                next_cable_id += 1;
                Cable {
                    id,
                    src_module,
                    src_port,
                    dst_module,
                    dst_port,
                }
            };

        let cables = vec![
            // --- Bass ---
            cable(clock, Lfo::OUT_BIPOLAR, bass_seq, Sequencer::IN_CLOCK),
            cable(bass_seq, Sequencer::OUT_CV, bass_quant, Quantizer::IN_CV),
            cable(bass_quant, Quantizer::OUT_CV, bass_vco, Vco::IN_PITCH),
            cable(bass_vco, Vco::OUT_SAW, bass_vcf, Vcf::IN_AUDIO),
            cable(bass_env, Envelope::OUT_CV, bass_vcf, Vcf::IN_CUTOFF_CV),
            cable(bass_vcf, Vcf::OUT_LOWPASS, bass_vca, Vca::IN_AUDIO),
            cable(bass_env, Envelope::OUT_CV, bass_vca, Vca::IN_CV),
            cable(bass_seq, Sequencer::OUT_GATE, bass_env, Envelope::IN_GATE),
            // --- Kick ---
            cable(clock, Lfo::OUT_BIPOLAR, kick_seq, Sequencer::IN_CLOCK),
            cable(kick_vco, Vco::OUT_SINE, kick_vca, Vca::IN_AUDIO),
            // Envelope into pitch for a little downward thump.
            cable(kick_env, Envelope::OUT_CV, kick_vco, Vco::IN_PITCH),
            cable(kick_env, Envelope::OUT_CV, kick_vca, Vca::IN_CV),
            cable(kick_seq, Sequencer::OUT_GATE, kick_env, Envelope::IN_GATE),
            // --- Snare ---
            cable(snare_clock, Lfo::OUT_BIPOLAR, snare_seq, Sequencer::IN_CLOCK),
            cable(snare_noise, Noise::OUT_WHITE, snare_vca, Vca::IN_AUDIO),
            cable(snare_env, Envelope::OUT_CV, snare_vca, Vca::IN_CV),
            cable(snare_seq, Sequencer::OUT_GATE, snare_env, Envelope::IN_GATE),
            // --- Lead (MIDI / keyboard) ---
            cable(midi_to_cv, MidiToCv::OUT_PITCH, lead_vco, Vco::IN_PITCH),
            cable(lead_vco, Vco::OUT_SQUARE, lead_vcf, Vcf::IN_AUDIO),
            cable(lead_env_filt, Envelope::OUT_CV, lead_vcf, Vcf::IN_CUTOFF_CV),
            cable(lead_vcf, Vcf::OUT_LOWPASS, lead_vca, Vca::IN_AUDIO),
            cable(lead_env_amp, Envelope::OUT_CV, lead_vca, Vca::IN_CV),
            cable(midi_to_cv, MidiToCv::OUT_GATE, lead_env_amp, Envelope::IN_GATE),
            cable(midi_to_cv, MidiToCv::OUT_GATE, lead_env_filt, Envelope::IN_GATE),
            // --- Mono mix bus ---
            cable(bass_vca, Vca::OUT_AUDIO, mixer, Mixer::IN_1),
            cable(kick_vca, Vca::OUT_AUDIO, mixer, Mixer::IN_2),
            cable(snare_vca, Vca::OUT_AUDIO, mixer, Mixer::IN_3),
            cable(lead_vca, Vca::OUT_AUDIO, mixer, Mixer::IN_4),
            cable(mixer, Mixer::OUT_MIX, scope, Oscilloscope::IN_AUDIO),
        ];

        Self {
            next_id,
            next_cable_id,
            modules,
            cables,
            roles: PatchRoles {
                note_source: midi_to_cv,
                // Lead amp envelope drives voice activity / steal protection.
                activity_gate: lead_env_amp,
                external_input: io_in,
                output_tap_module: scope,
                output_tap_port: Oscilloscope::OUT_AUDIO,
            },
            io_endpoints: standard_io_endpoints(io_in, at(0, 4), at(1, 4)),
            canvas: Some(PatchCanvas {
                width: 16,
                height: 10,
            }),
        }
    }

    /// Adds a module of `kind`, returning its stable id.
    pub fn add_module(&mut self, kind: &str) -> Result<u32, PatchError> {
        let info = registry::type_info(kind)
            .ok_or_else(|| PatchError::UnknownKind(kind.to_string()))?;
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);

        let params: Vec<(usize, f32)> = info
            .params
            .iter()
            .map(|p| (p.index, p.default))
            .collect();

        let (io_send, io_capture) = if kind == "io" {
            (Some(0), Some(2))
        } else {
            (None, None)
        };

        let coordinates = self.next_free_coordinates();
        self.modules.push(ModuleInstance {
            id,
            kind: kind.to_string(),
            name: info.display_name.to_string(),
            params,
            io_send_channels: io_send,
            io_capture_channels: io_capture,
            coordinates,
        });
        Ok(id)
    }

    /// First lattice cell not occupied by a module or IO endpoint.
    fn next_free_coordinates(&self) -> ModuleCoordinates {
        let mut occupied: Vec<(u32, u32)> = self
            .modules
            .iter()
            .map(|m| (m.coordinates.x, m.coordinates.y))
            .chain(
                self.io_endpoints
                    .iter()
                    .map(|e| (e.coordinates.x, e.coordinates.y)),
            )
            .collect();
        occupied.sort_unstable();
        occupied.dedup();
        // Row-major scan; keep growth bounded for large patches.
        for y in 0..64u32 {
            for x in 0..64u32 {
                if occupied.binary_search(&(x, y)).is_err() {
                    return ModuleCoordinates::new(x, y);
                }
            }
        }
        ModuleCoordinates::new(0, 64)
    }

    /// Point the main voice audio bus at `module`'s output `port`.
    /// That signal is what every voice mixes into master Audio Out L/R.
    pub fn set_output_tap(&mut self, module: u32, port: usize) -> Result<(), PatchError> {
        self.validate_port(module, port, true)?;
        self.roles.output_tap_module = module;
        self.roles.output_tap_port = port;
        Ok(())
    }

    /// Set lattice coordinates for a module (control surface only).
    pub fn set_module_coordinates(
        &mut self,
        id: u32,
        x: u32,
        y: u32,
    ) -> Result<(), PatchError> {
        let m = self
            .modules
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or(PatchError::UnknownModule(id))?;
        m.coordinates = ModuleCoordinates { x, y };
        Ok(())
    }

    /// Batch-update module coordinates and optional canvas size.
    pub fn set_coordinates(
        &mut self,
        positions: &[(u32, u32, u32)],
        canvas: Option<PatchCanvas>,
    ) -> Result<(), PatchError> {
        for &(id, x, y) in positions {
            self.set_module_coordinates(id, x, y)?;
        }
        if let Some(c) = canvas {
            self.canvas = Some(PatchCanvas {
                width: c.width.max(1),
                height: c.height.max(1),
            });
        }
        Ok(())
    }

    /// Removes a module and all incident cables. Refuses role modules.
    pub fn remove_module(&mut self, id: u32) -> Result<(), PatchError> {
        if self.is_role_module(id) {
            return Err(PatchError::RoleModule(id));
        }
        if !self.modules.iter().any(|m| m.id == id) {
            return Err(PatchError::UnknownModule(id));
        }
        self.modules.retain(|m| m.id != id);
        self.cables
            .retain(|c| c.src_module != id && c.dst_module != id);
        for ep in &mut self.io_endpoints {
            if ep.module == Some(id) {
                ep.module = None;
            }
        }
        Ok(())
    }

    pub fn connect(
        &mut self,
        src_module: u32,
        src_port: usize,
        dst_module: u32,
        dst_port: usize,
    ) -> Result<u32, PatchError> {
        self.validate_port(src_module, src_port, true)?;
        self.validate_port(dst_module, dst_port, false)?;
        if self.cables.iter().any(|c| {
            c.src_module == src_module
                && c.src_port == src_port
                && c.dst_module == dst_module
                && c.dst_port == dst_port
        }) {
            return Err(PatchError::DuplicateCable);
        }
        let id = self.next_cable_id;
        self.next_cable_id = self.next_cable_id.saturating_add(1);
        self.cables.push(Cable {
            id,
            src_module,
            src_port,
            dst_module,
            dst_port,
        });
        Ok(id)
    }

    pub fn disconnect(
        &mut self,
        src_module: u32,
        src_port: usize,
        dst_module: u32,
        dst_port: usize,
    ) -> Result<(), PatchError> {
        let before = self.cables.len();
        self.cables.retain(|c| {
            !(c.src_module == src_module
                && c.src_port == src_port
                && c.dst_module == dst_module
                && c.dst_port == dst_port)
        });
        if self.cables.len() == before {
            return Err(PatchError::Build("cable not found".into()));
        }
        Ok(())
    }

    pub fn disconnect_by_id(&mut self, cable_id: u32) -> Result<(), PatchError> {
        let before = self.cables.len();
        self.cables.retain(|c| c.id != cable_id);
        if self.cables.len() == before {
            return Err(PatchError::UnknownCable(cable_id));
        }
        Ok(())
    }

    /// Updates a param value in the document (does not touch live graphs).
    pub fn set_param(&mut self, module_id: u32, param: usize, value: f32) -> Result<(), PatchError> {
        let m = self
            .modules
            .iter_mut()
            .find(|m| m.id == module_id)
            .ok_or(PatchError::UnknownModule(module_id))?;
        m.set_param(param, value);
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, PatchError> {
        serde_json::to_string_pretty(self).map_err(|e| PatchError::Json(e.to_string()))
    }

    pub fn from_json(s: &str) -> Result<Self, PatchError> {
        serde_json::from_str(s).map_err(|e| PatchError::Json(e.to_string()))
    }

    /// Compile into a runnable graph. Allocates — not for the audio callback.
    pub fn build(&self, sample_rate: f32) -> Result<BuiltPatch, PatchError> {
        let mut graph = Graph::new();
        let mut id_to_index: Vec<(u32, usize)> = Vec::with_capacity(self.modules.len());
        let mut modules_info = Vec::new();

        for instance in &self.modules {
            let module = registry::create(sample_rate, instance)
                .ok_or_else(|| PatchError::UnknownKind(instance.kind.clone()))?;
            let index = graph.add_module(module);
            id_to_index.push((instance.id, index));

            // Apply document param values (envelope also set at construct).
            for &(param, value) in &instance.params {
                graph.set_param(index, param, value);
            }

            if let Some(type_info) = registry::type_info(&instance.kind) {
                // Expose modules that have knobs, or IO (calibration).
                if !type_info.params.is_empty() || instance.kind == "io" {
                    let params: Vec<ParamInfo> = type_info
                        .params
                        .iter()
                        .map(|p| {
                            let default_value = instance.param_value(p.index, p.default);
                            ParamInfo {
                                name: p.name.to_string(),
                                param: p.index,
                                min: p.min,
                                max: p.max,
                                default_value,
                            }
                        })
                        .collect();
                    // Prefer instance-stored param values as "default" for UI current.
                    let params = params
                        .into_iter()
                        .map(|mut p| {
                            p.default_value = instance.param_value(p.param, p.default_value);
                            p
                        })
                        .collect();
                    modules_info.push(ModuleInfo {
                        module: instance.id,
                        name: instance.name.clone(),
                        kind: instance.kind.clone(),
                        params,
                    });
                }
            }
        }

        let resolve = |id: u32| -> Result<usize, PatchError> {
            id_to_index
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, idx)| *idx)
                .ok_or(PatchError::UnknownModule(id))
        };

        for cable in &self.cables {
            let src = resolve(cable.src_module)?;
            let dst = resolve(cable.dst_module)?;
            graph.connect(src, cable.src_port, dst, cable.dst_port);
        }

        let note_source = resolve(self.roles.note_source)?;
        let activity_gate = resolve(self.roles.activity_gate)?;
        let external_input = resolve(self.roles.external_input)?;
        let tap_mod = resolve(self.roles.output_tap_module)?;
        let output_tap = graph.add_tap(tap_mod, self.roles.output_tap_port);

        graph.build(MAX_BLOCK_FRAMES);

        // IO endpoint module fields stay as stable ids; clone endpoints.
        let io_endpoints = self.io_endpoints.clone();

        Ok(BuiltPatch {
            graph,
            note_source,
            activity_gate,
            external_input,
            output_tap,
            id_to_index,
            modules: modules_info,
            io_endpoints,
        })
    }

    fn is_role_module(&self, id: u32) -> bool {
        id == self.roles.note_source
            || id == self.roles.activity_gate
            || id == self.roles.external_input
            || id == self.roles.output_tap_module
    }

    fn validate_port(&self, module_id: u32, port: usize, is_output: bool) -> Result<(), PatchError> {
        let instance = self
            .modules
            .iter()
            .find(|m| m.id == module_id)
            .ok_or(PatchError::UnknownModule(module_id))?;
        let info = registry::type_info(&instance.kind)
            .ok_or_else(|| PatchError::UnknownKind(instance.kind.clone()))?;
        let ports = if is_output { info.outputs } else { info.inputs };
        // IO ports may exceed static metadata; allow any port index for io.
        if instance.kind == "io" {
            return Ok(());
        }
        if !ports.iter().any(|p| p.index == port) {
            return Err(PatchError::InvalidPort {
                module: module_id,
                port,
                what: if is_output { "output" } else { "input" },
            });
        }
        Ok(())
    }

    /// Control-surface description from the document (no graph compile).
    pub fn describe_modules(&self) -> Vec<ModuleInfo> {
        let mut out = Vec::new();
        for instance in &self.modules {
            let Some(type_info) = registry::type_info(&instance.kind) else {
                continue;
            };
            if type_info.params.is_empty() && instance.kind != "io" {
                continue;
            }
            let params = type_info
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name.to_string(),
                    param: p.index,
                    min: p.min,
                    max: p.max,
                    default_value: instance.param_value(p.index, p.default),
                })
                .collect();
            out.push(ModuleInfo {
                module: instance.id,
                name: instance.name.clone(),
                kind: instance.kind.clone(),
                params,
            });
        }
        out
    }
}

/// A voice is one compiled patch graph plus the handles needed to drive it.
pub struct Voice {
    graph: Graph,
    note_source: usize,
    activity_gate: usize,
    external_input: usize,
    tap: usize,
    /// Stable id → dense graph index for set_param.
    id_to_index: Vec<(u32, usize)>,
    sample_rate: f32,
}

impl Voice {
    /// Builds a voice from a compiled patch.
    pub fn from_built(built: BuiltPatch, sample_rate: f32) -> Self {
        Self {
            graph: built.graph,
            note_source: built.note_source,
            activity_gate: built.activity_gate,
            external_input: built.external_input,
            tap: built.output_tap,
            id_to_index: built.id_to_index,
            sample_rate,
        }
    }

    /// Builds a voice by compiling `patch`. Returns the voice and metadata
    /// (module list, IO endpoints, id map) without a second full compile.
    pub fn new(patch: &Patch, sample_rate: f32) -> Result<(Self, BuiltPatch), PatchError> {
        let built = patch.build(sample_rate)?;
        let meta = BuiltPatch {
            graph: Graph::new(),
            note_source: built.note_source,
            activity_gate: built.activity_gate,
            external_input: built.external_input,
            output_tap: built.output_tap,
            id_to_index: built.id_to_index.clone(),
            modules: built.modules.clone(),
            io_endpoints: built.io_endpoints.clone(),
        };
        Ok((Self::from_built(built, sample_rate), meta))
    }

    pub fn note_on(&mut self, freq_hz: f32, velocity: f32) {
        let pitch_volts = (freq_hz / 440.0).log2();
        self.graph
            .set_param(self.note_source, MidiToCv::PARAM_PITCH, pitch_volts);
        self.graph
            .set_param(self.note_source, MidiToCv::PARAM_VELOCITY, velocity);
        self.graph
            .set_param(self.note_source, MidiToCv::PARAM_GATE, 1.0);
    }

    pub fn note_off(&mut self) {
        self.graph
            .set_param(self.note_source, MidiToCv::PARAM_GATE, 0.0);
    }

    pub fn is_active(&self) -> bool {
        self.graph.is_module_active(self.activity_gate)
    }

    pub fn external_input_module(&self) -> usize {
        self.external_input
    }

    pub fn write_input(&mut self, port: usize, samples: &[f32]) {
        self.graph.write_input(self.external_input, port, samples);
    }

    pub fn process_block(&mut self, out: &mut [f32]) {
        let ctx = BlockContext {
            sample_rate: self.sample_rate,
            frames: out.len(),
        };
        self.graph.process_block(&ctx);
        out.copy_from_slice(self.graph.tap(self.tap));
    }

    /// Sets a param by dense graph index (audio hot path).
    pub fn set_param_index(&mut self, module: usize, param: usize, value: f32) {
        self.graph.set_param(module, param, value);
    }

    /// Sets a param by stable document id.
    pub fn set_param_id(&mut self, module_id: u32, param: usize, value: f32) -> bool {
        if let Some((_, idx)) = self.id_to_index.iter().find(|(id, _)| *id == module_id) {
            self.graph.set_param(*idx, param, value);
            true
        } else {
            false
        }
    }

    pub fn graph_index(&self, module_id: u32) -> Option<usize> {
        self.id_to_index
            .iter()
            .find(|(id, _)| *id == module_id)
            .map(|(_, idx)| *idx)
    }

    /// Live samples on a module output after `process_block` (empty if unknown).
    pub fn module_output(&self, module_id: u32, port: usize) -> &[f32] {
        match self.graph_index(module_id) {
            Some(idx) => self.graph.output_samples(idx, port),
            None => &[],
        }
    }

    pub fn module_output_count(&self, module_id: u32) -> usize {
        match self.graph_index(module_id) {
            Some(idx) => self.graph.module_output_count(idx),
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_default_builds_and_is_silent() {
        let patch = Patch::default();
        // Only MIDI→CV (engine role) + Audio In Io; Audio Out is master-side.
        assert_eq!(patch.modules.len(), 2);
        assert!(patch.cables.is_empty());
        let built = patch.build(48_000.0).expect("build");
        let mut voice = Voice::from_built(built, 48_000.0);
        let mut out = [0.0f32; 256];
        voice.process_block(&mut out);
        assert!(out.iter().all(|&s| s.is_finite()));
        // No capture → silence.
        assert!(out.iter().all(|&s| s.abs() < 1e-6));
    }

    #[test]
    fn example_groove_makes_sound() {
        let patch = Patch::example_groove();
        let built = patch.build(48_000.0).expect("build");
        let mut voice = Voice::from_built(built, 48_000.0);
        voice.note_on(440.0, 0.8);
        let mut out = [0.0f32; 256];
        let mut energy = 0.0f64;
        for _ in 0..20 {
            voice.process_block(&mut out);
            for &s in &out {
                assert!(s.is_finite());
                energy += f64::from(s * s);
            }
        }
        assert!(energy > 0.0, "expected audio from groove example");
    }

    #[test]
    fn empty_and_groove_have_unique_coordinates() {
        for patch in [Patch::empty(), Patch::example_groove()] {
            let mut cells: Vec<(u32, u32)> = patch
                .modules
                .iter()
                .map(|m| (m.coordinates.x, m.coordinates.y))
                .collect();
            cells.sort_unstable();
            let before = cells.len();
            cells.dedup();
            assert_eq!(cells.len(), before, "modules share a lattice cell");
            for ep in &patch.io_endpoints {
                assert!(ep.coordinates.x < 32 && ep.coordinates.y < 32);
            }
        }
    }

    #[test]
    fn json_round_trip_preserves_topology() {
        let patch = Patch::default();
        let json = patch.to_json().unwrap();
        let loaded = Patch::from_json(&json).unwrap();
        assert_eq!(loaded.modules.len(), patch.modules.len());
        assert_eq!(loaded.cables.len(), patch.cables.len());
        assert_eq!(loaded.roles.note_source, patch.roles.note_source);
        assert_eq!(loaded.next_id, patch.next_id);
    }

    #[test]
    fn json_round_trip_preserves_coordinates() {
        let mut patch = Patch::default();
        let id = patch.modules[1].id;
        patch
            .set_module_coordinates(id, 3, 5)
            .expect("set coords");
        patch.canvas = Some(PatchCanvas {
            width: 32,
            height: 20,
        });
        let json = patch.to_json().unwrap();
        assert!(json.contains("\"coordinates\""));
        assert!(json.contains("\"x\": 3"));
        let loaded = Patch::from_json(&json).unwrap();
        let m = loaded.modules.iter().find(|m| m.id == id).unwrap();
        assert_eq!(m.coordinates, ModuleCoordinates { x: 3, y: 5 });
        assert_eq!(
            loaded.canvas,
            Some(PatchCanvas {
                width: 32,
                height: 20
            })
        );
    }

    #[test]
    fn add_connect_remove_module() {
        use crate::modules::Mixer;

        let mut patch = Patch::default();
        let vco = patch.add_module("vco").unwrap();
        let mix = patch.add_module("mixer").unwrap();
        patch.connect(vco, Vco::OUT_SAW, mix, Mixer::IN_1).unwrap();
        assert!(patch
            .cables
            .iter()
            .any(|c| c.src_module == vco && c.dst_module == mix));
        patch.remove_module(mix).unwrap();
        assert!(!patch.modules.iter().any(|m| m.id == mix));
        assert!(!patch.cables.iter().any(|c| c.dst_module == mix));
    }

    #[test]
    fn cannot_remove_role_module() {
        let mut patch = Patch::default();
        let role = patch.roles.note_source;
        assert_eq!(patch.remove_module(role), Err(PatchError::RoleModule(role)));
    }

    #[test]
    fn set_output_tap_points_main_bus() {
        let mut patch = Patch::default();
        let vco = patch.add_module("vco").unwrap();
        patch
            .set_output_tap(vco, Vco::OUT_SAW)
            .expect("set tap");
        assert_eq!(patch.roles.output_tap_module, vco);
        assert_eq!(patch.roles.output_tap_port, Vco::OUT_SAW);
        patch.build(48_000.0).expect("build after retap");
    }

    #[test]
    fn disconnect_removes_cable() {
        let mut patch = Patch::default();
        let midi = patch.roles.note_source;
        let io = patch.roles.external_input;
        // Any valid cable is fine — empty starts with none.
        let id = patch
            .connect(midi, MidiToCv::OUT_GATE, io, 0)
            .unwrap();
        assert_eq!(patch.cables.len(), 1);
        patch
            .disconnect(midi, MidiToCv::OUT_GATE, io, 0)
            .unwrap();
        assert!(!patch.cables.iter().any(|x| x.id == id));
    }
}
