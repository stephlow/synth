//! Application state: lattice cursor, mode stack, snapshot apply — no drawing or gRPC.

use std::collections::{HashMap, HashSet};

use synth_proto::{
    AudioDevice, CableDescriptor, CableSignal, ControlDescriptor, IoChannelMeter,
    IoEndpointDescriptor, ModuleInstanceDescriptor, ModuleSignal, ModuleType, PatchRolesDescriptor,
    PatchSnapshot,
};

use crate::build::{self, build_ui_modules};
use crate::model::{
    kind_abbrev, BuildInput, Item, LatticePos, MenuState, ModuleId, MoveState, PathEdit,
    PendingCable, PlaceKey, UiModule, HINTS,
};
use crate::draw::style::ThemeId;
use crate::place::{self, first_free_near, lattice_bounds, layout_modules, module_at, place_key};
use crate::settings::{self, UserSettings};

pub struct App {
    pub controls: Vec<ControlDescriptor>,
    pub values: Vec<f32>,
    pub modules: Vec<UiModule>,
    pub voice_modules: Vec<ModuleInstanceDescriptor>,
    pub cables: Vec<CableDescriptor>,
    pub module_types: Vec<ModuleType>,
    pub io_endpoints: Vec<IoEndpointDescriptor>,
    /// Main-bus / role modules from the last patch snapshot.
    pub roles: PatchRolesDescriptor,
    pub audio_devices: Vec<AudioDevice>,
    pub io_meters: Vec<IoChannelMeter>,
    pub scope: Vec<f32>,
    pub active_voices: u32,
    pub output_peak: f32,
    /// Live per-module peak + waveform from the engine (document id → signal).
    pub module_signals: HashMap<u32, ModuleSignal>,
    /// Live per-cable peak (cable id → peak).
    pub cable_signals: HashMap<u32, f32>,
    pub pending: Option<PendingCable>,

    pub status: String,
    pub quit: bool,
    pub state_stream_ended: bool,
    /// Last topology revision applied from the engine (`EngineState.patch_generation`).
    pub patch_generation: u64,

    /// Lattice target cursor.
    pub cursor: LatticePos,
    /// Top-left lattice cell visible in the viewport.
    pub view_origin: LatticePos,
    /// Parallel to `modules`: lattice placement (None = not on grid).
    pub positions: Vec<Option<LatticePos>>,
    /// Remembered positions by stable module identity (survives rebuilds / moves).
    pub placement_memory: HashMap<PlaceKey, LatticePos>,
    /// Lattice extent (cols, rows).
    pub grid_cols: u16,
    pub grid_rows: u16,
    /// Preferred canvas size from the patch document (`coordinates` space).
    pub patch_canvas: Option<(u16, u16)>,

    /// Nested command menus (top of stack is active). Empty = lattice mode.
    pub menu_stack: Vec<MenuState>,
    /// When set, ParamEdit mode (still may have menu_stack under it for Esc).
    pub param_edit: Option<usize>,
    /// When set, module is being dragged on the lattice.
    pub moving: Option<MoveState>,
    /// When set, path entry overlay for save/load.
    pub path_edit: Option<PathEdit>,
    /// Last successful save/load path (pre-fills the path editor).
    pub last_patch_path: String,

    pub term_width: u16,
    pub term_height: u16,

    /// Committed UI color theme (presets).
    pub theme_id: ThemeId,
    /// Live preview while Theme… menu is open (None = use `theme_id`).
    pub theme_preview: Option<ThemeId>,

    /// User settings (theme + last path); persisted via `directories`.
    settings: UserSettings,
}

impl App {
    pub fn new(
        module_types: Vec<ModuleType>,
        audio_devices: Vec<AudioDevice>,
        snap: PatchSnapshot,
    ) -> Self {
        let user = settings::load();
        let theme_id = user.theme_id();
        let last_patch_path = user.last_path_or_default();
        let mut app = Self {
            controls: Vec::new(),
            values: Vec::new(),
            modules: Vec::new(),
            voice_modules: Vec::new(),
            cables: Vec::new(),
            module_types,
            io_endpoints: Vec::new(),
            roles: PatchRolesDescriptor::default(),
            audio_devices,
            io_meters: Vec::new(),
            scope: Vec::new(),
            active_voices: 0,
            output_peak: 0.0,
            module_signals: HashMap::new(),
            cable_signals: HashMap::new(),
            pending: None,

            status: HINTS[0].into(),
            quit: false,
            state_stream_ended: false,
            patch_generation: 0,
            cursor: LatticePos::default(),
            view_origin: LatticePos::default(),
            positions: Vec::new(),
            placement_memory: HashMap::new(),
            grid_cols: place::MIN_COLS,
            grid_rows: place::MIN_ROWS,
            patch_canvas: None,
            menu_stack: Vec::new(),
            param_edit: None,
            moving: None,
            path_edit: None,
            last_patch_path,
            term_width: 80,
            term_height: 24,
            theme_id,
            theme_preview: None,
            settings: user,
        };
        app.apply_snapshot(snap);
        app
    }

