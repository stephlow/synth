//! Core types and constants for the rack TUI (no I/O).

use synth_proto::{ControlDescriptor, IoEndpointDescriptor, ModuleInstanceDescriptor, ModuleType};

pub const DEFAULT_ENGINE_ADDR: &str = "http://127.0.0.1:50051";
pub const DEFAULT_PATCH_FILE: &str = "patch.json";
/// Default directory hint for new save paths.
pub const DEFAULT_PATCH_DIR: &str = "patches";
/// Master IO calibration knobs use this module id (engine `MASTER_IO_MODULE`).
pub const MASTER_IO_MODULE: u32 = u32::MAX;

/// In-UI path entry for save / load (filename + optional directories).
#[derive(Clone, Debug)]
pub enum PathEdit {
    Save { path: String },
    Load { path: String },
}

impl PathEdit {
    pub fn path(&self) -> &str {
        match self {
            Self::Save { path } | Self::Load { path } => path,
        }
    }

    pub fn path_mut(&mut self) -> &mut String {
        match self {
            Self::Save { path } | Self::Load { path } => path,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Save { .. } => "SAVE PATCH",
            Self::Load { .. } => "LOAD PATCH",
        }
    }
}

pub const HINTS: &[&str] = &[
    "arrows move target  ·  Enter opens menu  ·  Esc back  ·  q quit",
    "all actions live in the menu  ·  Move… / params / patch / save from Enter",
    "Outputs… arm jack → Enter on destination module",
];

/// Patch endpoint id for master hardware output (matches `dsp::IO_ENDPOINT_AUDIO_OUT`).
pub const ENDPOINT_AUDIO_OUT: &str = "audio_out";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LatticePos {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Debug)]
pub enum ModuleId {
    /// Patch document module (stable engine id).
    Voice(u32),
    /// Hardware IO endpoint from `patch.io_endpoints` (`audio_out` / `audio_in`).
    Io(String),
}

/// Stable lattice identity across UI rebuilds (not list index).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PlaceKey {
    Voice(u32),
    Io(String),
}

impl PlaceKey {
    pub fn from_module_id(id: &ModuleId) -> Self {
        match id {
            ModuleId::Voice(v) => PlaceKey::Voice(*v),
            ModuleId::Io(s) => PlaceKey::Io(s.clone()),
        }
    }
}

/// Drag-move session: module follows cursor until drop/cancel.
#[derive(Clone, Debug)]
pub struct MoveState {
    pub module_idx: usize,
    pub origin: LatticePos,
    pub key: PlaceKey,
}

#[derive(Clone, Debug)]
pub enum Item {
    Param { control_index: usize },
    Input { port: u32, name: String },
    Output { port: u32, name: String },
}

#[derive(Clone, Debug)]
pub struct UiModule {
    pub id: ModuleId,
    pub title: String,
    pub kind: String,
    pub items: Vec<Item>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PendingCable {
    pub src_module: u32,
    pub src_port: u32,
}

/// Snapshot data the UI rebuild needs (subset of engine state).
pub struct BuildInput<'a> {
    pub voice_modules: &'a [ModuleInstanceDescriptor],
    pub controls: &'a [ControlDescriptor],
    pub module_types: &'a [ModuleType],
    pub io_endpoints: &'a [IoEndpointDescriptor],
}

// ── Mode stack / menus ──────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct MenuState {
    pub title: String,
    /// Full unfiltered item list.
    pub items: Vec<MenuEntry>,
    /// Index into the **filtered** view (see [`MenuState::filtered_indices`]).
    pub selected: usize,
    /// Incremental type-to-filter query (case-insensitive substring on labels).
    pub filter: String,
}

impl MenuState {
    pub fn new(title: impl Into<String>, items: Vec<MenuEntry>) -> Self {
        Self {
            title: title.into(),
            items,
            selected: 0,
            filter: String::new(),
        }
    }

