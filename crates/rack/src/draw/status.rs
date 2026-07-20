//! Status strip under the lattice.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::draw::style::{canvas_bg, status_primary, status_rule, status_secondary};
use crate::draw::text::truncate;
use crate::model::LatticePos;

pub fn draw_status(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    pending: bool,
    cursor: LatticePos,
    grid_cols: u16,
    grid_rows: u16,
    status_h: u16,
) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(Block::default().style(canvas_bg()), area);

    let w = area.width as usize;
    let mut lines = Vec::new();

    if status_h >= 2 {
        lines.push(Line::from(Span::styled(
            "─".repeat(w),
            status_rule(),
        )));
    }

    let prefix = if pending { " ◆ " } else { " · " };
    let pos = if w > 60 {
        format!("   ·  @{},{} / {}×{}", cursor.x, cursor.y, grid_cols, grid_rows)
    } else if w > 36 {
        format!("  @{},{}", cursor.x, cursor.y)
    } else {
        String::new()
    };
    let body = truncate(&format!("{prefix}{text}{pos}"), w);
    lines.push(Line::from(Span::styled(body, status_primary(pending))));

    if status_h >= 3 && area.height >= lines.len() as u16 + 1 {
        let hints = if w >= 56 {
            "   arrows navigate   Enter menu   Esc back   z-m notes   q quit"
        } else if w >= 36 {
            "   arrows · Enter · Esc · q"
        } else {
            "   ←↑↓→ Enter Esc q"
        };
        lines.push(Line::from(Span::styled(
            truncate(hints, w),
            status_secondary(),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}
