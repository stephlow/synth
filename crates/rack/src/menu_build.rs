//! Build context-sensitive command menus from App state.

use crate::app::App;
use crate::model::{Item, MenuAction, MenuEntry, MenuState, ModuleId};

/// Root menu for the cell under the cursor.
pub fn root_menu(app: &App) -> MenuState {
    if let Some(idx) = app.module_at_cursor() {
        // Armed source: jump straight to destinations (graph inputs or main out).
        if app.pending.is_some() {
            if app.is_master_audio_out(idx) {
                return inputs_menu(app, idx);
            }
            if let Some(m) = app.modules.get(idx) {
                let has_in = m.items.iter().any(|it| matches!(it, Item::Input { .. }));
                if has_in {
                    return inputs_menu(app, idx);
                }
            }
        }
        return module_root_menu(app, idx);
    }
    empty_cell_menu()
}

fn empty_cell_menu() -> MenuState {
    MenuState::new(
        "empty",
        vec![
            MenuEntry::submenu("Add module…", MenuAction::OpenAddKinds),
            MenuEntry::submenu("Theme…", MenuAction::OpenThemes),
            MenuEntry::action("New patch", MenuAction::NewPatch),
            MenuEntry::action("Save patch…", MenuAction::SavePatch),
            MenuEntry::action("Load patch…", MenuAction::LoadPatch),
        ],
    )
}

fn append_rack_actions(items: &mut Vec<MenuEntry>) {
    items.push(MenuEntry::submenu("Theme…", MenuAction::OpenThemes));
    items.push(MenuEntry::action("New patch", MenuAction::NewPatch));
    items.push(MenuEntry::action("Save patch…", MenuAction::SavePatch));
    items.push(MenuEntry::action("Load patch…", MenuAction::LoadPatch));
}

/// Built-in theme presets.
pub fn themes_menu(app: &App) -> MenuState {
    use crate::draw::style::ThemeId;
    let mut items = Vec::new();
    for (i, id) in ThemeId::ALL.iter().enumerate() {
        let mark = if *id == app.theme_id { "● " } else { "  " };
        items.push(MenuEntry::action(
            format!("{mark}{}", id.label()),
            MenuAction::SetTheme { theme_index: i },
        ));
    }
    let mut menu = MenuState::new("theme", items);
    menu.selected = ThemeId::ALL
        .iter()
        .position(|t| *t == app.theme_id)
        .unwrap_or(0);
    menu
}

fn module_root_menu(app: &App, module_idx: usize) -> MenuState {
    let m = &app.modules[module_idx];
    let mut items = Vec::new();

    match &m.id {
        ModuleId::Voice(id) => {
            let has_params = m.items.iter().any(|it| matches!(it, Item::Param { .. }));
            let has_out = m.items.iter().any(|it| matches!(it, Item::Output { .. }));
            let has_in = m.items.iter().any(|it| matches!(it, Item::Input { .. }));
            if has_params {
                items.push(MenuEntry::submenu(
                    "Parameters…",
                    MenuAction::OpenParams { module_idx },
                ));
            }
            if has_out {
                items.push(MenuEntry::submenu(
                    "Outputs…",
                    MenuAction::OpenOutputs { module_idx },
                ));
            }
            if has_in {
                items.push(MenuEntry::submenu(
                    "Inputs…",
                    MenuAction::OpenInputs { module_idx },
                ));
            }
            if app.module_has_cables(*id) {
                items.push(MenuEntry::submenu(
                    "Disconnect…",
                    MenuAction::OpenDisconnect { module_idx },
                ));
            }
            items.push(MenuEntry::action(
                "Move…",
                MenuAction::StartMove { module_idx },
            ));
            items.push(MenuEntry::action(
                "Delete module",
                MenuAction::DeleteModule { module_id: *id },
            ));
            items.push(MenuEntry::submenu(
                "Info",
                MenuAction::OpenInfo { module_idx },
            ));
        }
        ModuleId::Io(endpoint_id) => {
            let has_params = m.items.iter().any(|it| matches!(it, Item::Param { .. }));
            let has_in = m.items.iter().any(|it| matches!(it, Item::Input { .. }));
            let has_out = m.items.iter().any(|it| matches!(it, Item::Output { .. }));
            let master_out = app.is_master_audio_out(module_idx);
            // When a source is armed, assign/main-bus comes first (not Device/params).
            if master_out || has_in {
                let label = if app.pending.is_some() {
                    if master_out {
                        "Assign main out…"
                    } else {
                        "Assign input…"
                    }
                } else if master_out {
                    "Main bus…"
                } else {
                    "Inputs…"
                };
                items.push(MenuEntry::submenu(
                    label,
                    MenuAction::OpenInputs { module_idx },
                ));
            }
            if has_out {
                items.push(MenuEntry::submenu(
                    "Outputs…",
                    MenuAction::OpenOutputs { module_idx },
                ));
            }
            if has_params {
                items.push(MenuEntry::submenu(
                    "Parameters…",
                    MenuAction::OpenParams { module_idx },
                ));
            }
            items.push(MenuEntry::submenu(
                "Device…",
                MenuAction::OpenDevices {
                    endpoint_id: endpoint_id.clone(),
                },
            ));
            items.push(MenuEntry::action(
                "Move…",
                MenuAction::StartMove { module_idx },
            ));
            items.push(MenuEntry::submenu(
                "Info",
                MenuAction::OpenInfo { module_idx },
            ));
        }
    }

    if items.is_empty() {
        items.push(MenuEntry::value("(no actions)"));
    }
    append_rack_actions(&mut items);

    MenuState::new(m.title.clone(), items)
}