    pub fn persist_settings(&mut self) {
        self.settings.set_theme_id(self.theme_id);
        self.settings.last_patch_path = Some(self.last_patch_path.clone());
        settings::save(&self.settings);
    }

    pub fn begin_save_path(&mut self) {
        self.clear_menus();
        self.path_edit = Some(PathEdit::Save {
            path: self.last_patch_path.clone(),
        });
        self.refresh_status();
    }

    pub fn begin_load_path(&mut self) {
        self.clear_menus();
        self.path_edit = Some(PathEdit::Load {
            path: self.last_patch_path.clone(),
        });
        self.refresh_status();
    }

    pub fn cancel_path_edit(&mut self) {
        self.path_edit = None;
        self.refresh_status();
    }

    /// Theme used for drawing (preview if browsing Theme…, else committed).
    pub fn display_theme(&self) -> ThemeId {
        self.theme_preview.unwrap_or(self.theme_id)
    }

    pub fn set_theme(&mut self, id: ThemeId) {
        self.theme_id = id;
        self.theme_preview = None;
        self.persist_settings();
        self.status = format!("theme → {}", id.label());
    }

    /// Preview the theme under the Theme… menu cursor.
    pub fn preview_theme_from_menu(&mut self) {
        let Some(menu) = self.menu_stack.last() else {
            return;
        };
        if menu.title != "theme" {
            return;
        }
        let theme_index = match menu.selected_entry().and_then(|e| e.action.as_ref()) {
            Some(crate::model::MenuAction::SetTheme { theme_index }) => *theme_index,
            _ => return,
        };
        if let Some(id) = ThemeId::ALL.get(theme_index).copied() {
            self.theme_preview = Some(id);
            self.status = format!("preview · {}  ·  Enter keep  ·  Esc cancel", id.label());
        }
    }

    pub fn clear_theme_preview(&mut self) {
        self.theme_preview = None;
    }

    pub fn set_term_size(&mut self, width: u16, height: u16) {
        let nw = width.max(1);
        let nh = height.max(1);
        let changed = nw != self.term_width || nh != self.term_height;
        self.term_width = nw;
        self.term_height = nh;
        if changed {
            // Keep placements; only expand lattice to fill the new viewport.
            self.expand_lattice_bounds();
            self.ensure_cursor_visible();
        }
    }

    pub fn apply_snapshot(&mut self, snap: PatchSnapshot) {
        let keep_voice = self.cursor_voice_id();
        let keep_cursor = self.cursor;
        // Capture current positions before rebuild so user moves survive topology updates.
        self.commit_positions_to_memory();

        self.controls = snap.controls;
        self.values = self.controls.iter().map(|c| c.default_value).collect();
        self.voice_modules = snap.modules;
        self.cables = snap.cables;
        self.io_endpoints = snap.io_endpoints;
        self.roles = snap.roles.unwrap_or_default();
        self.patch_canvas = snap.canvas.as_ref().map(|c| {
            (
                c.width.min(u32::from(u16::MAX)) as u16,
                c.height.min(u32::from(u16::MAX)) as u16,
            )
        });
        // Engine coordinates win for voice modules (required on every module).
        for m in &self.voice_modules {
            if let Some(c) = m.coordinates.as_ref() {
                self.placement_memory.insert(
                    PlaceKey::Voice(m.id),
                    LatticePos {
                        x: c.x.min(u32::from(u16::MAX)) as u16,
                        y: c.y.min(u32::from(u16::MAX)) as u16,
                    },
                );
            }
        }
        // IO endpoint tiles (Audio Out / Audio In) always carry coordinates.
        for ep in &self.io_endpoints {
            if let Some(c) = ep.coordinates.as_ref() {
                self.placement_memory.insert(
                    PlaceKey::Io(ep.id.clone()),
                    LatticePos {
                        x: c.x.min(u32::from(u16::MAX)) as u16,
                        y: c.y.min(u32::from(u16::MAX)) as u16,
                    },
                );
            }
        }
        self.rebuild_ui_modules();
        self.rebind_placements(None);

        // Prefer previous cursor / focused voice.
        if keep_cursor.x < self.grid_cols && keep_cursor.y < self.grid_rows {
            self.cursor = keep_cursor;
        }
        if let Some(id) = keep_voice {
            if let Some(idx) = self
                .modules
                .iter()
                .position(|m| matches!(&m.id, ModuleId::Voice(v) if *v == id))
            {
                if let Some(pos) = self.positions.get(idx).copied().flatten() {
                    self.cursor = pos;
                }
            }
        }
        self.ensure_cursor_visible();
        self.refresh_status();
    }

