//! Build paint views from `App`.

use crate::app::App;
use crate::draw::glyph::GlyphKind;
use crate::draw::lattice::TilePaint;
use crate::draw::wires::WireView;
use crate::model::{LatticePos, ModuleId};

pub fn tile_paints(app: &App) -> Vec<TilePaint> {
    let pending_mod = app.pending.map(|p| p.src_module);
    let moving_idx = app.moving.as_ref().map(|m| m.module_idx);
    let mut out = Vec::new();
    for (i, m) in app.modules.iter().enumerate() {
        let Some(pos) = app.positions.get(i).copied().flatten() else {
            continue;
        };
        let armed = match &m.id {
            ModuleId::Voice(id) => pending_mod == Some(*id),
            _ => false,
        } || moving_idx == Some(i);

        let glyph = match &m.id {
            ModuleId::Io(_) => GlyphKind::Io,
            _ => GlyphKind::from_module_kind(&m.kind),
        };

        // Live waveform from engine module_signals (patch modules only).
        // Oscilloscope modules also fall back to master mix when they are the bus tap.
        let live_wave = match &m.id {
            ModuleId::Voice(id) => {
                let from_mod = app
                    .module_waveform(*id)
                    .map(|w| w.to_vec())
                    .filter(|w| w.iter().any(|s| s.abs() > 1e-5));
                if from_mod.is_some() {
                    from_mod
                } else if m.kind == "oscilloscope"
                    && app.roles.output_tap_module == *id
                    && !app.scope.is_empty()
                {
                    Some(app.scope.clone())
                } else {
                    None
                }
            }
            ModuleId::Io(_) => None,
        };
        let use_live = live_wave.is_some() || matches!(glyph, GlyphKind::Scope);

        out.push(TilePaint {
            pos,
            label: app.tile_label(i),
            glyph,
            intensity: app.module_intensity(i),
            soft: app.tile_soft(i),
            armed,
            use_scope: use_live,
            live_wave,
            detail: app.glyph_detail(i),
        });
    }
    out
}

pub fn wire_views(app: &App) -> Vec<WireView> {
    let mut wires = Vec::new();
    let hover = app.hover_voice_id();
    let any_hover = hover.is_some();

    for c in &app.cables {
        let from = pos_for_voice(app, c.src_module);
        let to = pos_for_voice(app, c.dst_module);
        let (Some(from), Some(to)) = (from, to) else {
            continue;
        };
        let is_hover = hover.is_some_and(|h| h == c.src_module || h == c.dst_module);
        let peak = app.cable_peak(c.id);
        // 0..1 level for drawing; quiet cables stay dim, hot signal bright.
        let level = if peak < 0.002 {
            0.0
        } else {
            (peak * 2.5).clamp(0.08, 1.0)
        };
        wires.push(WireView {
            from,
            to,
            level,
            pending: false,
            hover: is_hover,
            dimmed: any_hover && !is_hover,
        });
    }

    if let Some(p) = app.pending {
        if let Some(from) = pos_for_voice(app, p.src_module) {
            if from != app.cursor {
                // Ghost cable uses live peak of the armed source if available.
                let level = app
                    .module_peak(p.src_module)
                    .max(0.2)
                    .clamp(0.2, 0.7);
                wires.push(WireView {
                    from,
                    to: app.cursor,
                    level,
                    pending: true,
                    hover: false,
                    dimmed: false,
                });
            }
        }
    }

    wires
}

fn pos_for_voice(app: &App, voice_id: u32) -> Option<LatticePos> {
    let idx = app
        .modules
        .iter()
        .position(|m| matches!(&m.id, ModuleId::Voice(id) if *id == voice_id))?;
    app.positions.get(idx).copied().flatten()
}
