//! Status strip under the rack.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::draw::style::{status_primary, status_secondary};

pub fn draw_status(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    pending: bool,
    page: usize,
    pages: usize,
    vis_start: usize,
    vis_end: usize,
    total: usize,
) {
    if area.height == 0 {
        return;
    }

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "─".repeat(area.width as usize),
        Style::default().add_modifier(Modifier::DIM),
    )));

    if area.height >= 2 {
        let prefix = if pending { " ◆ " } else { " · " };
        let page_bit = if pages > 1 {
            format!(
                "   ·  page {}/{}  ·  showing {}–{} of {}",
                page + 1,
                pages,
                vis_start + 1,
                vis_end.max(vis_start + 1),
                total
            )
        } else {
            String::new()
        };
        lines.push(Line::from(Span::styled(
            format!("{prefix}{text}{page_bit}"),
            status_primary(pending),
        )));
    }

    if area.height >= 3 {
        lines.push(Line::from(Span::styled(
            "   h/l module   j/k item   enter patch   [ ] page   -/= knob   x unplug   S/L save",
            status_secondary(),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}