    pub fn rebuild_ui_modules(&mut self) {
        self.modules = build_ui_modules(BuildInput {
            voice_modules: &self.voice_modules,
            controls: &self.controls,
            module_types: &self.module_types,
            io_endpoints: &self.io_endpoints,
        });
    }

    /// Write parallel `positions` into `placement_memory`.
    pub fn commit_positions_to_memory(&mut self) {
        for (i, m) in self.modules.iter().enumerate() {
            if let Some(pos) = self.positions.get(i).copied().flatten() {
                self.placement_memory.insert(place_key(m), pos);
            }
        }
    }

    /// Assign lattice cells from memory + auto seeds for newcomers.
    /// `prefer_new` places a newly added voice at the cursor when free.
    pub fn rebind_placements(&mut self, prefer_new: Option<(PlaceKey, LatticePos)>) {
        use crate::draw::layout::{grid_area_from_term, Geom};

        let canvas = grid_area_from_term(self.term_width, self.term_height);
        let geom = Geom::from_canvas(canvas);
        let (vis_c, vis_r) = geom.visible_span(canvas);
        let pack = place::pack_cols_for(canvas.width, geom.cell_step_x());
        let auto = layout_modules(&self.modules, pack);

        // Drop memory for modules that no longer exist.
        let live: HashSet<PlaceKey> = self.modules.iter().map(place_key).collect();
        self.placement_memory.retain(|k, _| live.contains(k));

        if let Some((ref key, pos)) = prefer_new {
            self.placement_memory.insert(key.clone(), pos);
        }

        let mut occupied: HashSet<LatticePos> = HashSet::new();
        let mut positions = vec![None; self.modules.len()];

        // Pass 1: restore remembered positions (collision → leave for pass 2).
        for (i, m) in self.modules.iter().enumerate() {
            let key = place_key(m);
            if let Some(pos) = self.placement_memory.get(&key).copied() {
                if !occupied.contains(&pos) {
                    positions[i] = Some(pos);
                    occupied.insert(pos);
                }
            }
        }

        // Pass 2: place the rest near auto seed or origin.
        let mut max_c = vis_c.saturating_sub(1).max(place::MIN_COLS);
        let mut max_r = vis_r.saturating_sub(1).max(place::MIN_ROWS);
        for p in occupied.iter() {
            max_c = max_c.max(p.x);
            max_r = max_r.max(p.y);
        }

        for (i, m) in self.modules.iter().enumerate() {
            if positions[i].is_some() {
                continue;
            }
            let key = place_key(m);
            let prefer = auto
                .get(i)
                .copied()
                .flatten()
                .or_else(|| self.placement_memory.get(&key).copied())
                .unwrap_or(LatticePos::default());
            let cols = (max_c + 4).max(vis_c).max(place::MIN_COLS);
            let rows = (max_r + 3).max(vis_r).max(place::MIN_ROWS);
            let pos = first_free_near(&occupied, prefer, cols, rows);
            positions[i] = Some(pos);
            occupied.insert(pos);
            self.placement_memory.insert(key, pos);
            max_c = max_c.max(pos.x);
            max_r = max_r.max(pos.y);
        }

        self.positions = positions;
        let (cw, ch) = self.patch_canvas.unzip();
        let (cols, rows) = lattice_bounds(&self.positions, vis_c, vis_r, cw, ch);
        self.grid_cols = cols;
        self.grid_rows = rows;
        self.cursor.x = self.cursor.x.min(cols.saturating_sub(1));
        self.cursor.y = self.cursor.y.min(rows.saturating_sub(1));
    }

    /// Grow grid to fill viewport without moving modules.
    pub fn expand_lattice_bounds(&mut self) {
        use crate::draw::layout::{grid_area_from_term, Geom};

        let canvas = grid_area_from_term(self.term_width, self.term_height);
        let geom = Geom::from_canvas(canvas);
        let (vis_c, vis_r) = geom.visible_span(canvas);
        let (cw, ch) = self.patch_canvas.unzip();
        let (cols, rows) = lattice_bounds(&self.positions, vis_c, vis_r, cw, ch);
        self.grid_cols = cols;
        self.grid_rows = rows;
        self.cursor.x = self.cursor.x.min(cols.saturating_sub(1));
        self.cursor.y = self.cursor.y.min(rows.saturating_sub(1));
    }

