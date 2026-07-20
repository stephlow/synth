//! Module tile: tiny top label + kind-specific audio glyph body.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::draw::glyph::{paint_glyph, GlyphKind, GlyphPaint};
use crate::draw::style::{
    bg, tile_body_armed, tile_body_hot, tile_body_idle, tile_body_soft, tile_border_armed,
    tile_border_hot, tile_border_idle, tile_border_soft, tile_label_armed, tile_label_hot,
    tile_label_idle, tile_label_soft,
};
use crate::draw::text::truncate;

pub struct TileView<'a> {
    pub label: &'a str,
    pub glyph: GlyphKind,
    /// 0..1 activity for glyph amplitude / density.
    pub intensity: f32,
    pub armed: bool,
    /// Connected / present (border soft even at low intensity).
    pub soft: bool,
    /// Live samples for scope glyph.
    pub samples: Option<&'a [f32]>,
}

pub fn draw_tile(buf: &mut Buffer, area: Rect, view: &TileView<'_>) {
    if area.width < 3 || area.height < 2 {
        return;
    }

    let intensity = view.intensity.clamp(0.0, 1.0);
    let hot = intensity >= 0.55 || view.armed;

    let label_style = if view.armed {
        tile_label_armed()
    } else if hot {
        tile_label_hot()
    } else if view.soft || intensity > 0.1 {
        tile_label_soft()
    } else {
        tile_label_idle()
    };

    let border_style = if view.armed {
        tile_border_armed()
    } else if hot {
        tile_border_hot()
    } else if view.soft || intensity > 0.1 {
        tile_border_soft()
    } else {
        tile_border_idle()
    };

    let body_style = if view.armed {
        tile_body_armed()
    } else if hot {
        tile_body_hot()
    } else if view.soft || intensity > 0.12 {
        tile_body_soft()
    } else {
        tile_body_idle()
    };

    let (tl, tr, bl, br, h, v) = if view.armed || hot {
        ('╔', '╗', '╚', '╝', '═', '║')
    } else if view.soft || intensity > 0.1 {
        ('┌', '┐', '└', '┘', '─', '│')
    } else {
        ('╭', '╮', '╰', '╯', '─', '│')
    };

    // Prefer a dedicated top label row so the bordered body is pure glyph.
    // On short tiles, squeeze the label into the top border line instead.
    let has_label_row = area.height >= 5;
    let body = if has_label_row {
        Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height - 1,
        }
    } else {
        area
    };

    if has_label_row {
        let cap = (area.width as usize).min(3);
        let text = truncate(view.label, cap);
        let start = area.x + area.width.saturating_sub(text.chars().count() as u16) / 2;
        for (i, ch) in text.chars().enumerate() {
            let x = start + i as u16;
            if x >= area.x + area.width {
                break;
            }
            set(buf, x, area.y, ch, label_style);
        }
    }

    if body.width < 3 || body.height < 2 {
        return;
    }

    let x0 = body.x;
    let y0 = body.y;
    let x1 = body.x + body.width - 1;
    let y1 = body.y + body.height - 1;

    let clear = Style::default().bg(bg()).fg(bg());
    for y in y0..=y1 {
        for x in x0..=x1 {
            set(buf, x, y, ' ', clear);
        }
    }

    set(buf, x0, y0, tl, border_style);
    set(buf, x1, y0, tr, border_style);
    set(buf, x0, y1, bl, border_style);
    set(buf, x1, y1, br, border_style);
    for x in (x0 + 1)..x1 {
        set(buf, x, y0, h, border_style);
        set(buf, x, y1, h, border_style);
    }
    for y in (y0 + 1)..y1 {
        set(buf, x0, y, v, border_style);
        set(buf, x1, y, v, border_style);
    }

    // Short tiles: micro label sits on the top border (not a separate row).
    if !has_label_row && area.width > 2 {
        let cap = (area.width.saturating_sub(2) as usize).min(3);
        let text = truncate(view.label, cap);
        let start =
            x0 + 1 + (area.width.saturating_sub(2).saturating_sub(text.chars().count() as u16)) / 2;
        for (i, ch) in text.chars().enumerate() {
            let x = start + i as u16;
            if x >= x1 {
                break;
            }
            set(buf, x, y0, ch, label_style);
        }
    }

    let inner = Rect {
        x: x0 + 1,
        y: y0 + 1,
        width: body.width.saturating_sub(2),
        height: body.height.saturating_sub(2),
    };
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // Always draw the kind glyph; floor intensity so idle modules still show
    // their waveform / ADSR / filter shape (activity scales brightness later).
    let fill_level = if view.armed {
        intensity.max(0.55)
    } else if view.soft {
        intensity.max(0.35)
    } else {
        intensity.max(0.22)
    };

    paint_glyph(
        buf,
        inner,
        &GlyphPaint {
            kind: view.glyph,
            intensity: fill_level,
            style: body_style,
            samples: view.samples,
        },
    );
}

fn set(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch);
        cell.set_style(style);
    }
}