    /// Indices into `items` that match the current filter (all items if empty).
    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.filter.is_empty() {
            return (0..self.items.len()).collect();
        }
        let q = self.filter.to_ascii_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, e)| e.label.to_ascii_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn filtered_len(&self) -> usize {
        if self.filter.is_empty() {
            self.items.len()
        } else {
            self.filtered_indices().len()
        }
    }

    /// Source index of the currently selected filtered row.
    pub fn selected_source_index(&self) -> Option<usize> {
        let filtered = self.filtered_indices();
        if filtered.is_empty() {
            return None;
        }
        let sel = self.selected.min(filtered.len() - 1);
        Some(filtered[sel])
    }

    pub fn selected_entry(&self) -> Option<&MenuEntry> {
        self.selected_source_index()
            .and_then(|i| self.items.get(i))
    }

    /// Clamp selection into the filtered range (call after filter changes).
    pub fn clamp_selection(&mut self) {
        let n = self.filtered_len();
        if n == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(n - 1);
        }
    }

    pub fn push_filter_char(&mut self, ch: char) {
        self.filter.push(ch);
        self.selected = 0;
        self.clamp_selection();
    }

    pub fn pop_filter_char(&mut self) -> bool {
        if self.filter.is_empty() {
            return false;
        }
        self.filter.pop();
        self.selected = 0;
        self.clamp_selection();
        true
    }

    pub fn clear_filter(&mut self) -> bool {
        if self.filter.is_empty() {
            return false;
        }
        self.filter.clear();
        self.selected = 0;
        true
    }
}

#[derive(Clone, Debug)]
pub struct MenuEntry {
    pub label: String,
    pub action: Option<MenuAction>,
    /// Visual hint that activating opens a deeper menu.
    pub submenu: bool,
}

impl MenuEntry {
    pub fn action(label: impl Into<String>, action: MenuAction) -> Self {
        Self {
            label: label.into(),
            action: Some(action),
            submenu: false,
        }
    }

    pub fn submenu(label: impl Into<String>, action: MenuAction) -> Self {
        Self {
            label: label.into(),
            action: Some(action),
            submenu: true,
        }
    }

    pub fn value(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            action: None,
            submenu: false,
        }
    }
}

/// Actions fired from menu entries (built by keys/menu layer).
#[derive(Clone, Debug)]
pub enum MenuAction {
    /// Push Parameters submenu for module index.
    OpenParams { module_idx: usize },
    OpenOutputs { module_idx: usize },
    OpenInputs { module_idx: usize },
    OpenDisconnect { module_idx: usize },
    OpenAddKinds,
    OpenDevices { endpoint_id: String },
    OpenInfo { module_idx: usize },
    /// Enter ParamEdit mode.
    EditParam { control_index: usize },
    /// Arm source jack; navigate to a destination module (or Audio Out) and Enter.
    ArmOutput {
        module: u32,
        port: u32,
        name: String,
    },
    ConnectInput {
        module: u32,
        port: u32,
        name: String,
    },
    /// Route a voice-module output to master Audio Out (L/R dual mono).
    SetOutputTap {
        module: u32,
        port: u32,
        name: String,
    },
    DisconnectCable {
        cable_id: u32,
        src_module: u32,
        src_port: u32,
        dst_module: u32,
        dst_port: u32,
    },
    AddModule {
        kind: String,
        display_name: String,
    },
    DeleteModule { module_id: u32 },
    SetDevice {
        endpoint_id: String,
        device_name: String,
    },
    /// Pick up module and drag with arrows.
    StartMove { module_idx: usize },
    /// Reset engine to the built-in default patch.
    NewPatch,
    /// Open path entry to save document JSON to disk.
    SavePatch,
    /// Open path entry to load document JSON from disk.
    LoadPatch,
    /// Open Theme… preset list.
    OpenThemes,
    /// Apply a built-in theme by index into `ThemeId::ALL`.
    SetTheme { theme_index: usize },
}

/// Short kind label for a tile (3 chars max-ish).
pub fn kind_abbrev(kind: &str) -> String {
    let k = kind.to_ascii_uppercase();
    match k.as_str() {
        "VCO" | "OSC" => "VCO".into(),
        "VCF" | "FILTER" => "VCF".into(),
        "VCA" => "VCA".into(),
        "LFO" => "LFO".into(),
        "ENV" | "ENVELOPE" | "ADSR" => "ENV".into(),
        "MIX" | "MIXER" => "MIX".into(),
        "NOISE" => "NSE".into(),
        "S&H" | "SAMPLE_HOLD" | "SAMPLE-HOLD" => "S&H".into(),
        "ATTENUVERTER" | "ATV" => "ATV".into(),
        "MIDI_TO_CV" | "M2CV" | "MIDITOCV" => "M2C".into(),
        "IO" => "IO".into(),
        "SCOPE" => "SCP".into(),
        "METER" => "MTR".into(),
        "KEYBOARD" | "KEYS" => "KEY".into(),
        other => {
            let chars: String = other.chars().filter(|c| c.is_ascii_alphanumeric()).take(3).collect();
            if chars.is_empty() {
                "···".into()
            } else {
                chars
            }
        }
    }
}
