//! Orthogonal cable drawing through lattice gutters.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::draw::layout::{tile_in_port, tile_out_port, Geom};
use crate::draw::style::{
    wire_active, wire_dimmed, wire_hover, wire_idle, wire_pending, wire_soft, PRIMARY, PRIMARY_DIM,
    PRIMARY_HOT, BG,
};
use crate::model::LatticePos;
use ratatui::style::Color;

pub struct WireView {
    pub from: LatticePos,
    pub to: LatticePos,
    /// 0..1 live signal level on this cable (from engine peak).
    pub level: f32,
    pub pending: bool,
    /// Endpoint matches module under target cursor.
    pub hover: bool,
    /// Another module is hovered — dim this non-related cable.
    pub dimmed: bool,
}

pub fn draw_wires(
    buf: &mut Buffer,
    canvas: Rect,
    view_origin: LatticePos,
    geom: Geom,
    wires: &[WireView],
) {
    // Quiet first, hot last so active cables win junctions.
    let mut order: Vec<usize> = (0..wires.len()).collect();
    order.sort_by_key(|&i| {
        let w = &wires[i];
        if w.pending {
            4
        } else if w.hover {
            3
        } else if w.level > 0.55 {
            2
        } else if w.level > 0.08 {
            1
        } else {
            0
        }
    });

    for i in order {
        let w = &wires[i];
        let style = if w.pending {
            wire_pending()
        } else if w.hover {
            wire_hover()
        } else if w.dimmed {
            wire_dimmed()
        } else {
            style_for_level(w.level)
        };
        let Some((x0, y0)) = tile_out_port(w.from, view_origin, canvas, geom) else {
            continue;
        };
        let Some((x1, y1)) = tile_in_port(w.to, view_origin, canvas, geom) else {
            continue;
        };
        // Lane offset so parallel cables don't fully stack (scale with gutter).
        let lane_span = (geom.gutter_x as i16 / 2).max(1);
        let lane = ((i as i16) % (lane_span * 2 + 1)) - lane_span;
        let dashed = w.pending || (w.level < 0.05 && !w.hover);
        draw_orth(buf, canvas, x0, y0, x1, y1, lane, style, dashed);
    }
}

fn style_for_level(level: f32) -> Style {
    let l = level.clamp(0.0, 1.0);
    if l < 0.04 {
        wire_idle()
    } else if l < 0.2 {
        wire_soft()
    } else if l < 0.55 {
        // Mid: primary dim scaled by level.
        let t = ((l - 0.2) / 0.35).clamp(0.0, 1.0);
        Style::default()
            .fg(lerp_rgb(PRIMARY_DIM, PRIMARY, t))
            .bg(BG)
    } else if l < 0.85 {
        wire_active()
    } else {
        let t = ((l - 0.85) / 0.15).clamp(0.0, 1.0);
        Style::default()
            .fg(lerp_rgb(PRIMARY, PRIMARY_HOT, t))
            .bg(BG)
    }
}

fn lerp_rgb(a: Color, b: Color, t: f32) -> Color {
    let (ar, ag, ab) = match a {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => (100.0, 100.0, 115.0),
    };
    let (br, bg, bb) = match b {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => (180.0, 230.0, 255.0),
    };
    Color::Rgb(
        (ar + (br - ar) * t).round() as u8,
        (ag + (bg - ag) * t).round() as u8,
        (ab + (bb - ab) * t).round() as u8,
    )
}

fn draw_orth(
    buf: &mut Buffer,
    canvas: Rect,
    x0: u16,
    y0: u16,
    x1: u16,
    y1: u16,
    lane: i16,
    style: Style,
    dashed: bool,
) {
    // Mid-lane dogleg: horizontal to mid_x, vertical, then horizontal.
    let mid_x = {
        let raw = ((x0 as i16 + x1 as i16) / 2) + lane;
        raw.clamp(
            canvas.x as i16,
            (canvas.x + canvas.width.saturating_sub(1)) as i16,
        ) as u16
    };
    hline(buf, canvas, x0, mid_x, y0, style, dashed);
    vline(buf, canvas, mid_x, y0, y1, style, dashed);
    hline(buf, canvas, mid_x, x1, y1, style, dashed);
}

fn hline(
    buf: &mut Buffer,
    canvas: Rect,
    x_a: u16,
    x_b: u16,
    y: u16,
    style: Style,
    dashed: bool,
) {
    if y < canvas.y || y >= canvas.y + canvas.height {
        return;
    }
    let (lo, hi) = if x_a <= x_b { (x_a, x_b) } else { (x_b, x_a) };
    for x in lo..=hi {
        if x < canvas.x || x >= canvas.x + canvas.width {
            continue;
        }
        let ch = if dashed && ((x + y) % 2 == 0) {
            '·'
        } else if x == lo && x == hi {
            '─'
        } else {
            '─'
        };
        plot(buf, x, y, ch, style);
    }
}

fn vline(
    buf: &mut Buffer,
    canvas: Rect,
    x: u16,
    y_a: u16,
    y_b: u16,
    style: Style,
    dashed: bool,
) {
    if x < canvas.x || x >= canvas.x + canvas.width {
        return;
    }
    let (lo, hi) = if y_a <= y_b { (y_a, y_b) } else { (y_b, y_a) };
    for y in lo..=hi {
        if y < canvas.y || y >= canvas.y + canvas.height {
            continue;
        }
        let ch = if dashed && ((x + y) % 2 == 0) {
            '·'
        } else {
            '│'
        };
        plot(buf, x, y, ch, style);
    }
}

fn plot(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        // Prefer existing corner glyphs at junctions when both wires claim a cell.
        let existing = cell.symbol().chars().next().unwrap_or(' ');
        let next = merge_wire_chars(existing, ch);
        cell.set_char(next);
        cell.set_style(style);
    }
}

fn merge_wire_chars(old: char, new: char) -> char {
    match (old, new) {
        (' ', c) | ('·', c) => c,
        ('─', '│') | ('│', '─') => '┼',
        ('─', '─') => '─',
        ('│', '│') => '│',
        (_, c) => c,
    }
}