pub fn params_menu(app: &App, module_idx: usize) -> MenuState {
    let title = app
        .modules
        .get(module_idx)
        .map(|m| format!("{} · params", m.title))
        .unwrap_or_else(|| "params".into());
    let mut items = Vec::new();
    for ci in app.param_indices_for_module(module_idx) {
        if let Some(c) = app.controls.get(ci) {
            let shown = app.format_param(ci);
            items.push(MenuEntry::action(
                format!("{}  {shown}", c.name),
                MenuAction::EditParam { control_index: ci },
            ));
        }
    }
    if items.is_empty() {
        items.push(MenuEntry::value("(no parameters)"));
    }
    MenuState::new(title, items)
}

pub fn outputs_menu(app: &App, module_idx: usize) -> MenuState {
    let m = &app.modules[module_idx];
    let voice = match &m.id {
        ModuleId::Voice(id) => Some(*id),
        ModuleId::Io(epid) => {
            let ep_name = app
                .io_endpoints
                .iter()
                .find(|e| e.id == *epid)
                .map(|e| e.name.as_str());
            app.voice_modules
                .iter()
                .find(|vm| {
                    vm.kind == "io"
                        && ep_name.is_some_and(|n| vm.name == n)
                })
                .or_else(|| app.voice_modules.iter().find(|vm| vm.kind == "io"))
                .map(|vm| vm.id)
        }
    };
    let mut items = Vec::new();
    if let Some(voice) = voice {
        for it in &m.items {
            if let Item::Output { port, name } = it {
                let peer = app.jack_peer_hint(voice, *port, true);
                let tap = app.roles.output_tap_module == voice
                    && app.roles.output_tap_port == *port;
                let mut label = name.clone();
                if tap {
                    label.push_str("  ◆main");
                }
                if !peer.is_empty() {
                    label.push_str("  ");
                    label.push_str(&peer);
                }
                // Arm only — destination is a lattice module (or Audio Out main bus).
                items.push(MenuEntry::action(
                    label,
                    MenuAction::ArmOutput {
                        module: voice,
                        port: *port,
                        name: name.clone(),
                    },
                ));
            }
        }
    }
    if items.is_empty() {
        items.push(MenuEntry::value("(no outputs)"));
    }
    MenuState::new(format!("{} · out", m.title), items)
}

pub fn inputs_menu(app: &App, module_idx: usize) -> MenuState {
    let m = &app.modules[module_idx];
    let mut items = Vec::new();

    // Master Audio Out: main bus is a role (SetOutputTap), not a graph cable.
    if app.is_master_audio_out(module_idx) {
        let current = app.main_bus_source_label();
        if let Some(p) = app.pending {
            let src_name = app.port_name(p.src_module, p.src_port, true);
            let src = format!("{}.{}", app.module_short(p.src_module), src_name);
            items.push(MenuEntry::action(
                format!("Audio L/R  ←  {src}"),
                MenuAction::SetOutputTap {
                    module: p.src_module,
                    port: p.src_port,
                    name: src_name,
                },
            ));
            items.push(MenuEntry::value(
                "(dual mono — same bus on L and R)",
            ));
            items.push(MenuEntry::value(format!("current  {current}")));
        } else {
            items.push(MenuEntry::value(format!("Audio L/R  ←  {current}")));
            items.push(MenuEntry::value(
                "change: Outputs… → arm jack → Enter here",
            ));
            items.push(MenuEntry::value("Pitch CV / Gate  ←  master MIDI"));
        }
        return MenuState::new(format!("{} · main bus", m.title), items);
    }

    let voice = match &m.id {
        ModuleId::Voice(id) => *id,
        _ => 0,
    };
    for it in &m.items {
        if let Item::Input { port, name } = it {
            let peer = app.jack_peer_hint(voice, *port, false);
            let label = if peer.is_empty() {
                format!("{name}  (open)")
            } else {
                format!("{name}  {peer}")
            };
            items.push(MenuEntry::action(
                label,
                MenuAction::ConnectInput {
                    module: voice,
                    port: *port,
                    name: name.clone(),
                },
            ));
        }
    }
    if items.is_empty() {
        items.push(MenuEntry::value("(no inputs)"));
    }
    let title = if app.pending.is_some() {
        format!("{} · connect in", m.title)
    } else {
        format!("{} · in", m.title)
    };
    MenuState::new(title, items)
}



