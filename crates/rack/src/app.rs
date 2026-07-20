//! Application state: focus, snapshot apply, status — no drawing or gRPC.

use std::collections::HashSet;

use synth_proto::{
    AudioDevice, CableDescriptor, ControlDescriptor, IoChannelMeter, IoEndpointDescriptor,
    ModuleInstanceDescriptor, ModuleType, PatchSnapshot,
};

use crate::build::{self, build_ui_modules};
use crate::model::{
    BuildInput, Focus, Item, ModuleId, PendingCable, UiModule, HINTS,
};

pub struct App {
    pub controls: Vec<ControlDescriptor>,
    pub values: Vec<f32>,
    pub modules: Vec<UiModule>,
    pub voice_modules: Vec<ModuleInstanceDescriptor>,
    pub cables: Vec<CableDescriptor>,
    pub module_types: Vec<ModuleType>,
    pub io_endpoints: Vec<IoEndpointDescriptor>,
    pub audio_devices: Vec<AudioDevice>,
    pub io_meters: Vec<IoChannelMeter>,
    pub scope: Vec<f32>,
    pub active_voices: u32,
    pub output_peak: f32,
    pub focus: Focus,
    pub pending: Option<PendingCable>,
    pub held_notes: HashSet<u8>,
    pub status: String,
    pub hint_idx: usize,
    pub quit: bool,
    pub state_stream_ended: bool,
    /// First module index of the current viewport page.
    pub view_origin: usize,
    /// Terminal size (cols, rows) updated each frame for page math.
    pub term_width: u16,
    pub term_height: u16,
    /// Last computed visible range [start, end) and page info (from draw).
    pub last_visible_start: usize,
    pub last_visible_end: usize,
    pub last_page: usize,
    pub last_pages: usize,
}

impl App {
    pub fn new(
        module_types: Vec<ModuleType>,
        audio_devices: Vec<AudioDevice>,
        snap: PatchSnapshot,
    ) -> Self {
        let mut app = Self {
            controls: Vec::new(),
            values: Vec::new(),
            modules: Vec::new(),
            voice_modules: Vec::new(),
            cables: Vec::new(),
            module_types,
            io_endpoints: Vec::new(),
            audio_devices,
            io_meters: Vec::new(),
            scope: Vec::new(),
            active_voices: 0,
            output_peak: 0.0,
            focus: Focus::default(),
            pending: None,
            held_notes: HashSet::new(),
            status: HINTS[0].into(),
            hint_idx: 0,
            quit: false,
            state_stream_ended: false,
            view_origin: 0,
            term_width: 80,
            term_height: 24,
            last_visible_start: 0,
            last_visible_end: 0,
            last_page: 0,
            last_pages: 1,
        };
        app.apply_snapshot(snap);
        app
    }

    pub fn set_term_size(&mut self, width: u16, height: u16) {
        self.term_width = width.max(1);
        self.term_height = height.max(1);
    }

    pub fn record_layout(
        &mut self,
        visible_start: usize,
        visible_end: usize,
        page: usize,
        pages: usize,
    ) {
        self.last_visible_start = visible_start;
        self.last_visible_end = visible_end;
        self.last_page = page;
        self.last_pages = pages.max(1);
    }

    /// Keep focused module inside the viewport.
    pub fn ensure_focus_visible(&mut self) {
        use crate::draw::layout::{grid_area_from_term, origin_for_focus};
        use crate::draw::panel_views;

        let panels = panel_views(self);
        let area = grid_area_from_term(self.term_width, self.term_height);
        self.view_origin = origin_for_focus(
            &panels,
            area,
            self.focus.module,
            self.view_origin,
        );
    }

