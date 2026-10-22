//! Composable TUI drawing — lattice + mode overlays.

mod assemble;
pub(crate) mod glyph;
mod lattice;
pub mod layout;
mod menu;
mod screen;
mod status;
pub mod style;
mod text;
mod tile;
mod wires;

use ratatui::Frame;

use crate::app::App;
use screen::draw_screen;

/// Draw the UI from `app` state.
pub fn draw(frame: &mut Frame, app: &mut App) {
    // Keep lattice bounds filled to the current viewport.
    app.ensure_cursor_visible();
    style::set_active(app.display_theme().palette());
    draw_screen(frame, app);
}