pub fn disconnect_menu(app: &App, module_idx: usize) -> MenuState {
    let m = &app.modules[module_idx];
    let voice = match &m.id {
        ModuleId::Voice(id) => *id,
        _ => {
            return MenuState::new(
                "disconnect",
                vec![MenuEntry::value("(not a voice)")],
            );
        }
    };
    let mut items = Vec::new();
    for c in app.cables_on_module(voice) {
        let label = format!(
            "{}.{} → {}.{}",
            app.module_short(c.src_module),
            app.port_name(c.src_module, c.src_port, true),
            app.module_short(c.dst_module),
            app.port_name(c.dst_module, c.dst_port, false),
        );
        items.push(MenuEntry::action(
            label,
            MenuAction::DisconnectCable {
                cable_id: c.id,
                src_module: c.src_module,
                src_port: c.src_port,
                dst_module: c.dst_module,
                dst_port: c.dst_port,
            },
        ));
    }
    if items.is_empty() {
        items.push(MenuEntry::value("(no cables)"));
    }
    MenuState::new(format!("{} · unplug", m.title), items)
}

pub fn add_kinds_menu(app: &App) -> MenuState {
    let mut items = Vec::new();
    for t in &app.module_types {
        items.push(MenuEntry::action(
            format!("{}  ({})", t.display_name, t.kind),
            MenuAction::AddModule {
                kind: t.kind.clone(),
                display_name: t.display_name.clone(),
            },
        ));
    }
    if items.is_empty() {
        items.push(MenuEntry::value("(no types)"));
    }
    MenuState::new("add module", items)
}

pub fn devices_menu(app: &App, endpoint_id: &str) -> MenuState {
    let mut items = Vec::new();
    let devices = app.devices_for_endpoint(endpoint_id);
    if devices.is_empty() {
        // Fallback: cycle action via current next device
        if let Some(name) = app.next_device_for(endpoint_id) {
            items.push(MenuEntry::action(
                format!("cycle → {name}"),
                MenuAction::SetDevice {
                    endpoint_id: endpoint_id.to_string(),
                    device_name: name,
                },
            ));
        } else {
            items.push(MenuEntry::value("(no devices)"));
        }
    } else {
        for name in devices {
            items.push(MenuEntry::action(
                name.clone(),
                MenuAction::SetDevice {
                    endpoint_id: endpoint_id.to_string(),
                    device_name: name,
                },
            ));
        }
    }
    MenuState::new(format!("device · {endpoint_id}"), items)
}

pub fn info_menu(app: &App, module_idx: usize) -> MenuState {
    let mut items = Vec::new();
    let Some(m) = app.modules.get(module_idx) else {
        return MenuState::new("info", vec![MenuEntry::value("—")]);
    };
    items.push(MenuEntry::value(format!("title  {}", m.title)));
    items.push(MenuEntry::value(format!("kind   {}", m.kind)));
    match &m.id {
        ModuleId::Voice(id) => {
            items.push(MenuEntry::value(format!("id     {id}")));
            let n = app.cables_on_module(*id).len();
            items.push(MenuEntry::value(format!("cables {n}")));
        }
        ModuleId::Io(epid) => {
            items.push(MenuEntry::value(format!("endpoint {epid}")));
            if let Some(meter) = app.io_meters.iter().find(|x| x.endpoint_id == *epid) {
                items.push(MenuEntry::value(format!(
                    "device  {}  conn={}",
                    meter.device_name, meter.connected
                )));
            }
        }
    }
    MenuState::new(format!("{} · info", m.title), items)
}
