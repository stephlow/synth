//! Keyboard handling and engine RPCs — mode-dispatched.

use crossterm::event::KeyCode;
use synth_proto::synth_client::SynthClient;
use synth_proto::{
    AddModuleRequest, ConnectRequest, DisconnectRequest, Empty, LoadPatchRequest,
    ModuleCoordinatesEntry, PatchCanvas, RemoveModuleRequest, SetCoordinatesRequest,
    SetIoDeviceRequest, SetOutputTapRequest, SetParamRequest,
};
use tracing::{debug, info, warn};

use crate::app::App;
use crate::menu_build;
use crate::model::{MenuAction, PathEdit, PendingCable};

pub async fn handle_key(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    code: KeyCode,
) {
    // Always quit — except while typing a path (so 'q' can appear in filenames).
    if matches!(code, KeyCode::Char('q')) && app.path_edit.is_none() {
        app.quit = true;
        return;
    }

    if app.path_edit.is_some() {
        handle_path_edit(app, client, code).await;
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
    _client: &mut SynthClient<tonic::transport::Channel>,
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
        _ => {}
    }
}

async fn handle_menu(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    code: KeyCode,
) {
    match code {
        KeyCode::Esc => {
            // Clear filter first so submenus keep easy re-search; then pop.
            if !app.menu_filter_clear() {
                app.pop_menu();
            }
        }
        KeyCode::Left => {
            app.pop_menu();
        }
        KeyCode::Backspace => {
            if !app.menu_filter_backspace() {
                app.pop_menu();
            }
        }
        KeyCode::Up => app.menu_selected_delta(-1),
        KeyCode::Down => app.menu_selected_delta(1),
        // Keep j/k as navigation (not filter letters).
        KeyCode::Char('k') => app.menu_selected_delta(-1),
        KeyCode::Char('j') => app.menu_selected_delta(1),
        KeyCode::Enter | KeyCode::Right => {
            activate_menu_item(app, client).await;
        }
        // `l` opens like Enter when not filtering; while typing, it filters.
        KeyCode::Char('l') => {
            let filtering = app
                .menu_stack
                .last()
                .is_some_and(|m| !m.filter.is_empty());
            if filtering {
                app.menu_filter_char('l');
            } else {
                activate_menu_item(app, client).await;
            }
        }
        // `h` pops when filter empty; otherwise filters (e.g. "highpass").
        KeyCode::Char('h') => {
            let filtering = app
                .menu_stack
                .last()
                .is_some_and(|m| !m.filter.is_empty());
            if filtering {
                app.menu_filter_char('h');
            } else {
                app.pop_menu();
            }
        }
        KeyCode::Char(ch) if !ch.is_control() => {
            app.menu_filter_char(ch);
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
        .and_then(|m| m.selected_entry())
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
                "armed {}.{} — move to a module (or Audio Out), Enter",
                app.module_short(module),
                name
            );
        }
        MenuAction::ConnectInput { module, port, name } => {
            connect_input(app, client, module, port, &name).await;
        }
        MenuAction::SetOutputTap { module, port, name } => {
            match client
                .set_output_tap(SetOutputTapRequest { module, port })
                .await
            {
                Ok(resp) => {
                    app.apply_snapshot(resp.into_inner());
                    app.pending = None;
                    app.clear_menus();
                    app.status = format!(
                        "main out ← {}.{}",
                        app.module_short(module),
                        name
                    );
                }
                Err(err) => app.status = format!("set out failed: {err}"),
            }
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
        MenuAction::NewPatch => {
            app.clear_menus();
            new_patch(app, client).await;
        }
        MenuAction::SavePatch => {
            app.begin_save_path();
        }
        MenuAction::LoadPatch => {
            app.begin_load_path();
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

async fn handle_path_edit(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    code: KeyCode,
) {
    match code {
        KeyCode::Esc => {
            app.cancel_path_edit();
            app.status = "path entry cancelled".into();
        }
        KeyCode::Enter => {
            let Some(pe) = app.path_edit.clone() else {
                return;
            };
            let path = pe.path().trim().to_string();
            if path.is_empty() {
                app.status = "path is empty".into();
                return;
            }
            match pe {
                PathEdit::Save { .. } => {
                    commit_save(app, client, &path).await;
                }
                PathEdit::Load { .. } => {
                    commit_load(app, client, &path).await;
                }
            }
        }
        KeyCode::Backspace => {
            if let Some(pe) = app.path_edit.as_mut() {
                pe.path_mut().pop();
                app.refresh_status();
            }
        }
        KeyCode::Char(ch) if !ch.is_control() => {
            if let Some(pe) = app.path_edit.as_mut() {
                pe.path_mut().push(ch);
                app.refresh_status();
            }
        }
        _ => {}
    }
}

async fn new_patch(app: &mut App, client: &mut SynthClient<tonic::transport::Channel>) {
    match client.new_patch(Empty {}).await {
        Ok(resp) => {
            // Drop remembered placements so default coords win cleanly.
            app.placement_memory.clear();
            app.pending = None;
            app.apply_snapshot(resp.into_inner());
            app.clear_menus();
            app.status = "new patch (default)".into();
            info!("new_patch applied");
        }
        Err(err) => app.status = format!("new failed: {err}"),
    }
}

async fn commit_save(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    path: &str,
) {
    // Ensure current lattice layout is in the document before export.
    flush_coordinates(app, client).await;
    match client.save_patch(Empty {}).await {
        Ok(resp) => {
            let json = resp.into_inner().document_json;
            if let Some(parent) = std::path::Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    if let Err(err) = std::fs::create_dir_all(parent) {
                        app.status = format!("mkdir failed: {err}");
                        return;
                    }
                }
            }
            match std::fs::write(path, &json) {
                Ok(()) => {
                    app.last_patch_path = path.to_string();
                    app.persist_settings();
                    app.path_edit = None;
                    app.status = format!("saved {path}");
                    info!(%path, "saved");
                }
                Err(err) => app.status = format!("write failed: {err}"),
            }
        }
        Err(err) => app.status = format!("save failed: {err}"),
    }
}

async fn commit_load(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    path: &str,
) {
    match std::fs::read_to_string(path) {
        Ok(json) => match client
            .load_patch(LoadPatchRequest {
                document_json: json,
            })
            .await
        {
            Ok(resp) => {
                app.placement_memory.clear();
                app.pending = None;
                app.apply_snapshot(resp.into_inner());
                app.last_patch_path = path.to_string();
                app.persist_settings();
                app.path_edit = None;
                app.clear_menus();
                app.status = format!("loaded {path}");
                info!(%path, "loaded");
            }
            Err(err) => app.status = format!("load failed: {err}"),
        },
        Err(err) => app.status = format!("read {path}: {err}"),
    }
}


