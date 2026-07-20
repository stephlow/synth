//! Readability-first styles — no color overrides.
//!
//! Primary content uses the terminal default foreground.
//! DIM is only for decoration. Focus uses reverse + bold.

use ratatui::style::{Modifier, Style};

/// Normal readable text (theme default fg).
pub fn primary() -> Style {
    Style::default()
}

/// Emphasized labels (section headers, connected jacks).
pub fn strong() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

/// Decoration only (hairlines, secondary keymap). Never for names/values.
pub fn quiet() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

/// Focused row / jack.
pub fn focus_row() -> Style {
    Style::default()
        .add_modifier(Modifier::REVERSED)
        .add_modifier(Modifier::BOLD)
}

/// Pending cable arm.
pub fn pending() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

pub fn title_style(module_focused: bool) -> Style {
    if module_focused {
        strong()
    } else {
        primary()
    }
}

pub fn border_style(module_focused: bool) -> Style {
    if module_focused {
        strong()
    } else {
        primary()
    }
}

pub fn line_style(focused: bool, highlight: bool) -> Style {
    if focused {
        focus_row()
    } else if highlight {
        pending()
    } else {
        primary()
    }
}

pub fn status_primary(pending_arm: bool) -> Style {
    if pending_arm {
        pending()
    } else {
        primary()
    }
}

pub fn status_secondary() -> Style {
    quiet()
}
