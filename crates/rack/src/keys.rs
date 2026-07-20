//! Keyboard handling and engine RPCs.

use crossterm::event::KeyCode;
use synth_proto::synth_client::SynthClient;
use synth_proto::{
    AddModuleRequest, ConnectRequest, DisconnectRequest, Empty, LoadPatchRequest, NoteOffRequest,
    NoteOnRequest, RemoveModuleRequest, SetIoDeviceRequest, SetParamRequest,
};
use tracing::{debug, info, trace, warn};

use crate::app::App;
use crate::model::{Item, ModuleId, PendingCable, DEFAULT_PATCH_FILE, NOTE_KEYS};

pub async fn handle_key(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
    code: KeyCode,
) {
    trace!(?code, "key");

    match code {
        KeyCode::Char('q') => {
            app.quit = true;
        }
        KeyCode::Esc => {
            app.pending = None;
            app.status = "pending cable cleared".into();
        }
        KeyCode::Char('?') => app.cycle_hint(),
        KeyCode::Char('h') | KeyCode::Left => app.focus_module_delta(-1),
        KeyCode::Char('l') | KeyCode::Right => app.focus_module_delta(1),
        KeyCode::Char('k') | KeyCode::Up => app.focus_item_delta(-1),
        KeyCode::Char('j') | KeyCode::Down => app.focus_item_delta(1),
        KeyCode::PageDown | KeyCode::Char(']') => app.page_delta(1),
        KeyCode::PageUp | KeyCode::Char('[') => app.page_delta(-1),
        KeyCode::Char('S') => save_patch(app, client).await,
        KeyCode::Char('L') => load_patch(app, client).await,
        KeyCode::Char('D') => delete_focused_voice(app, client).await,
        KeyCode::Char('-') | KeyCode::Char('_') => {
            let fine = matches!(code, KeyCode::Char('_'));
            if let Some(Item::Param { control_index }) = app.focused_item().cloned() {
                if let Some((module, param, value)) = app.adjust_param(control_index, -1.0, fine) {
                    let _ = client
                        .set_param(SetParamRequest {
                            module,
                            param,
                            value,
                        })
                        .await;
                    app.refresh_status();
                }
            } else {
                app.status = "focus a param to adjust".into();
            }
        }
        KeyCode::Char('=') | KeyCode::Char('+') => {
            let fine = matches!(code, KeyCode::Char('+'));
            if let Some(Item::Param { control_index }) = app.focused_item().cloned() {
                if let Some((module, param, value)) = app.adjust_param(control_index, 1.0, fine) {
                    let _ = client
                        .set_param(SetParamRequest {
                            module,
                            param,
                            value,
                        })
                        .await;
                    app.refresh_status();
                }
            } else {
                app.status = "focus a param to adjust".into();
            }
        }
        KeyCode::Enter => on_enter(app, client).await,
        KeyCode::Char('x') => disconnect_focused_jack(app, client).await,
        KeyCode::Char(ch) => {
            if let Some(&(_, note)) = NOTE_KEYS.iter().find(|(k, _)| *k == ch) {
                toggle_note(app, client, note).await;
            }
        }
        _ => {}
    }
}

async fn on_enter(app: &mut App, client: &mut SynthClient<tonic::transport::Channel>) {
    let item = app.focused_item().cloned();
    let voice_id = app.focused_voice_id();

    match item {
        Some(Item::Output { port, name }) => {
            if let Some(mid) = voice_id {
                app.pending = Some(PendingCable {
                    src_module: mid,
                    src_port: port,
                });
                app.status = format!(
                    "armed {}.{} — focus an input jack, Enter",
                    app.module_short(mid),
                    name
                );
            }
        }
        Some(Item::Input { port, name }) => {
            let Some(pending) = app.pending else {
                app.status = "arm an output jack first (focus out, Enter)".into();
                return;
            };
            let Some(dst) = voice_id else {
                app.status = "focus a voice module input".into();
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
                    app.status = format!("connected → {name}");
                }
                Err(err) => {
                    warn!(%err, "connect failed");
                    app.status = format!("connect failed: {err}");
                }
            }
        }
        Some(Item::DeviceCycle { endpoint_id }) => {
            if let Some(device_name) = app.next_device_for(&endpoint_id) {
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
                        app.restore_focus(None);
                        app.status = format!("device → {device_name}");
                    }
                    Err(err) => app.status = format!("device failed: {err}"),
                }
            } else {
                app.status = "no devices for this endpoint".into();
            }
        }
        Some(Item::AddKind { kind, display_name }) => {
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
                    if let Some(id) = new_id {
                        if let Some(idx) = app
                            .modules
                            .iter()
                            .position(|m| matches!(&m.id, ModuleId::Voice(v) if *v == id))
                        {
                            app.focus.module = idx;
                            app.focus.item = 0;
                        }
                    }
                    app.status = format!("added {display_name}");
                    app.refresh_status();
                }
                Err(err) => app.status = format!("add failed: {err}"),
            }
        }
        Some(Item::Note { midi, .. }) => toggle_note(app, client, midi).await,
        Some(Item::Param { control_index }) => {
            if let Some((module, param, value)) = app.adjust_param(control_index, 1.0, true) {
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
        Some(Item::Display) | None => {
            app.status = "nothing to activate".into();
        }
    }
}

async fn disconnect_focused_jack(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
) {
    let item = app.focused_item().cloned();
    let voice_id = app.focused_voice_id();
    let (module_id, port, is_output) = match (voice_id, item) {
        (Some(mid), Some(Item::Input { port, .. })) => (mid, port, false),
        (Some(mid), Some(Item::Output { port, .. })) => (mid, port, true),
        _ => {
            app.status = "focus a jack to unplug".into();
            return;
        }
    };

    let cables: Vec<_> = app
        .cables_on_jack(module_id, port, is_output)
        .into_iter()
        .cloned()
        .collect();
    if cables.is_empty() {
        app.status = "no cables on this jack".into();
        return;
    }

    let mut last_snap = None;
    for c in &cables {
        match client
            .patch_disconnect(DisconnectRequest {
                cable_id: c.id,
                src_module: c.src_module,
                src_port: c.src_port,
                dst_module: c.dst_module,
                dst_port: c.dst_port,
                use_cable_id: true,
            })
            .await
        {
            Ok(resp) => last_snap = Some(resp.into_inner()),
            Err(err) => {
                app.status = format!("disconnect failed: {err}");
                return;
            }
        }
    }
    if let Some(snap) = last_snap {
        app.apply_snapshot(snap);
        app.status = format!("unplugged {} cable(s)", cables.len());
    }
}

async fn delete_focused_voice(
    app: &mut App,
    client: &mut SynthClient<tonic::transport::Channel>,
) {
    let Some(id) = app.focused_voice_id() else {
        app.status = "focus a voice module to delete".into();
        return;
    };
    match client
        .remove_module(RemoveModuleRequest { module_id: id })
        .await
    {
        Ok(resp) => {
            app.apply_snapshot(resp.into_inner());
            app.status = format!("removed module {id}");
        }
        Err(err) => app.status = format!("remove failed: {err}"),
    }
}

async fn save_patch(app: &mut App, client: &mut SynthClient<tonic::transport::Channel>) {
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
