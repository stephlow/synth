//! Command menu overlay (mode stack top).

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::draw::layout::menu_rect;
use crate::draw::style::{
    menu_border, menu_clear, menu_item, menu_selected, menu_title, menu_value, param_bar_empty,
    param_bar_fill, param_value,
};
use crate::draw::text::truncate;
use crate::model::MenuState;

pub fn draw_menu(frame: &mut Frame, canvas: Rect, menu: &MenuState) {
    let n = menu.items.len().max(1) as u16;
    let rows = (n + 2).min(canvas.height.saturating_sub(1)).max(3);
    let max_label = menu
        .items
        .iter()
        .map(|e| e.label.chars().count())
        .max()
        .unwrap_or(8)
        .max(menu.title.chars().count())
        + 4;
    let want_cols = (max_label as u16 + 4).max(16);
    let area = menu_rect(canvas, rows, want_cols);

    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(menu_clear()), area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(menu_border())
        .style(menu_clear())
        .title(Span::styled(format!(" {} ", menu.title), menu_title()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let visible = inner.height as usize;
    let sel = menu.selected;
    let start = if sel >= visible {
        sel + 1 - visible
    } else {
        0
    };

    let mut lines = Vec::new();
    for (i, entry) in menu.items.iter().enumerate().skip(start).take(visible) {
        let focused = i == sel;
        let prefix = if entry.submenu {
            "▸ "
        } else if entry.action.is_none() {
            "· "
        } else {
            "  "
        };
        let text = truncate(
            &format!("{prefix}{}", entry.label),
            inner.width as usize,
        );
        let style = if focused {
            menu_selected()
        } else if entry.action.is_none() {
            menu_value()
        } else {
            menu_item()
        };
        // Pad selected row to full width for solid reverse bar.
        let padded = if focused {
            let n = text.chars().count();
            let pad = (inner.width as usize).saturating_sub(n);
            format!("{text}{}", " ".repeat(pad))
        } else {
            text
        };
        lines.push(Line::from(Span::styled(padded, style)));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

pub fn draw_param_edit(
    frame: &mut Frame,
    canvas: Rect,
    title: &str,
    name: &str,
    value: f32,
    min: f32,
    max: f32,
) {
    let range = (max - min).abs().max(1e-6);
    let t = ((value - min) / range).clamp(0.0, 1.0);
    // Bar width scales with canvas.
    let bar_w = ((canvas.width as usize / 3).clamp(10, 28)) as usize;
    let filled = ((t * bar_w as f32).round() as usize).min(bar_w);

    let lines_n = 5u16;
    let cols = ((bar_w as u16) + 18)
        .min(canvas.width.saturating_sub(2))
        .max(20);
    let area = menu_rect(canvas, lines_n, cols);
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(menu_clear()), area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(menu_border())
        .style(menu_clear())
        .title(Span::styled(format!(" {title} "), menu_title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Multi-span fader: filled primary, empty deep grey.
    let mut bar_spans = vec![Span::styled("[", menu_value())];
    if filled > 0 {
        bar_spans.push(Span::styled("█".repeat(filled), param_bar_fill()));
    }
    if filled < bar_w {
        bar_spans.push(Span::styled("·".repeat(bar_w - filled), param_bar_empty()));
    }
    bar_spans.push(Span::styled("]", menu_value()));
    bar_spans.push(Span::styled(format!("  {value:.3}"), param_value()));

    let body = vec![
        Line::from(Span::styled(name.to_string(), menu_title())),
        Line::from(bar_spans),
        Line::from(Span::styled(
            "-/= adjust   _/+ fine   j/k next   Esc",
            menu_value(),
        )),
    ];
    frame.render_widget(Paragraph::new(body), inner);
}
