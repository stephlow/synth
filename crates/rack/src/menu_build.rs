//! Build context-sensitive command menus from App state.

use crate::app::App;
use crate::model::{
    Item, MenuAction, MenuEntry, MenuState, ModuleId,
};

/// Root menu for the cell under the cursor.
pub fn root_menu(app: &App) -> MenuState {
    if let Some(idx) = app.module_at_cursor() {
        // Pending cable: jump straight to inputs if this module has any.
        if app.pending.is_some() {
            if let Some(m) = app.modules.get(idx) {
                if matches!(m.id, ModuleId::Voice(_))
                    && m.items.iter().any(|it| matches!(it, Item::Input { .. }))
                {
                    return inputs_menu(app, idx);
                }
            }
        }
        return module_root_menu(app, idx);
    }
    empty_cell_menu()
}

fn empty_cell_menu() -> MenuState {
    MenuState {
        title: "empty".into(),
        selected: 0,
        items: vec![
            MenuEntry::submenu("Add module…", MenuAction::OpenAddKinds),
            MenuEntry::action("Save patch", MenuAction::SavePatch),
            MenuEntry::action("Load patch", MenuAction::LoadPatch),
        ],
    }
}

fn append_rack_actions(items: &mut Vec<MenuEntry>) {
    items.push(MenuEntry::action("Save patch", MenuAction::SavePatch));
    items.push(MenuEntry::action("Load patch", MenuAction::LoadPatch));
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
        ModuleId::Keyboard => {
            items.push(MenuEntry::submenu(
                "Notes…",
                MenuAction::OpenNotes { module_idx },
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
        ModuleId::Scope | ModuleId::Meter => {
            items.push(MenuEntry::submenu(
                "Readout",
                MenuAction::OpenInfo { module_idx },
            ));
            items.push(MenuEntry::action(
                "Move…",
                MenuAction::StartMove { module_idx },
            ));
        }
    }

    if items.is_empty() {
        items.push(MenuEntry::value("(no actions)"));
    }
    append_rack_actions(&mut items);

    MenuState {
        title: m.title.clone(),
        selected: 0,
        items,
    }
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
            let v = app.values.get(ci).copied().unwrap_or(0.0);
            items.push(MenuEntry::action(
                format!("{}  {v:.3}", c.name),
                MenuAction::EditParam { control_index: ci },
            ));
        }
    }
    if items.is_empty() {
        items.push(MenuEntry::value("(no parameters)"));
    }
    MenuState {
        title,
        selected: 0,
        items,
    }
}

pub fn outputs_menu(app: &App, module_idx: usize) -> MenuState {
    let m = &app.modules[module_idx];
    let voice = match &m.id {
        ModuleId::Voice(id) => *id,
        _ => 0,
    };
    let mut items = Vec::new();
    for it in &m.items {
        if let Item::Output { port, name } = it {
            let peer = app.jack_peer_hint(voice, *port, true);
            let label = if peer.is_empty() {
                format!("{name}  (open)")
            } else {
                format!("{name}  {peer}")
            };
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
    if items.is_empty() {
        items.push(MenuEntry::value("(no outputs)"));
    }
    MenuState {
        title: format!("{} · out", m.title),
        selected: 0,
        items,
    }
}

pub fn inputs_menu(app: &App, module_idx: usize) -> MenuState {
    let m = &app.modules[module_idx];
    let voice = match &m.id {
        ModuleId::Voice(id) => *id,
        _ => 0,
    };
    let mut items = Vec::new();
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
    MenuState {
        title,
        selected: 0,
        items,
    }
}

pub fn disconnect_menu(app: &App, module_idx: usize) -> MenuState {
    let m = &app.modules[module_idx];
    let voice = match &m.id {
        ModuleId::Voice(id) => *id,
        _ => {
            return MenuState {
                title: "disconnect".into(),
                selected: 0,
                items: vec![MenuEntry::value("(not a voice)")],
            };
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
    MenuState {
        title: format!("{} · unplug", m.title),
        selected: 0,
        items,
    }
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
    MenuState {
        title: "add module".into(),
        selected: 0,
        items,
    }
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
    MenuState {
        title: format!("device · {endpoint_id}"),
        selected: 0,
        items,
    }
}

pub fn notes_menu(app: &App, module_idx: usize) -> MenuState {
    let mut items = Vec::new();
    if let Some(m) = app.modules.get(module_idx) {
        for it in &m.items {
            if let Item::Note { midi, label } = it {
                let held = app.held_notes.contains(midi);
                let mark = if held { "●" } else { "○" };
                items.push(MenuEntry::action(
                    format!("{mark}  {label}  ({midi})"),
                    MenuAction::ToggleNote { midi: *midi },
                ));
            }
        }
    }
    if items.is_empty() {
        items.push(MenuEntry::value("(no notes)"));
    }
    MenuState {
        title: "notes".into(),
        selected: 0,
        items,
    }
}

pub fn info_menu(app: &App, module_idx: usize) -> MenuState {
    let mut items = Vec::new();
    let Some(m) = app.modules.get(module_idx) else {
        return MenuState {
            title: "info".into(),
            selected: 0,
            items: vec![MenuEntry::value("—")],
        };
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
        ModuleId::Scope => {
            items.push(MenuEntry::value(format!(
                "scope samples {}",
                app.scope.len()
            )));
            items.push(MenuEntry::value(format!("peak   {:.3}", app.output_peak)));
        }
        ModuleId::Meter => {
            items.push(MenuEntry::value(format!("peak   {:.3}", app.output_peak)));
            items.push(MenuEntry::value(format!("voices {}", app.active_voices)));
        }
        ModuleId::Keyboard => {
            items.push(MenuEntry::value(format!(
                "held   {}",
                app.held_notes.len()
            )));
        }
    }
    MenuState {
        title: format!("{} · info", m.title),
        selected: 0,
        items,
    }
}
