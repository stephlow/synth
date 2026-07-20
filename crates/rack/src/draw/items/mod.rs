//! Item row formatters — pure strings, no Frame/App.
//!
//! Assembly that needs App data lives in `draw::assemble`.

mod add_kind_row;
mod device_row;
mod display_row;
mod focus_mark;
mod note_row;
mod param_row;
mod sparkline;

pub use add_kind_row::{format_add_kind_row, AddKindRowView};
pub use device_row::{format_device_row, DeviceRowView};
pub use display_row::{format_display_row, DisplayKind, DisplayRowView};
pub use focus_mark::focus_mark;
pub use note_row::{format_note_row, NoteRowView};
pub use param_row::{format_param_row, ParamRowView};

use crate::draw::panel::PanelLine;

/// A fully formatted list row ready for the panel painter.
pub fn panel_line(text: String, focused: bool, highlight: bool) -> PanelLine {
    PanelLine {
        text,
        focused,
        highlight,
    }
}
