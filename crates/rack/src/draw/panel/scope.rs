//! Full-area oscilloscope: uses the entire panel inner rect.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::Frame;

use crate::draw::style::{focus_row, primary, strong};

/// Paint a waveform into `area` (already inside chrome).
pub fn draw_scope(frame: &mut Frame, area: Rect, samples: &[f32], focused: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let buf = frame.buffer_mut();
    let w = area.width as usize;
    let h = area.height as usize;

    // Clear / fill with spaces at default style
    let base = if focused {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        primary()
    };
    for y in 0..area.height {
        for x in 0..area.width {
            set(buf, area.x + x, area.y + y, ' ', base);
        }
    }

    if samples.is_empty() || h == 0 || w == 0 {
        let msg = " no signal ";
        let start = area.x + area.width.saturating_sub(msg.len() as u16) / 2;
        for (i, ch) in msg.chars().enumerate() {
            set(buf, start + i as u16, area.y + area.height / 2, ch, strong());
        }
        return;
    }

    // Midline
    let mid_y = area.y + area.height / 2;
    let mid_style = Style::default().add_modifier(Modifier::DIM);
    for x in 0..area.width {
        set(buf, area.x + x, mid_y, '─', mid_style);
    }

    // Resample to width columns; value in -1..1 (or abs peak normalize)
    let peak = samples
        .iter()
        .fold(0.0f32, |m, &s| m.max(s.abs()))
        .max(1e-4);

    let style = if focused {
        focus_row()
    } else {
        strong()
    };

    for col in 0..w {
        let idx = col * samples.len() / w;
        let s = samples.get(idx).copied().unwrap_or(0.0) / peak;
        let s = s.clamp(-1.0, 1.0);
        // Map -1..1 to bottom..top of area
        let row_f = (1.0 - (s + 1.0) * 0.5) * (h.saturating_sub(1) as f32);
        let row = (row_f.round() as usize).min(h.saturating_sub(1));
        let y = area.y + row as u16;
        let x = area.x + col as u16;

        // Vertical stem from midline to sample for a filled scope look
        let mid = h / 2;
        let (lo, hi) = if row <= mid {
            (row, mid)
        } else {
            (mid, row)
        };
        for r in lo..=hi {
            let ch = if r == row {
                if s.abs() < 0.05 {
                    '·'
                } else if s > 0.0 {
                    '▀'
                } else {
                    '▄'
                }
            } else if r == mid {
                '│'
            } else {
                '│'
            };
            // Don't wipe midline style entirely — plot over it
            let st = if r == row { style } else { mid_style };
            set(buf, x, area.y + r as u16, ch, st);
        }
        let _ = y;
    }

    // Peak readout bottom-right
    let label = format!(" ±{:.2} ", peak.min(9.99));
    let lx = area
        .x
        .saturating_add(area.width.saturating_sub(label.len() as u16 + 1));
    let ly = area.y + area.height.saturating_sub(1);
    for (i, ch) in label.chars().enumerate() {
        set(buf, lx + i as u16, ly, ch, primary());
    }
}

fn set(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch);
        cell.set_style(style);
    }
}
