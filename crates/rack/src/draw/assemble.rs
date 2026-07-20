//! Build dumb panel views from `App` (only place in draw that reads App).

use crate::app::App;
use crate::draw::items::{
    format_add_kind_row, format_device_row, format_display_row, format_note_row, format_param_row,
    panel_line, AddKindRowView, DeviceRowView, DisplayKind, DisplayRowView, NoteRowView,
    ParamRowView, focus_mark,
};
use crate::draw::panel::{JackFace, ModulePanelView, PanelLine};
use crate::model::{Item, ModuleId, UiModule};

pub fn panel_views(app: &App) -> Vec<ModulePanelView> {
    let pending = app.pending.map(|p| (p.src_module, p.src_port));
    app.modules
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let focused_mod = i == app.focus.module;
            let title = match &m.id {
                ModuleId::Io(id) => format!("{} · {id}", m.title),
                _ => m.title.clone(),
            };

            let voice_id = match &m.id {
                ModuleId::Voice(id) => Some(*id),
                _ => None,
            };

            let mut inputs = Vec::new();
            let mut outputs = Vec::new();
            let mut body = Vec::new();
            let mut scope_samples = None;

            // Scope: full-panel waveform; skip list body.
            if matches!(m.id, ModuleId::Scope) {
                scope_samples = Some(app.scope.clone());
            } else {
                for (j, item) in m.items.iter().enumerate() {
                    let focused = focused_mod && j == app.focus.item;
                    match item {
                        Item::Input { port, name } => {
                            let mid = voice_id.unwrap_or(0);
                            let peer = app.jack_peer_hint(mid, *port, false);
                            inputs.push(JackFace {
                                name: name.clone(),
                                connected: !peer.is_empty(),
                                focused,
                                pending: false,
                                peer,
                            });
                        }
                        Item::Output { port, name } => {
                            let mid = voice_id.unwrap_or(0);
                            let peer = app.jack_peer_hint(mid, *port, true);
                            let is_pending = pending == Some((mid, *port));
                            outputs.push(JackFace {
                                name: name.clone(),
                                connected: !peer.is_empty(),
                                focused,
                                pending: is_pending,
                                peer,
                            });
                        }
                        other => {
                            body.push(body_line(app, m, other, focused));
                        }
                    }
                }
            }

            let show_jacks = !inputs.is_empty() || !outputs.is_empty();

            ModulePanelView {
                title,
                kind: m.kind.clone(),
                focused: focused_mod,
                inputs,
                outputs,
                body,
                show_jacks,
                scope_samples,
            }
        })
        .collect()
}

fn body_line(app: &App, module: &UiModule, item: &Item, focused: bool) -> PanelLine {
    let mark = focus_mark(focused);
    match item {
        Item::Param { control_index } => {
            let c = &app.controls[*control_index];
            let v = app.values[*control_index];
            let text = format_param_row(ParamRowView {
                mark,
                name: &c.name,
                value: v,
                min: c.min,
                max: c.max,
            });
            panel_line(text, focused, false)
        }
        Item::DeviceCycle { endpoint_id } => {
            let dev = app
                .io_meters
                .iter()
                .find(|m| m.endpoint_id == *endpoint_id)
                .map(|m| m.device_name.as_str())
                .or_else(|| {
                    app.io_endpoints
                        .iter()
                        .find(|e| e.id == *endpoint_id)
                        .map(|e| e.device_name.as_str())
                })
                .unwrap_or("?");
            let text = format_device_row(DeviceRowView {
                mark,
                device_name: dev,
                max_width: 22,
            });
            panel_line(text, focused, false)
        }
        Item::AddKind {
            display_name,
            kind,
        } => {
            let text = format_add_kind_row(AddKindRowView {
                mark,
                display_name,
                kind,
            });
            panel_line(text, focused, false)
        }
        Item::Note { label, midi } => {
            let text = format_note_row(NoteRowView {
                mark,
                label: *label,
                midi: *midi,
                held: app.held_notes.contains(midi),
            });
            panel_line(text, focused, false)
        }
        Item::Display => {
            let kind = match module.id {
                ModuleId::Scope => DisplayKind::Scope {
                    samples: app.scope.clone(),
                },
                ModuleId::Meter => DisplayKind::Meter {
                    peak: app.output_peak,
                    voices: app.active_voices,
                },
                _ => DisplayKind::Empty,
            };
            let text = format_display_row(DisplayRowView { mark, kind });
            panel_line(text, focused, false)
        }
        Item::Input { .. } | Item::Output { .. } => {
            // handled as jacks
            panel_line(String::new(), false, false)
        }
    }
}
