//! Responsive module grid with pagination viewport.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::draw::layout::{compute_grid, GridLayout};
use crate::draw::panel::{draw_module_panel, ModulePanelView};

/// Draw only the visible page of modules; returns layout for App/status.
pub fn draw_module_grid(
    frame: &mut Frame,
    area: Rect,
    panels: &[ModulePanelView],
    view_origin: usize,
) -> GridLayout {
    if panels.is_empty() {
        frame.render_widget(Paragraph::new("  no modules"), area);
        return compute_grid(area, panels, 0);
    }

    let layout = compute_grid(area, panels, view_origin);
    let slice = &panels[layout.visible_start..layout.visible_end];
    if slice.is_empty() {
        return layout;
    }

    let panel_w = layout.panel_w;
    let gap = layout.gap;
    let cols = layout.cols;

    // Outer side margins from leftover gap formula
    let margin = gap;
    let grid_area = Rect {
        x: area.x.saturating_add(margin),
        y: area.y,
        width: area.width.saturating_sub(margin * 2).max(panel_w),
        height: area.height,
    };

    let rows: Vec<&[ModulePanelView]> = slice.chunks(cols).collect();
    let row_heights: Vec<u16> = rows
        .iter()
        .map(|row| {
            let h = row
                .iter()
                .map(|p| crate::draw::panel::panel_height(p))
                .max()
                .unwrap_or(6);
            h.min(grid_area.height.max(5)).saturating_add(gap)
        })
        .collect();

    let mut constraints: Vec<Constraint> = row_heights
        .iter()
        .map(|h| Constraint::Length(*h))
        .collect();
    constraints.push(Constraint::Min(0));
    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(grid_area);

    for (ri, row) in rows.iter().enumerate() {
        if ri >= row_areas.len() {
            break;
        }
        let row_rect = row_areas[ri];
        let panel_h = row_rect.height.saturating_sub(gap).max(5);
        let paint_row = Rect {
            x: row_rect.x,
            y: row_rect.y,
            width: row_rect.width,
            height: panel_h,
        };

        let mut col_constraints: Vec<Constraint> = Vec::new();
        for _ in row.iter() {
            col_constraints.push(Constraint::Length(panel_w));
            col_constraints.push(Constraint::Length(gap));
        }
        col_constraints.push(Constraint::Min(0));
        let col_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(paint_row);

        for (ci, panel) in row.iter().enumerate() {
            let idx = ci * 2;
            if idx < col_areas.len() {
                draw_module_panel(frame, col_areas[idx], panel);
            }
        }
    }

    layout
}