    pub fn page_delta(&mut self, delta: isize) {
        use crate::draw::layout::{grid_area_from_term, page_origin};
        use crate::draw::panel_views;

        let panels = panel_views(self);
        let area = grid_area_from_term(self.term_width, self.term_height);
        self.view_origin = page_origin(&panels, area, self.view_origin, delta);
        // Move focus onto the page if it left the window
        if self.focus.module < self.view_origin
            || (self.last_visible_end > 0 && self.focus.module >= self.last_visible_end)
        {
            self.focus.module = self.view_origin.min(self.modules.len().saturating_sub(1));
            self.focus.item = 0;
        }
        self.refresh_status();
    }

    pub fn apply_snapshot(&mut self, snap: PatchSnapshot) {
        let keep_voice = self.focused_voice_id();
        self.controls = snap.controls;
        self.values = self.controls.iter().map(|c| c.default_value).collect();
        self.voice_modules = snap.modules;
        self.cables = snap.cables;
        self.io_endpoints = snap.io_endpoints;
        self.rebuild_ui_modules();
        self.restore_focus(keep_voice);
        self.view_origin = self.view_origin.min(self.modules.len().saturating_sub(1));
        self.ensure_focus_visible();
        self.refresh_status();
    }

    pub fn focused_voice_id(&self) -> Option<u32> {
        match self.modules.get(self.focus.module).map(|m| &m.id) {
            Some(ModuleId::Voice(id)) => Some(*id),
            _ => None,
        }
    }

    pub fn restore_focus(&mut self, prefer_voice: Option<u32>) {
        if self.modules.is_empty() {
            self.focus = Focus::default();
            return;
        }
        if let Some(id) = prefer_voice {
            if let Some(idx) = self
                .modules
                .iter()
                .position(|m| matches!(&m.id, ModuleId::Voice(v) if *v == id))
            {
                self.focus.module = idx;
            }
        }
        self.focus.module = self.focus.module.min(self.modules.len() - 1);
        let n = self.modules[self.focus.module].items.len();
        self.focus.item = if n == 0 {
            0
        } else {
            self.focus.item.min(n - 1)
        };
    }

    pub fn rebuild_ui_modules(&mut self) {
        self.modules = build_ui_modules(BuildInput {
            voice_modules: &self.voice_modules,
            controls: &self.controls,
            module_types: &self.module_types,
            io_endpoints: &self.io_endpoints,
        });
    }

    pub fn focus_module_delta(&mut self, delta: isize) {
        if self.modules.is_empty() {
            return;
        }
        let n = self.modules.len() as isize;
        let mut i = self.focus.module as isize + delta;
        i = ((i % n) + n) % n;
        self.focus.module = i as usize;
        self.focus.item = 0;
        self.ensure_focus_visible();
        self.refresh_status();
    }

    pub fn focus_item_delta(&mut self, delta: isize) {
        let Some(m) = self.modules.get(self.focus.module) else {
            return;
        };
        if m.items.is_empty() {
            return;
        }
        let n = m.items.len() as isize;
        let mut i = self.focus.item as isize + delta;
        i = ((i % n) + n) % n;
        self.focus.item = i as usize;
        self.refresh_status();
    }

    pub fn focused_item(&self) -> Option<&Item> {
        self.modules
            .get(self.focus.module)
            .and_then(|m| m.items.get(self.focus.item))
    }

    pub fn port_name(&self, module_id: u32, port: u32, is_output: bool) -> String {
        build::port_name(
            &self.voice_modules,
            &self.module_types,
            module_id,
            port,
            is_output,
        )
    }

    pub fn module_short(&self, id: u32) -> String {
        build::module_short(&self.voice_modules, id)
    }

    pub fn jack_peer_hint(&self, module_id: u32, port: u32, is_output: bool) -> String {
        build::jack_peer_hint(
            &self.voice_modules,
            &self.module_types,
            &self.cables,
            module_id,
            port,
            is_output,
        )
    }

    pub fn cables_on_jack(
        &self,
        module_id: u32,
        port: u32,
        is_output: bool,
    ) -> Vec<&CableDescriptor> {
        build::cables_on_jack(&self.cables, module_id, port, is_output)
    }

