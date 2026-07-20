//! App chrome: wordmark banner, lattice canvas, status strip, mode overlays.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::draw::assemble::{tile_paints, wire_views};
use crate::draw::lattice::{draw_lattice, LatticePaint};
use crate::draw::layout::{chrome_heights, Geom};
use crate::draw::menu::{draw_menu, draw_param_edit};
use crate::draw::status::draw_status;
use crate::draw::style::{
    banner_right, banner_rule, banner_title, canvas_bg,
};
use crate::draw::text::truncate;

pub fn draw_screen(frame: &mut Frame, app: &App) {
    let area = frame.area();
    // Full-terminal dark ground.
    frame.render_widget(Block::default().style(canvas_bg()), area);

    let (banner_h, status_h) = chrome_heights(area.height);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(banner_h),
            Constraint::Min(3),
            Constraint::Length(status_h),
        ])
        .split(area);

    draw_banner(frame, chunks[0], app, banner_h);
    let canvas = chunks[1];
    let geom = Geom::from_canvas(canvas);

    let tiles = tile_paints(app);
    let wires = wire_views(app);
    let dimmed =
        (!app.menu_stack.is_empty() || app.param_edit.is_some()) && app.moving.is_none();

    draw_lattice(
        frame,
        canvas,
        &LatticePaint {
            grid_cols: app.grid_cols,
            grid_rows: app.grid_rows,
            view_origin: app.view_origin,
            cursor: app.cursor,
            tiles: &tiles,
            wires: &wires,
            geom,
            dimmed,
            scope_samples: &app.scope,
        },
    );

    if let Some(ci) = app.param_edit {
        if let Some(c) = app.controls.get(ci) {
            let v = app.values.get(ci).copied().unwrap_or(0.0);
            draw_param_edit(frame, canvas, "PARAM", &c.name, v, c.min, c.max);
        }
    } else if let Some(menu) = app.menu_stack.last() {
        draw_menu(frame, canvas, menu);
    }

    draw_status(
        frame,
        chunks[2],
        &app.status,
        app.pending.is_some(),
        app.cursor,
        app.grid_cols,
        app.grid_rows,
        status_h,
    );
}

fn draw_banner(frame: &mut Frame, area: Rect, app: &App, banner_h: u16) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    frame.render_widget(Block::default().style(canvas_bg()), area);

    let w = area.width as usize;
    let left = "◈  RACK";
    let right = if w >= 40 {
        format!(
            "{},{} · {}×{}",
            app.cursor.x, app.cursor.y, app.grid_cols, app.grid_rows
        )
    } else {
        format!("{},{}", app.cursor.x, app.cursor.y)
    };

    let left_n = left.chars().count();
    let right_n = right.chars().count();
    let gap = w.saturating_sub(left_n + right_n + 4).max(1);

    let spans = vec![
        Span::styled(format!("  {left}"), banner_title()),
        Span::styled(" ".repeat(gap), canvas_bg()),
        Span::styled(truncate(&right, w.saturating_sub(left_n + 4)), banner_right()),
    ];

    let line = Line::from(spans);
    if banner_h >= 2 {
        let rule = Line::from(Span::styled("═".repeat(w), banner_rule()));
        frame.render_widget(Paragraph::new(vec![line, rule]), area);
    } else {
        frame.render_widget(Paragraph::new(line), area);
    }
}
