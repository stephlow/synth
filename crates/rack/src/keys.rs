//! Keyboard handling and engine RPCs — mode-dispatched.

use crossterm::event::KeyCode;
use synth_proto::synth_client::SynthClient;
use synth_proto::{
    AddModuleRequest, ConnectRequest, DisconnectRequest, Empty, LoadPatchRequest,
    ModuleCoordinatesEntry, NoteOffRequest, NoteOnRequest, PatchCanvas, RemoveModuleRequest,
    SetCoordinatesRequest, SetIoDeviceRequest, SetParamRequest,
};
use tracing::{debug, info, warn};

use crate::app::App;
use crate::menu_build;
use crate::model::{MenuAction, ModuleId, PendingCable, DEFAULT_PATCH_FILE, NOTE_KEYS};

pub async fn handle_key(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    code: KeyCode,
) {
    // Always quit.
    if matches!(code, KeyCode::Char('q')) {
        app.quit = true;
        return;
    }

    if app.moving.is_some() {
        handle_move(app, client, code).await;
        return;
    }

    if app.param_edit.is_some() {
        handle_param_edit(app, client, code).await;
        return;
    }

    if !app.menu_stack.is_empty() {
        handle_menu(app, client, code).await;
        return;
    }

    handle_lattice(app, client, code).await;
}

async fn handle_move(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    code: KeyCode,
) {
    match code {
        KeyCode::Esc => app.cancel_move(),
        KeyCode::Enter => {
            app.confirm_move();
            flush_coordinates(app, client).await;
        }
        KeyCode::Left | KeyCode::Char('h') => app.move_cursor(-1, 0),
        KeyCode::Right | KeyCode::Char('l') => app.move_cursor(1, 0),
        KeyCode::Up | KeyCode::Char('k') => app.move_cursor(0, -1),
        KeyCode::Down | KeyCode::Char('j') => app.move_cursor(0, 1),
        _ => {}
    }
}

/// Persist voice lattice coordinates + canvas size into the patch document.
async fn flush_coordinates(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
) {
    let modules: Vec<ModuleCoordinatesEntry> = app
        .voice_coordinate_entries()
        .into_iter()
        .map(|(module_id, x, y)| ModuleCoordinatesEntry { module_id, x, y })
        .collect();
    let canvas = Some(PatchCanvas {
        width: u32::from(app.grid_cols),
        height: u32::from(app.grid_rows),
    });
    app.patch_canvas = Some((app.grid_cols, app.grid_rows));
    match client
        .set_coordinates(SetCoordinatesRequest { modules, canvas })
        .await
    {
        Ok(_) => {
            // Document-only; generation is not bumped — keep local placements.
            debug!("coordinates flushed to engine");
        }
        Err(err) => {
            warn!(%err, "failed to flush coordinates");
            app.status = format!("coords save failed: {err}");
        }
    }
}

async fn handle_lattice(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    code: KeyCode,
) {
    match code {
        KeyCode::Esc => {
            app.pending = None;
            app.status = "pending cable cleared".into();
        }
        // Navigation only — actions go through Enter → menu.
        KeyCode::Left | KeyCode::Char('h') => app.move_cursor(-1, 0),
        KeyCode::Right | KeyCode::Char('l') => app.move_cursor(1, 0),
        KeyCode::Up | KeyCode::Char('k') => app.move_cursor(0, -1),
        KeyCode::Down | KeyCode::Char('j') => app.move_cursor(0, 1),
        KeyCode::Enter => {
            let menu = menu_build::root_menu(app);
            app.push_menu(menu);
        }
        // Performance keys (not menu actions) — live notes while patching.
        KeyCode::Char(ch) => {
            if let Some(&(_, note)) = NOTE_KEYS.iter().find(|(k, _)| *k == ch) {
                toggle_note(app, client, note).await;
            }
        }
        _ => {}
    }
}

async fn handle_menu(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    code: KeyCode,
) {
    match code {
        KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => {
            app.pop_menu();
        }
        KeyCode::Up | KeyCode::Char('k') => app.menu_selected_delta(-1),
        KeyCode::Down | KeyCode::Char('j') => app.menu_selected_delta(1),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
            activate_menu_item(app, client).await;
        }
        _ => {}
    }
}

