//! Core types and constants for the rack TUI (no I/O).

use synth_proto::{ControlDescriptor, IoEndpointDescriptor, ModuleInstanceDescriptor, ModuleType};

pub const NOTE_KEYS: &[(char, u8)] = &[
    ('z', 60),
    ('x', 62),
    ('c', 64),
    ('v', 65),
    ('b', 67),
    ('n', 69),
    ('m', 71),
];

pub const DEFAULT_ENGINE_ADDR: &str = "http://127.0.0.1:50051";
pub const DEFAULT_LOG_FILE: &str = "rack.log";
pub const DEFAULT_PATCH_FILE: &str = "patch.json";
/// Minimum faceplate width (IN | KNOBS | OUT still usable).
pub const PANEL_WIDTH_MIN: u16 = 36;
/// Maximum panel width on large terminals.
pub const PANEL_WIDTH_MAX: u16 = 56;
/// Gutter range between modules.
pub const PANEL_GAP_MIN: u16 = 1;
pub const PANEL_GAP_MAX: u16 = 3;
/// Master IO calibration knobs use this module id (engine `MASTER_IO_MODULE`).
pub const MASTER_IO_MODULE: u32 = u32::MAX;

pub const HINTS: &[&str] = &[
    "h/l module  j/k item  Enter jack/add/device  -/= param  x unplug  D del  S/L save  z-m notes",
    "patch: focus an output jack, Enter, then an input jack, Enter  ·  Esc clears pending",
    "Add module: navigate to the Add panel (last), pick a type, Enter",
];

#[derive(Clone, Debug)]
pub enum ModuleId {
    Voice(u32),
    Io(String),
    Scope,
    Meter,
    Keyboard,
    Add,
}

#[derive(Clone, Debug)]
pub enum Item {
    Param { control_index: usize },
    Input { port: u32, name: String },
    Output { port: u32, name: String },
    DeviceCycle { endpoint_id: String },
    AddKind { kind: String, display_name: String },
    Note { midi: u8, label: char },
    Display,
}

#[derive(Clone, Debug)]
pub struct UiModule {
    pub id: ModuleId,
    pub title: String,
    pub kind: String,
    pub items: Vec<Item>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Focus {
    pub module: usize,
    pub item: usize,
}

#[derive(Clone, Copy, Debug)]
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
