//! Main lattice canvas: dim dots, wires, tiles, target cursor.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Block;
use ratatui::Frame;

use crate::draw::layout::{cell_origin, cursor_rect, tile_rect, Geom};
use crate::draw::style::{canvas_bg, cursor_style, lattice_dot};
use crate::draw::glyph::GlyphKind;
use crate::draw::tile::{draw_tile, TileView};
use crate::draw::wires::{draw_wires, WireView};
use crate::model::LatticePos;

pub struct LatticePaint<'a> {
    pub grid_cols: u16,
    pub grid_rows: u16,
    pub view_origin: LatticePos,
    pub cursor: LatticePos,
    pub tiles: &'a [TilePaint],
    pub wires: &'a [WireView],
    pub geom: Geom,
    /// Dim lattice when a menu is open.
    pub dimmed: bool,
    /// Shared live scope buffer for Scope tiles.
    pub scope_samples: &'a [f32],
}

pub struct TilePaint {
    pub pos: LatticePos,
    pub label: String,
    pub glyph: GlyphKind,
    /// 0..1 activity for body fill.
    pub intensity: f32,
    pub soft: bool,
    pub armed: bool,
    /// Prefer live samples when painting (scope buffer or per-module wave).
    pub use_scope: bool,
    /// Owned live waveform for this tile (per-module); scope uses shared buffer.
    pub live_wave: Option<Vec<f32>>,
    /// Quantizer scale pitch-class mask for glyph.
    pub scale_mask: Option<u16>,
}

pub fn draw_lattice(frame: &mut Frame, area: Rect, paint: &LatticePaint<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Fill canvas with dark ground.
    frame.render_widget(Block::default().style(canvas_bg()), area);

    let buf = frame.buffer_mut();
    let geom = paint.geom;
    let (vis_c, vis_r) = geom.visible_span(area);

    // Background dim dots at every lattice cell (visible window + partial spill).
    for dr in 0..=vis_r {
        for dc in 0..=vis_c {
            let col = paint.view_origin.x + dc;
            let row = paint.view_origin.y + dr;
            if col >= paint.grid_cols || row >= paint.grid_rows {
                continue;
            }
            let pos = LatticePos { x: col, y: row };
            let occupied = paint.tiles.iter().any(|t| t.pos == pos);
            if occupied {
                continue;
            }
            let (x, y) = cell_origin(pos, paint.view_origin, area, geom);
            if x >= area.x + area.width || y >= area.y + area.height {
                continue;
            }

            let cx = x + geom.tile_w / 2;
            let cy = y + geom.tile_h / 2;
            if cx < area.x + area.width && cy < area.y + area.height {
                plot_dot(buf, cx, cy);
            }
            // Sparse corner dots when tile is large enough.
            if geom.tile_w >= 5 && geom.tile_h >= 3 {
                let corners = [
                    (1u16, 0u16),
                    (geom.tile_w.saturating_sub(2), 0),
                    (1, geom.tile_h.saturating_sub(1)),
                    (geom.tile_w.saturating_sub(2), geom.tile_h.saturating_sub(1)),
                ];
                for (ox, oy) in corners {
                    let px = x + ox;
                    let py = y + oy;
                    if px < area.x + area.width && py < area.y + area.height {
                        plot_dot(buf, px, py);
                    }
                }
            }
        }
    }

    draw_wires(buf, area, paint.view_origin, geom, paint.wires);

    for t in paint.tiles {
        if let Some(rect) = tile_rect(t.pos, paint.view_origin, area, geom) {
            let intensity = if paint.dimmed {
                t.intensity * 0.45
            } else {
                t.intensity
            };
            let samples: Option<&[f32]> = t
                .live_wave
                .as_deref()
                .or_else(|| {
                    if t.use_scope && !paint.scope_samples.is_empty() {
                        Some(paint.scope_samples)
                    } else {
                        None
                    }
                });
            draw_tile(
                buf,
                rect,
                &TileView {
                    label: &t.label,
                    glyph: t.glyph,
                    intensity,
                    soft: t.soft,
                    armed: t.armed && !paint.dimmed,
                    samples,
                    scale_mask: t.scale_mask,
                },
            );
        }
    }

    if let Some(rect) = cursor_rect(paint.cursor, paint.view_origin, area, geom) {
        draw_cursor(buf, rect, paint.dimmed);
    }
}

fn plot_dot(buf: &mut Buffer, x: u16, y: u16) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char('·');
        cell.set_style(lattice_dot());
    }
}

fn draw_cursor(buf: &mut Buffer, area: Rect, dimmed: bool) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    let style = if dimmed {
        lattice_dot()
    } else {
        cursor_style()
    };
    let x0 = area.x;
    let y0 = area.y;
    let x1 = area.x + area.width - 1;
    let y1 = area.y + area.height - 1;

    set(buf, x0, y0, '┏', style);
    set(buf, x1, y0, '┓', style);
    set(buf, x0, y1, '┗', style);
    set(buf, x1, y1, '┛', style);

    if area.width > 2 {
        set(buf, x0 + 1, y0, '━', style);
        set(buf, x1 - 1, y0, '━', style);
        set(buf, x0 + 1, y1, '━', style);
        set(buf, x1 - 1, y1, '━', style);
    }
    if area.height > 2 {
        set(buf, x0, y0 + 1, '┃', style);
        set(buf, x1, y0 + 1, '┃', style);
        set(buf, x0, y1 - 1, '┃', style);
        set(buf, x1, y1 - 1, '┃', style);
    }
}

fn set(buf: &mut Buffer, x: u16, y: u16, ch: char, style: ratatui::style::Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch);
        cell.set_style(style);
    }
}