async fn handle_param_edit(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    code: KeyCode,
) {
    let Some(ci) = app.param_edit else {
        return;
    };
    match code {
        KeyCode::Esc => {
            app.param_edit = None;
            app.refresh_status();
        }
        KeyCode::Enter => {
            app.param_edit = None;
            app.refresh_status();
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            let fine = matches!(code, KeyCode::Char('_'));
            if let Some((module, param, value)) = app.adjust_param(ci, -1.0, fine) {
                let _ = client
                    .set_param(SetParamRequest {
                        module,
                        param,
                        value,
                    })
                    .await;
                app.refresh_status();
            }
        }
        KeyCode::Char('=') | KeyCode::Char('+') => {
            let fine = matches!(code, KeyCode::Char('+'));
            if let Some((module, param, value)) = app.adjust_param(ci, 1.0, fine) {
                let _ = client
                    .set_param(SetParamRequest {
                        module,
                        param,
                        value,
                    })
                    .await;
                app.refresh_status();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(next) = app.sibling_param(ci, -1) {
                app.param_edit = Some(next);
                app.refresh_status();
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(next) = app.sibling_param(ci, 1) {
                app.param_edit = Some(next);
                app.refresh_status();
            }
        }
        _ => {}
    }
}

async fn activate_menu_item(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
) {
    let action = app
        .menu_stack
        .last()
        .and_then(|m| m.items.get(m.selected))
        .and_then(|e| e.action.clone());

    let Some(action) = action else {
        // Value-only row — ignore.
        return;
    };

    match action {
        MenuAction::OpenParams { module_idx } => {
            app.push_menu(menu_build::params_menu(app, module_idx));
        }
        MenuAction::OpenOutputs { module_idx } => {
            app.push_menu(menu_build::outputs_menu(app, module_idx));
        }
        MenuAction::OpenInputs { module_idx } => {
            app.push_menu(menu_build::inputs_menu(app, module_idx));
        }
        MenuAction::OpenDisconnect { module_idx } => {
            app.push_menu(menu_build::disconnect_menu(app, module_idx));
        }
        MenuAction::OpenAddKinds => {
            app.push_menu(menu_build::add_kinds_menu(app));
        }
        MenuAction::OpenThemes => {
            app.push_menu(menu_build::themes_menu(app));
        }
        MenuAction::SetTheme { theme_index } => {
            use crate::draw::style::ThemeId;
            if let Some(id) = ThemeId::ALL.get(theme_index).copied() {
                app.set_theme(id);
                // Keep committed theme; clear menus without wiping theme_id.
                app.menu_stack.clear();
                app.param_edit = None;
                app.theme_preview = None;
                app.refresh_status();
                app.status = format!("theme → {}", id.label());
            }
        }
        MenuAction::OpenDevices { endpoint_id } => {
            app.push_menu(menu_build::devices_menu(app, &endpoint_id));
        }
        MenuAction::OpenNotes { module_idx } => {
            app.push_menu(menu_build::notes_menu(app, module_idx));
        }
        MenuAction::OpenInfo { module_idx } => {
            app.push_menu(menu_build::info_menu(app, module_idx));
        }
        MenuAction::EditParam { control_index } => {
            app.param_edit = Some(control_index);
            app.refresh_status();
        }
        MenuAction::ArmOutput { module, port, name } => {
            app.pending = Some(PendingCable {
                src_module: module,
                src_port: port,
            });
            app.clear_menus();
            app.status = format!(
                "armed {}.{} — target a module, Enter → Inputs…",
                app.module_short(module),
                name
            );
        }
        MenuAction::ConnectInput { module, port, name } => {
            connect_input(app, client, module, port, &name).await;
        }
        MenuAction::DisconnectCable {
            cable_id,
            src_module,
            src_port,
            dst_module,
            dst_port,
        } => {
            match client
                .patch_disconnect(DisconnectRequest {
                    cable_id,
                    src_module,
                    src_port,
                    dst_module,
                    dst_port,
                    use_cable_id: true,
                })
                .await
            {
                Ok(resp) => {
                    app.apply_snapshot(resp.into_inner());
                    app.clear_menus();
                    app.status = "unplugged cable".into();
                }
                Err(err) => {
                    warn!(%err, "disconnect failed");
                    app.status = format!("disconnect failed: {err}");
                }
            }
        }
        MenuAction::AddModule { kind, display_name } => {
            let prefer = app.cursor;
            match client
                .add_module(AddModuleRequest {
                    kind: kind.clone(),
                })
                .await
            {
                Ok(resp) => {
                    let snap = resp.into_inner();
                    let new_id = snap
                        .modules
                        .iter()
                        .filter(|m| m.kind == kind)
                        .map(|m| m.id)
                        .max();
                    app.apply_snapshot(snap);
                    app.clear_menus();
                    if let Some(id) = new_id {
                        app.place_new_voice_at(id, prefer);
                        flush_coordinates(app, client).await;
                    }
                    app.status = format!("added {display_name}");
                }
                Err(err) => app.status = format!("add failed: {err}"),
            }
        }
        MenuAction::StartMove { module_idx } => {
            if !app.start_move(module_idx) {
                app.status = "cannot move this module".into();
            }
        }
        MenuAction::SavePatch => {
            app.clear_menus();
            save_patch(app, client).await;
        }
        MenuAction::LoadPatch => {
            app.clear_menus();
            load_patch(app, client).await;
        }
        MenuAction::DeleteModule { module_id } => {
            match client
                .remove_module(RemoveModuleRequest { module_id })
                .await
            {
                Ok(resp) => {
                    app.apply_snapshot(resp.into_inner());
                    app.clear_menus();
                    app.status = format!("removed module {module_id}");
                }
                Err(err) => app.status = format!("remove failed: {err}"),
            }
        }
        MenuAction::SetDevice {
            endpoint_id,
            device_name,
        } => {
            match client
                .set_io_device(SetIoDeviceRequest {
                    endpoint_id: endpoint_id.clone(),
                    device_name: device_name.clone(),
                })
                .await
            {
                Ok(_) => {
                    if let Ok(resp) = client.list_io_endpoints(Empty {}).await {
                        app.io_endpoints = resp.into_inner().endpoints;
                    }
                    if let Ok(resp) = client.list_audio_devices(Empty {}).await {
                        app.audio_devices = resp.into_inner().devices;
                    }
                    app.rebuild_ui_modules();
                    app.rebind_placements(None);
                    app.clear_menus();
                    app.status = format!("device → {device_name}");
                }
                Err(err) => app.status = format!("device failed: {err}"),
            }
        }
        MenuAction::ToggleNote { midi } => {
            toggle_note(app, client, midi).await;
            // Refresh notes menu in place if still open.
            if let Some(top) = app.menu_stack.last() {
                if top.title == "notes" {
                    // Rebuild notes for keyboard module under cursor or any keyboard.
                    let kbd = app
                        .modules
                        .iter()
                        .position(|m| matches!(m.id, ModuleId::Keyboard));
                    if let Some(idx) = kbd {
                        let sel = top.selected;
                        let mut menu = menu_build::notes_menu(app, idx);
                        menu.selected = sel.min(menu.items.len().saturating_sub(1));
                        if let Some(slot) = app.menu_stack.last_mut() {
                            *slot = menu;
                        }
                    }
                }
            }
            app.refresh_status();
        }
    }
}

async fn connect_input(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    dst: u32,
    port: u32,
    name: &str,
) {
    let Some(pending) = app.pending else {
        app.status = "arm an output first (Outputs… → pick jack)".into();
        return;
    };
    match client
        .patch_connect(ConnectRequest {
            src_module: pending.src_module,
            src_port: pending.src_port,
            dst_module: dst,
            dst_port: port,
        })
        .await
    {
        Ok(resp) => {
            app.pending = None;
            app.apply_snapshot(resp.into_inner());
            app.clear_menus();
            app.status = format!("connected → {name}");
        }
        Err(err) => {
            warn!(%err, "connect failed");
            app.status = format!("connect failed: {err}");
        }
    }
}

async fn save_patch(app: &mut App, client: &mut SynthClient<tonic::transport::Channel>) {
    // Ensure current lattice layout is in the document before export.
    flush_coordinates(app, client).await;
    match client.save_patch(Empty {}).await {
        Ok(resp) => {
            let json = resp.into_inner().document_json;
            match std::fs::write(DEFAULT_PATCH_FILE, &json) {
                Ok(()) => {
                    app.status = format!("saved {DEFAULT_PATCH_FILE}");
                    info!(path = DEFAULT_PATCH_FILE, "saved");
                }
                Err(err) => app.status = format!("write failed: {err}"),
            }
        }
        Err(err) => app.status = format!("save failed: {err}"),
    }
}

async fn load_patch(app: &mut App, client: &mut SynthClient<tonic::transport::Channel>) {
    match std::fs::read_to_string(DEFAULT_PATCH_FILE) {
        Ok(json) => match client
            .load_patch(LoadPatchRequest {
                document_json: json,
            })
            .await
        {
            Ok(resp) => {
                app.apply_snapshot(resp.into_inner());
                app.clear_menus();
                app.status = format!("loaded {DEFAULT_PATCH_FILE}");
            }
            Err(err) => app.status = format!("load failed: {err}"),
        },
        Err(err) => app.status = format!("read {DEFAULT_PATCH_FILE}: {err}"),
    }
}

async fn toggle_note(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    note: u8,
) {
    if app.held_notes.remove(&note) {
        let _ = client
            .note_off(NoteOffRequest { note: note as u32 })
            .await;
        debug!(note, "note_off");
    } else {
        app.held_notes.insert(note);
        let _ = client
            .note_on(NoteOnRequest {
                note: note as u32,
                velocity: 100,
            })
            .await;
        debug!(note, "note_on");
    }
    app.refresh_status();
}