    pub fn module_at_cursor(&self) -> Option<usize> {
        module_at(&self.positions, self.cursor)
    }

    pub fn cursor_voice_id(&self) -> Option<u32> {
        let idx = self.module_at_cursor()?;
        match self.modules.get(idx).map(|m| &m.id) {
            Some(ModuleId::Voice(id)) => Some(*id),
            _ => None,
        }
    }

    /// Begin dragging the module under the cursor (or at `module_idx`).
    pub fn start_move(&mut self, module_idx: usize) -> bool {
        let Some(origin) = self.positions.get(module_idx).copied().flatten() else {
            return false;
        };
        let Some(m) = self.modules.get(module_idx) else {
            return false;
        };
        let key = place_key(m);
        self.menu_stack.clear();
        self.param_edit = None;
        self.pending = None;
        self.moving = Some(MoveState {
            module_idx,
            origin,
            key,
        });
        self.cursor = origin;
        self.ensure_cursor_visible();
        self.refresh_status();
        true
    }

    pub fn cancel_move(&mut self) {
        if let Some(mv) = self.moving.take() {
            if let Some(slot) = self.positions.get_mut(mv.module_idx) {
                *slot = Some(mv.origin);
            }
            self.placement_memory.insert(mv.key, mv.origin);
            self.cursor = mv.origin;
            self.ensure_cursor_visible();
            self.status = "move cancelled".into();
        }
        self.refresh_status();
    }

    pub fn confirm_move(&mut self) {
        if let Some(mv) = self.moving.take() {
            if let Some(pos) = self.positions.get(mv.module_idx).copied().flatten() {
                self.placement_memory.insert(mv.key, pos);
                self.cursor = pos;
                self.status = format!("placed at {},{}", pos.x, pos.y);
            }
        }
        self.ensure_cursor_visible();
        self.refresh_status();
    }

    /// Voice module lattice entries for `SetCoordinates` (id, x, y).
    pub fn voice_coordinate_entries(&self) -> Vec<(u32, u32, u32)> {
        self.modules
            .iter()
            .zip(self.positions.iter())
            .filter_map(|(m, pos)| match (&m.id, pos) {
                (ModuleId::Voice(id), Some(p)) => Some((*id, u32::from(p.x), u32::from(p.y))),
                _ => None,
            })
            .collect()
    }

    pub fn move_cursor(&mut self, dcol: i32, drow: i32) {
        if !self.menu_stack.is_empty() || self.param_edit.is_some() {
            return;
        }

        if self.moving.is_some() {
            self.drag_moving(dcol, drow);
            return;
        }

        let nc = self.cursor.x as i32 + dcol;
        let nr = self.cursor.y as i32 + drow;
        self.cursor.x = nc.clamp(0, self.grid_cols as i32 - 1) as u16;
        self.cursor.y = nr.clamp(0, self.grid_rows as i32 - 1) as u16;
        self.ensure_cursor_visible();
        self.refresh_status();
    }

    /// Drag the picked-up module; swap if the target cell is occupied.
    fn drag_moving(&mut self, dcol: i32, drow: i32) {
        let Some(mv) = self.moving.clone() else {
            return;
        };
        let idx = mv.module_idx;
        let Some(cur) = self.positions.get(idx).copied().flatten() else {
            return;
        };

        // Allow growing the lattice while dragging past the edge.
        let mut nc = cur.x as i32 + dcol;
        let mut nr = cur.y as i32 + drow;
        if nc < 0 {
            nc = 0;
        }
        if nr < 0 {
            nr = 0;
        }
        let mut new_pos = LatticePos {
            x: nc as u16,
            y: nr as u16,
        };
        if new_pos.x >= self.grid_cols {
            self.grid_cols = new_pos.x + 2;
        }
        if new_pos.y >= self.grid_rows {
            self.grid_rows = new_pos.y + 2;
        }
        // Clamp to (possibly expanded) grid.
        new_pos.x = new_pos.x.min(self.grid_cols.saturating_sub(1));
        new_pos.y = new_pos.y.min(self.grid_rows.saturating_sub(1));

        if new_pos == cur {
            self.cursor = new_pos;
            self.ensure_cursor_visible();
            self.refresh_status();
            return;
        }

        if let Some(other) = module_at(&self.positions, new_pos) {
            if other != idx {
                // Swap.
                if let Some(other_pos) = self.positions.get_mut(other) {
                    *other_pos = Some(cur);
                }
                if let Some(m) = self.modules.get(other) {
                    self.placement_memory.insert(place_key(m), cur);
                }
            }
        }

        if let Some(slot) = self.positions.get_mut(idx) {
            *slot = Some(new_pos);
        }
        self.placement_memory.insert(mv.key, new_pos);
        self.cursor = new_pos;
        self.ensure_cursor_visible();
        self.refresh_status();
    }

