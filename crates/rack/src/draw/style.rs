//! Dark-mode palette: near-black ground, white/grey text, primary accent for intensity.

use ratatui::style::{Color, Modifier, Style};

// ── Palette ─────────────────────────────────────────────────────────────────

/// Canvas / chrome ground.
pub const BG: Color = Color::Rgb(10, 10, 14);
/// Slightly lifted surface (menus, panels).
pub const BG_RAISED: Color = Color::Rgb(18, 18, 24);
/// Deep lattice dots.
pub const GREY_DEEP: Color = Color::Rgb(42, 42, 52);
/// Quiet secondary (hints, idle wires).
pub const GREY: Color = Color::Rgb(100, 100, 115);
/// Readable mid grey (soft tiles, secondary labels).
pub const GREY_BRIGHT: Color = Color::Rgb(165, 165, 180);
/// Primary text / idle module labels.
pub const WHITE: Color = Color::Rgb(232, 232, 240);
/// Intensity accent — active wires, lit modules, armed pending.
pub const PRIMARY: Color = Color::Rgb(120, 200, 255);
/// Softer primary (pending cable, soft glow).
pub const PRIMARY_DIM: Color = Color::Rgb(70, 130, 175);
/// Hot primary (peak activity).
pub const PRIMARY_HOT: Color = Color::Rgb(180, 230, 255);

fn base(fg: Color) -> Style {
    Style::default().fg(fg).bg(BG)
}

fn raised(fg: Color) -> Style {
    Style::default().fg(fg).bg(BG_RAISED)
}

// ── Semantic styles ─────────────────────────────────────────────────────────

pub fn quiet() -> Style {
    base(GREY)
}

pub fn focus_row() -> Style {
    Style::default()
        .fg(BG)
        .bg(PRIMARY)
        .add_modifier(Modifier::BOLD)
}

pub fn pending() -> Style {
    base(PRIMARY).add_modifier(Modifier::BOLD)
}

pub fn canvas_bg() -> Style {
    Style::default().bg(BG).fg(GREY_DEEP)
}

pub fn lattice_dot() -> Style {
    base(GREY_DEEP)
}

/// Tiny kind label above the body — always quieter than the activity fill.
pub fn tile_label_idle() -> Style {
    base(GREY_DEEP)
}

pub fn tile_label_soft() -> Style {
    base(GREY)
}

pub fn tile_label_hot() -> Style {
    base(GREY_BRIGHT)
}

pub fn tile_label_armed() -> Style {
    base(PRIMARY_DIM)
}

/// Border around activity body.
pub fn tile_border_idle() -> Style {
    base(GREY_DEEP)
}

pub fn tile_border_soft() -> Style {
    base(GREY)
}

pub fn tile_border_hot() -> Style {
    base(PRIMARY_DIM)
}

pub fn tile_border_armed() -> Style {
    base(PRIMARY).add_modifier(Modifier::BOLD)
}

/// Body fill glyphs (activity level).
pub fn tile_body_idle() -> Style {
    base(GREY_DEEP)
}

pub fn tile_body_soft() -> Style {
    base(GREY)
}

pub fn tile_body_hot() -> Style {
    base(PRIMARY).add_modifier(Modifier::BOLD)
}

pub fn tile_body_armed() -> Style {
    base(PRIMARY_HOT).add_modifier(Modifier::BOLD)
}

pub fn cursor_style() -> Style {
    base(WHITE).add_modifier(Modifier::BOLD)
}

pub fn wire_idle() -> Style {
    base(GREY_DEEP)
}

pub fn wire_soft() -> Style {
    base(GREY)
}

pub fn wire_active() -> Style {
    base(PRIMARY).add_modifier(Modifier::BOLD)
}

/// Cables belonging to the module under the target cursor.
pub fn wire_hover() -> Style {
    base(PRIMARY_HOT).add_modifier(Modifier::BOLD)
}

pub fn wire_pending() -> Style {
    base(PRIMARY_DIM).add_modifier(Modifier::BOLD)
}

/// Non-related wires while a module is hovered (extra dim).
pub fn wire_dimmed() -> Style {
    base(Color::Rgb(28, 28, 36))
}

pub fn menu_border() -> Style {
    raised(GREY_BRIGHT)
}

pub fn menu_title() -> Style {
    raised(WHITE).add_modifier(Modifier::BOLD)
}

pub fn menu_item() -> Style {
    raised(WHITE)
}

pub fn menu_selected() -> Style {
    focus_row()
}

pub fn menu_value() -> Style {
    raised(GREY)
}

pub fn menu_clear() -> Style {
    Style::default().bg(BG_RAISED).fg(WHITE)
}

pub fn status_primary(pending_arm: bool) -> Style {
    if pending_arm {
        pending()
    } else {
        base(WHITE)
    }
}

pub fn status_secondary() -> Style {
    quiet()
}

pub fn status_rule() -> Style {
    base(GREY_DEEP)
}

pub fn banner_title() -> Style {
    base(WHITE).add_modifier(Modifier::BOLD)
}

pub fn banner_right() -> Style {
    base(GREY)
}

pub fn banner_rule() -> Style {
    base(GREY_DEEP)
}

pub fn param_value() -> Style {
    raised(PRIMARY).add_modifier(Modifier::BOLD)
}

pub fn param_bar_fill() -> Style {
    raised(PRIMARY)
}

pub fn param_bar_empty() -> Style {
    raised(GREY_DEEP)
}
