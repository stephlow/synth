//! App chrome: wordmark banner, rack grid, status strip.

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::draw::grid::draw_module_grid;
use crate::draw::layout::GridLayout;
use crate::draw::panel::ModulePanelView;
use crate::draw::status::draw_status;

pub fn draw_screen(
    frame: &mut Frame,
    panels: &[ModulePanelView],
    view_origin: usize,
    status: &str,
    pending: bool,
    page: usize,
    pages: usize,
) -> GridLayout {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area);

    draw_banner(frame, chunks[0], page, pages, panels.len());
    let layout = draw_module_grid(frame, chunks[1], panels, view_origin);
    // Prefer live layout page numbers for status strip
    draw_status(
        frame,
        chunks[2],
        status,
        pending,
        layout.page,
        layout.pages,
        layout.visible_start,
        layout.visible_end,
        layout.total,
    );
    layout
}

fn draw_banner(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    page: usize,
    pages: usize,
    total: usize,
) {
    let w = area.width as usize;
    let left = "◈  RACK";
    let mid = "modular voice · live dsp";
    let right = if pages > 1 {
        format!("p{}/{} · {} mod", page + 1, pages, total)
    } else {
        format!("{total} mod")
    };
    let used = left.chars().count() + mid.chars().count() + right.chars().count();
    let gap = w.saturating_sub(used + 4).max(2);
    let gap_l = gap / 2;
    let gap_r = gap - gap_l;

    let line = Line::from(vec![
        Span::styled(
            format!("  {left}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(gap_l)),
        Span::raw(mid),
        Span::raw(" ".repeat(gap_r)),
        Span::styled(right, Style::default().add_modifier(Modifier::DIM)),
    ]);

    let rule = Line::from(Span::styled(
        "═".repeat(w),
        Style::default().add_modifier(Modifier::DIM),
    ));

    frame.render_widget(Paragraph::new(vec![line, rule]), area);
}