    pub fn next_device_for(&self, endpoint_id: &str) -> Option<String> {
        let ep = self.io_endpoints.iter().find(|e| e.id == endpoint_id)?;
        let want_input = ep.direction == "input";
        let devices: Vec<&AudioDevice> = self
            .audio_devices
            .iter()
            .filter(|d| if want_input { d.is_input } else { d.is_output })
            .collect();
        if devices.is_empty() {
            return None;
        }
        let current = self
            .io_meters
            .iter()
            .find(|m| m.endpoint_id == endpoint_id)
            .map(|m| m.device_name.as_str())
            .unwrap_or(ep.device_name.as_str());
        let idx = devices
            .iter()
            .position(|d| d.name == current)
            .map(|i| (i + 1) % devices.len())
            .unwrap_or(0);
        Some(devices[idx].name.clone())
    }

    pub fn adjust_param(
        &mut self,
        control_index: usize,
        direction: f32,
        fine: bool,
    ) -> Option<(u32, u32, f32)> {
        let c = self.controls.get(control_index)?;
        let step = (c.max - c.min) * if fine { 0.002 } else { 0.02 };
        let v = self.values.get_mut(control_index)?;
        *v = (*v + direction * step).clamp(c.min, c.max);
        Some((c.module, c.param, *v))
    }

    pub fn refresh_status(&mut self) {
        let where_ = if let Some(m) = self.modules.get(self.focus.module) {
            let item_desc = match self.focused_item() {
                Some(Item::Param { control_index }) => {
                    let c = &self.controls[*control_index];
                    let v = self.values[*control_index];
                    format!("{} = {v:.3}", c.name)
                }
                Some(Item::Input { name, port }) => {
                    let mid = self.focused_voice_id().unwrap_or(0);
                    let peer = self.jack_peer_hint(mid, *port, false);
                    if peer.is_empty() {
                        format!("in {name}")
                    } else {
                        format!("in {name}  {peer}")
                    }
                }
                Some(Item::Output { name, port }) => {
                    let mid = self.focused_voice_id().unwrap_or(0);
                    let peer = self.jack_peer_hint(mid, *port, true);
                    if peer.is_empty() {
                        format!("out {name}")
                    } else {
                        format!("out {name}  {peer}")
                    }
                }
                Some(Item::DeviceCycle { endpoint_id }) => format!("device {endpoint_id}"),
                Some(Item::AddKind { display_name, .. }) => format!("add {display_name}"),
                Some(Item::Note { label, midi }) => format!("note {label} ({midi})"),
                Some(Item::Display) => "display".into(),
                None => "—".into(),
            };
            format!("{}  ·  {item_desc}", m.title)
        } else {
            "—".into()
        };

        let action = if let Some(p) = self.pending {
            format!(
                "ARMED {}.{} → pick an input, Enter",
                self.module_short(p.src_module),
                self.port_name(p.src_module, p.src_port, true)
            )
        } else {
            match self.focused_item() {
                Some(Item::Output { .. }) => "Enter: arm cable from this output".into(),
                Some(Item::Input { .. }) => "Enter: connect (arm an output first)".into(),
                Some(Item::AddKind { .. }) => "Enter: add this module".into(),
                Some(Item::DeviceCycle { .. }) => "Enter: cycle device".into(),
                Some(Item::Param { .. }) => "-/= : adjust param".into(),
                Some(Item::Note { .. }) => "Enter: toggle note".into(),
                _ => HINTS[self.hint_idx % HINTS.len()].into(),
            }
        };

        let page = if self.last_pages > 1 {
            format!(
                "  ·  page {}/{}  ( [ ] or PgUp/PgDn )",
                self.last_page + 1,
                self.last_pages
            )
        } else {
            String::new()
        };
        self.status = format!("{where_}  ·  {action}{page}");
    }

    pub fn cycle_hint(&mut self) {
        self.hint_idx = (self.hint_idx + 1) % HINTS.len();
        self.status = HINTS[self.hint_idx].into();
    }
}