    /// After adding a module, seat it at `prefer` if free (or nearby).
    pub fn place_new_voice_at(&mut self, voice_id: u32, prefer: LatticePos) {
        let key = PlaceKey::Voice(voice_id);
        let occupied: HashSet<LatticePos> = self.positions.iter().flatten().copied().collect();
        let pos = if !occupied.contains(&prefer) {
            prefer
        } else {
            first_free_near(
                &occupied,
                prefer,
                self.grid_cols.max(prefer.x + 2),
                self.grid_rows.max(prefer.y + 2),
            )
        };
        self.placement_memory.insert(key.clone(), pos);
        // Rebind so the new module index picks up memory.
        self.rebind_placements(Some((key, pos)));
        self.cursor = pos;
        self.ensure_cursor_visible();
    }

    pub fn ensure_cursor_visible(&mut self) {
        use crate::draw::layout::{grid_area_from_term, Geom};

        let canvas = grid_area_from_term(self.term_width, self.term_height);
        let geom = Geom::from_canvas(canvas);
        let (vis_c, vis_r) = geom.visible_span(canvas);

        if self.cursor.x < self.view_origin.x {
            self.view_origin.x = self.cursor.x;
        } else if self.cursor.x >= self.view_origin.x.saturating_add(vis_c) {
            self.view_origin.x = self.cursor.x.saturating_sub(vis_c.saturating_sub(1));
        }
        if self.cursor.y < self.view_origin.y {
            self.view_origin.y = self.cursor.y;
        } else if self.cursor.y >= self.view_origin.y.saturating_add(vis_r) {
            self.view_origin.y = self.cursor.y.saturating_sub(vis_r.saturating_sub(1));
        }
        // Clamp origin so we don't pan past the lattice.
        self.view_origin.x = self
            .view_origin
            .x
            .min(self.grid_cols.saturating_sub(vis_c));
        self.view_origin.y = self
            .view_origin
            .y
            .min(self.grid_rows.saturating_sub(vis_r));
    }

    pub fn push_menu(&mut self, menu: MenuState) {
        let is_theme = menu.title == "theme";
        self.menu_stack.push(menu);
        if is_theme {
            self.preview_theme_from_menu();
        } else {
            self.refresh_status();
        }
    }

    pub fn pop_menu(&mut self) {
        let leaving_theme = self
            .menu_stack
            .last()
            .is_some_and(|m| m.title == "theme");
        self.menu_stack.pop();
        if leaving_theme {
            self.clear_theme_preview();
        }
        self.refresh_status();
    }

    pub fn clear_menus(&mut self) {
        self.menu_stack.clear();
        self.param_edit = None;
        self.clear_theme_preview();
        // Do not clear `moving` — menus close when starting a move.
        self.refresh_status();
    }

    pub fn menu_selected_delta(&mut self, delta: isize) {
        let Some(menu) = self.menu_stack.last_mut() else {
            return;
        };
        let n = menu.filtered_len() as isize;
        if n == 0 {
            return;
        }
        let mut i = menu.selected as isize + delta;
        i = ((i % n) + n) % n;
        menu.selected = i as usize;
        let is_theme = menu.title == "theme";
        if is_theme {
            self.preview_theme_from_menu();
        } else {
            self.refresh_status();
        }
    }

    /// Type-to-filter: append a character to the active menu filter.
    pub fn menu_filter_char(&mut self, ch: char) {
        let Some(menu) = self.menu_stack.last_mut() else {
            return;
        };
        if ch.is_control() {
            return;
        }
        menu.push_filter_char(ch.to_ascii_lowercase());
        let is_theme = menu.title == "theme";
        if is_theme {
            self.preview_theme_from_menu();
        } else {
            self.refresh_status();
        }
    }

    /// Backspace on the menu filter. Returns true if a char was removed.
    pub fn menu_filter_backspace(&mut self) -> bool {
        let Some(menu) = self.menu_stack.last_mut() else {
            return false;
        };
        if !menu.pop_filter_char() {
            return false;
        }
        let is_theme = menu.title == "theme";
        if is_theme {
            self.preview_theme_from_menu();
        } else {
            self.refresh_status();
        }
        true
    }

