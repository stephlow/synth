//! Composable TUI drawing.

mod assemble;
mod grid;
mod items;
pub mod layout;
mod panel;
mod screen;
mod status;
mod style;
mod text;

pub use assemble::panel_views;

use ratatui::Frame;

use crate::app::App;
use screen::draw_screen;

/// Draw the UI and store viewport layout back onto `app`.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let panels = panel_views(app);
    let layout = draw_screen(
        frame,
        &panels,
        app.view_origin,
        &app.status,
        app.pending.is_some(),
        app.last_page,
        app.last_pages,
    );
    app.record_layout(
        layout.visible_start,
        layout.visible_end,
        layout.page,
        layout.pages,
    );
}