    /// Clear filter without leaving the menu. Returns true if filter was non-empty.
    pub fn menu_filter_clear(&mut self) -> bool {
        let Some(menu) = self.menu_stack.last_mut() else {
            return false;
        };
        if !menu.clear_filter() {
            return false;
        }
        let is_theme = menu.title == "theme";
        if is_theme {
            self.preview_theme_from_menu();
        } else {
            self.refresh_status();
        }
        true
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

    pub fn cables_on_module(&self, module_id: u32) -> Vec<&CableDescriptor> {
        self.cables
            .iter()
            .filter(|c| c.src_module == module_id || c.dst_module == module_id)
            .collect()
    }

    pub fn module_has_cables(&self, module_id: u32) -> bool {
        self.cables
            .iter()
            .any(|c| c.src_module == module_id || c.dst_module == module_id)
    }

    /// Soft-lit: connected or utility present (vs fully dim empty-ish).
    pub fn tile_soft(&self, module_idx: usize) -> bool {
        let Some(m) = self.modules.get(module_idx) else {
            return false;
        };
        match &m.id {
            ModuleId::Voice(id) => self.module_has_cables(*id),
            ModuleId::Io(_) => true,
        }
    }

    pub fn tile_label(&self, module_idx: usize) -> String {
        let Some(m) = self.modules.get(module_idx) else {
            return String::new();
        };
        // Prefer short lowercase for the micro label.
        kind_abbrev(&m.kind).to_ascii_lowercase()
    }

    /// Apply live meters from a StreamState frame.
    pub fn apply_live_signals(
        &mut self,
        module_signals: Vec<ModuleSignal>,
        cable_signals: Vec<CableSignal>,
    ) {
        self.module_signals.clear();
        for s in module_signals {
            self.module_signals.insert(s.module_id, s);
        }
        self.cable_signals.clear();
        for c in cable_signals {
            self.cable_signals.insert(c.cable_id, c.peak);
        }
    }

    /// Live peak for a voice module document id (0 if unknown/silent).
    pub fn module_peak(&self, module_id: u32) -> f32 {
        self.module_signals
            .get(&module_id)
            .map(|s| s.peak)
            .unwrap_or(0.0)
            .clamp(0.0, 1.5)
    }

    /// Live decimated waveform for a voice module (empty if none).
    pub fn module_waveform(&self, module_id: u32) -> Option<&[f32]> {
        self.module_signals
            .get(&module_id)
            .map(|s| s.waveform.as_slice())
            .filter(|w| !w.is_empty())
    }

    /// Live peak for a cable (0 if unknown).
    pub fn cable_peak(&self, cable_id: u32) -> f32 {
        self.cable_signals
            .get(&cable_id)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.5)
    }

    /// 0..1 activity for tile body fill — prefer live module peak.
    pub fn module_intensity(&self, module_idx: usize) -> f32 {
        let Some(m) = self.modules.get(module_idx) else {
            return 0.0;
        };
        let voices_hot = self.active_voices > 0;
        match &m.id {
            ModuleId::Voice(id) => {
                // Live peak from engine; scale aggressively so quiet CV still
                // reads and audio hits full brightness.
                let p = self.module_peak(*id);
                let live = if p < 0.001 {
                    0.0
                } else {
                    // sqrt expands small peaks (envelope CV, LFO) into the UI range.
                    p.sqrt().clamp(0.2, 1.0)
                };
                if live > 0.0 {
                    live
                } else if voices_hot && self.module_has_cables(*id) {
                    // Fallback if stream hasn't delivered module_signals yet.
                    0.75
                } else if self.module_has_cables(*id) {
                    0.18
                } else {
                    0.08
                }
            }
            ModuleId::Io(epid) => {
                let peak = self
                    .io_meters
                    .iter()
                    .find(|m| m.endpoint_id == *epid)
                    .and_then(|m| m.peak.first().copied())
                    .unwrap_or(0.0);
                let connected = self
                    .io_meters
                    .iter()
                    .find(|m| m.endpoint_id == *epid)
                    .map(|m| m.connected)
                    .unwrap_or(false);
                if peak > 0.001 {
                    peak.sqrt().clamp(0.2, 1.0)
                } else if connected {
                    0.25
                } else {
                    0.1
                }
            }
        }
    }

    /// Voice id under the target (or being moved) for cable hover highlight.
    pub fn hover_voice_id(&self) -> Option<u32> {
        if let Some(mv) = &self.moving {
            return match self.modules.get(mv.module_idx).map(|m| &m.id) {
                Some(ModuleId::Voice(id)) => Some(*id),
                _ => None,
            };
        }
        // Only when menus closed so lattice targeting is live.
        if !self.menu_stack.is_empty() || self.param_edit.is_some() {
            return None;
        }
        let idx = self.module_at_cursor()?;
        match self.modules.get(idx).map(|m| &m.id) {
            Some(ModuleId::Voice(id)) => Some(*id),
            _ => None,
        }
    }

    /// Master Audio Out endpoint (hardware L/R + CV), not a voice-graph module.
    pub fn is_master_audio_out(&self, module_idx: usize) -> bool {
        use crate::model::ENDPOINT_AUDIO_OUT;
        match self.modules.get(module_idx).map(|m| &m.id) {
            Some(ModuleId::Io(epid)) => {
                epid == ENDPOINT_AUDIO_OUT
                    || self
                        .io_endpoints
                        .iter()
                        .find(|e| e.id == *epid)
                        .is_some_and(|e| e.direction == "output")
            }
            _ => false,
        }
    }

    /// Human label for the current main-bus tap (`roles.output_tap_*`).
    pub fn main_bus_source_label(&self) -> String {
        let m = self.roles.output_tap_module;
        let p = self.roles.output_tap_port;
        format!(
            "{}.{}",
            self.module_short(m),
            self.port_name(m, p, true)
        )
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

    pub fn devices_for_endpoint(&self, endpoint_id: &str) -> Vec<String> {
        let Some(ep) = self.io_endpoints.iter().find(|e| e.id == endpoint_id) else {
            return Vec::new();
        };
        let want_input = ep.direction == "input";
        self.audio_devices
            .iter()
            .filter(|d| if want_input { d.is_input } else { d.is_output })
            .map(|d| d.name.clone())
            .collect()
    }

    pub fn adjust_param(
        &mut self,
        control_index: usize,
        direction: f32,
        fine: bool,
    ) -> Option<(u32, u32, f32)> {
        let c = self.controls.get(control_index)?.clone();
        let v = self.values.get_mut(control_index)?;
        *v = crate::param_fmt::step_value(&c, *v, direction, fine, &self.module_types);
        Some((c.module, c.param, *v))
    }

    /// Formatted param value (scale names, integer steps, or float).
    pub fn format_param(&self, control_index: usize) -> String {
        let Some(c) = self.controls.get(control_index) else {
            return String::new();
        };
        let v = self.values.get(control_index).copied().unwrap_or(0.0);
        crate::param_fmt::format_value(c, v, &self.module_types)
    }

    /// Read a module param by UI name (defaults if missing).
    fn param_named(&self, module_idx: usize, name: &str, default: f32) -> f32 {
        let Some(m) = self.modules.get(module_idx) else {
            return default;
        };
        for it in &m.items {
            let Item::Param { control_index } = it else {
                continue;
            };
            let Some(c) = self.controls.get(*control_index) else {
                continue;
            };
            if c.name == name {
                return self.values.get(*control_index).copied().unwrap_or(default);
            }
        }
        default
    }

    /// Param-derived glyph state for kind-specific tile art.
    pub fn glyph_detail(&self, module_idx: usize) -> crate::draw::glyph::GlyphDetail {
        use crate::draw::glyph::GlyphDetail;
        let Some(m) = self.modules.get(module_idx) else {
            return GlyphDetail::None;
        };
        match m.kind.as_str() {
            "quantizer" => {
                let v = self.param_named(module_idx, "Scale", 1.0);
                // Resolve index via control choices when present.
                let mut idx = v.round().max(0.0) as usize;
                for it in &m.items {
                    let Item::Param { control_index } = it else {
                        continue;
                    };
                    let Some(c) = self.controls.get(*control_index) else {
                        continue;
                    };
                    if c.name != "Scale" {
                        continue;
                    }
                    let choices = crate::param_fmt::choices_for(c, &self.module_types);
                    if !choices.is_empty() {
                        idx = v.round().clamp(0.0, (choices.len() - 1) as f32) as usize;
                    }
                    break;
                }
                GlyphDetail::Quantizer {
                    mask: crate::scale_ui::mask_for_index(idx),
                }
            }
            "comparator" => GlyphDetail::Comparator {
                thresh: self.param_named(module_idx, "Thresh", 0.0),
            },
            "logic" => GlyphDetail::Logic {
                mode: self.param_named(module_idx, "Mode", 0.0).round().clamp(0.0, 3.0) as u8,
            },
            "crossfade" => GlyphDetail::Crossfade {
                fade: self.param_named(module_idx, "Fade", 0.5).clamp(0.0, 1.0),
            },
            "edge" => GlyphDetail::Edge {
                width_ms: self.param_named(module_idx, "Width", 5.0),
            },
            "bernoulli" => GlyphDetail::Bernoulli {
                chance: self.param_named(module_idx, "Chance", 0.5).clamp(0.0, 1.0),
            },
            "euclidean" => GlyphDetail::Euclidean {
                steps: self.param_named(module_idx, "Steps", 16.0).round().clamp(1.0, 32.0) as u8,
                fill: self.param_named(module_idx, "Fill", 4.0).round().clamp(0.0, 32.0) as u8,
                rotate: self
                    .param_named(module_idx, "Rotate", 0.0)
                    .round()
                    .clamp(0.0, 31.0) as u8,
            },
            "random_trig" => GlyphDetail::RandomTrig {
                rate_hz: self.param_named(module_idx, "Rate", 4.0),
            },
            "switch" => GlyphDetail::Switch {
                n_steps: self.param_named(module_idx, "Steps", 4.0).round().clamp(2.0, 4.0) as u8,
            },
            "clip" => GlyphDetail::Clip {
                level: self.param_named(module_idx, "Level", 1.0),
            },
            "saturator" => GlyphDetail::Saturator {
                drive: self.param_named(module_idx, "Drive", 2.0),
                mix: self.param_named(module_idx, "Mix", 1.0),
            },
            "bitcrush" => GlyphDetail::Bitcrush {
                bits: self.param_named(module_idx, "Bits", 8.0),
                rate: self.param_named(module_idx, "Rate", 0.25),
            },
            "env_follow" => GlyphDetail::EnvFollow {
                rise_s: self.param_named(module_idx, "Rise", 0.01),
                fall_s: self.param_named(module_idx, "Fall", 0.1),
            },
            _ => GlyphDetail::None,
        }
    }

    pub fn param_indices_for_module(&self, module_idx: usize) -> Vec<usize> {
        let Some(m) = self.modules.get(module_idx) else {
            return Vec::new();
        };
        m.items
            .iter()
            .filter_map(|it| match it {
                Item::Param { control_index } => Some(*control_index),
                _ => None,
            })
            .collect()
    }

    pub fn sibling_param(&self, control_index: usize, delta: isize) -> Option<usize> {
        // Find which module owns this control and step within its params.
        let module_idx = self.modules.iter().position(|m| {
            m.items.iter().any(|it| {
                matches!(it, Item::Param { control_index: c } if *c == control_index)
            })
        })?;
        let params = self.param_indices_for_module(module_idx);
        if params.is_empty() {
            return None;
        }
        let cur = params.iter().position(|&c| c == control_index).unwrap_or(0);
        let n = params.len() as isize;
        let mut i = cur as isize + delta;
        i = ((i % n) + n) % n;
        Some(params[i as usize])
    }

    pub fn refresh_status(&mut self) {
        if let Some(pe) = &self.path_edit {
            self.status = format!(
                "{}  ·  type path  ·  Enter confirm  ·  Esc cancel",
                pe.title()
            );
            return;
        }

        if let Some(ci) = self.param_edit {
            if let Some(c) = self.controls.get(ci) {
                let shown = self.format_param(ci);
                self.status = format!(
                    "PARAM {}  ·  {shown}  ·  -/= adjust  _/+ fine  j/k sibling  Esc back",
                    c.name
                );
                return;
            }
        }

        if let Some(mv) = &self.moving {
            let title = self
                .modules
                .get(mv.module_idx)
                .map(|m| m.title.as_str())
                .unwrap_or("module");
            self.status = format!(
                "MOVE {title}  ·  @{},{}  ·  arrows drag  ·  Enter drop  ·  Esc cancel",
                self.cursor.x, self.cursor.y
            );
            return;
        }

        if let Some(menu) = self.menu_stack.last() {
            let sel = menu
                .selected_entry()
                .map(|e| e.label.as_str())
                .unwrap_or("—");
            if menu.filter.is_empty() {
                self.status = format!(
                    "MENU {}  ·  {}  ·  type to filter  ·  Enter  ·  Esc back",
                    menu.title, sel
                );
            } else {
                self.status = format!(
                    "MENU {}  ·  /{}  ·  {}  ·  Enter  ·  Bksp/Esc clear",
                    menu.title, menu.filter, sel
                );
            }
            return;
        }

        let cell = if let Some(idx) = self.module_at_cursor() {
            let m = &self.modules[idx];
            format!("{} ({})", m.title, m.kind)
        } else {
            format!("empty · {},{}", self.cursor.x, self.cursor.y)
        };

        let action = if let Some(p) = self.pending {
            format!(
                "ARMED {}.{} → Enter on destination module",
                self.module_short(p.src_module),
                self.port_name(p.src_module, p.src_port, true)
            )
        } else {
            HINTS[0].to_string()
        };

        self.status = format!("{cell}  ·  {action}");
    }
}
