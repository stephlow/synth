//! Themeable palette + semantic styles for the rack TUI.
//!
//! Draw call sites keep using helpers like `tile_body_hot()`. The active
//! palette is set once per frame via [`set_active`] (from `App`).

use std::cell::Cell;

use ratatui::style::{Color, Modifier, Style};

// ── Palette ─────────────────────────────────────────────────────────────────

/// Named color set. All semantic styles read from the active palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bg: Color,
    pub bg_raised: Color,
    pub grey_deep: Color,
    pub grey: Color,
    pub grey_bright: Color,
    pub text: Color,
    pub primary: Color,
    pub primary_dim: Color,
    pub primary_hot: Color,
    pub wire_dim: Color,
}

/// Built-in theme ids (menu + session state).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeId {
    Nocturne,
    Phosphor,
    Ember,
    Violet,
    Mono,
    Ice,
    Ocean,
    Rose,
    Crimson,
    Forest,
    Copper,
    Plasma,
    Paper,
    Indigo,
    Toxic,
}

impl ThemeId {
    pub const ALL: [ThemeId; 15] = [
        ThemeId::Nocturne,
        ThemeId::Phosphor,
        ThemeId::Ember,
        ThemeId::Violet,
        ThemeId::Mono,
        ThemeId::Ice,
        ThemeId::Ocean,
        ThemeId::Rose,
        ThemeId::Crimson,
        ThemeId::Forest,
        ThemeId::Copper,
        ThemeId::Plasma,
        ThemeId::Paper,
        ThemeId::Indigo,
        ThemeId::Toxic,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Nocturne => "Nocturne (cyan)",
            ThemeId::Phosphor => "Phosphor (green)",
            ThemeId::Ember => "Ember (amber)",
            ThemeId::Violet => "Violet",
            ThemeId::Mono => "Mono",
            ThemeId::Ice => "Ice",
            ThemeId::Ocean => "Ocean (teal)",
            ThemeId::Rose => "Rose",
            ThemeId::Crimson => "Crimson",
            ThemeId::Forest => "Forest",
            ThemeId::Copper => "Copper",
            ThemeId::Plasma => "Plasma",
            ThemeId::Paper => "Paper (light)",
            ThemeId::Indigo => "Indigo",
            ThemeId::Toxic => "Toxic",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            ThemeId::Nocturne => nocturne(),
            ThemeId::Phosphor => phosphor(),
            ThemeId::Ember => ember(),
            ThemeId::Violet => violet(),
            ThemeId::Mono => mono(),
            ThemeId::Ice => ice(),
            ThemeId::Ocean => ocean(),
            ThemeId::Rose => rose(),
            ThemeId::Crimson => crimson(),
            ThemeId::Forest => forest(),
            ThemeId::Copper => copper(),
            ThemeId::Plasma => plasma(),
            ThemeId::Paper => paper(),
            ThemeId::Indigo => indigo(),
            ThemeId::Toxic => toxic(),
        }
    }
}

/// Current default (historical rack look).
pub fn nocturne() -> Palette {
    Palette {
        bg: Color::Rgb(10, 10, 14),
        bg_raised: Color::Rgb(18, 18, 24),
        grey_deep: Color::Rgb(42, 42, 52),
        grey: Color::Rgb(100, 100, 115),
        grey_bright: Color::Rgb(165, 165, 180),
        text: Color::Rgb(232, 232, 240),
        primary: Color::Rgb(120, 200, 255),
        primary_dim: Color::Rgb(70, 130, 175),
        primary_hot: Color::Rgb(180, 230, 255),
        wire_dim: Color::Rgb(28, 28, 36),
    }
}

pub fn phosphor() -> Palette {
    Palette {
        bg: Color::Rgb(6, 10, 6),
        bg_raised: Color::Rgb(12, 20, 12),
        grey_deep: Color::Rgb(28, 48, 28),
        grey: Color::Rgb(70, 110, 70),
        grey_bright: Color::Rgb(140, 190, 140),
        text: Color::Rgb(200, 255, 200),
        primary: Color::Rgb(80, 230, 100),
        primary_dim: Color::Rgb(40, 140, 55),
        primary_hot: Color::Rgb(180, 255, 180),
        wire_dim: Color::Rgb(16, 28, 16),
    }
}

pub fn ember() -> Palette {
    Palette {
        bg: Color::Rgb(12, 8, 6),
        bg_raised: Color::Rgb(22, 14, 10),
        grey_deep: Color::Rgb(52, 36, 28),
        grey: Color::Rgb(120, 90, 70),
        grey_bright: Color::Rgb(190, 150, 120),
        text: Color::Rgb(255, 236, 210),
        primary: Color::Rgb(255, 160, 60),
        primary_dim: Color::Rgb(180, 100, 40),
        primary_hot: Color::Rgb(255, 210, 140),
        wire_dim: Color::Rgb(32, 20, 14),
    }
}

pub fn violet() -> Palette {
    Palette {
        bg: Color::Rgb(10, 8, 16),
        bg_raised: Color::Rgb(18, 14, 28),
        grey_deep: Color::Rgb(48, 40, 70),
        grey: Color::Rgb(110, 100, 140),
        grey_bright: Color::Rgb(170, 160, 200),
        text: Color::Rgb(240, 230, 255),
        primary: Color::Rgb(200, 120, 255),
        primary_dim: Color::Rgb(130, 70, 180),
        primary_hot: Color::Rgb(230, 180, 255),
        wire_dim: Color::Rgb(28, 22, 40),
    }
}

pub fn mono() -> Palette {
    Palette {
        bg: Color::Rgb(8, 8, 8),
        bg_raised: Color::Rgb(18, 18, 18),
        grey_deep: Color::Rgb(48, 48, 48),
        grey: Color::Rgb(110, 110, 110),
        grey_bright: Color::Rgb(170, 170, 170),
        text: Color::Rgb(235, 235, 235),
        primary: Color::Rgb(210, 210, 210),
        primary_dim: Color::Rgb(140, 140, 140),
        primary_hot: Color::Rgb(250, 250, 250),
        wire_dim: Color::Rgb(28, 28, 28),
    }
}

pub fn ice() -> Palette {
    Palette {
        bg: Color::Rgb(8, 12, 18),
        bg_raised: Color::Rgb(14, 20, 30),
        grey_deep: Color::Rgb(40, 55, 70),
        grey: Color::Rgb(100, 130, 155),
        grey_bright: Color::Rgb(160, 190, 210),
        text: Color::Rgb(230, 245, 255),
        primary: Color::Rgb(140, 220, 255),
        primary_dim: Color::Rgb(70, 140, 180),
        primary_hot: Color::Rgb(220, 245, 255),
        wire_dim: Color::Rgb(22, 30, 40),
    }
}

pub fn ocean() -> Palette {
    Palette {
        bg: Color::Rgb(6, 14, 16),
        bg_raised: Color::Rgb(10, 22, 26),
        grey_deep: Color::Rgb(30, 55, 60),
        grey: Color::Rgb(70, 120, 125),
        grey_bright: Color::Rgb(130, 180, 185),
        text: Color::Rgb(210, 245, 245),
        primary: Color::Rgb(40, 210, 200),
        primary_dim: Color::Rgb(25, 130, 125),
        primary_hot: Color::Rgb(140, 255, 245),
        wire_dim: Color::Rgb(14, 28, 30),
    }
}

pub fn rose() -> Palette {
    Palette {
        bg: Color::Rgb(14, 8, 12),
        bg_raised: Color::Rgb(24, 14, 20),
        grey_deep: Color::Rgb(55, 35, 48),
        grey: Color::Rgb(130, 95, 115),
        grey_bright: Color::Rgb(200, 150, 175),
        text: Color::Rgb(255, 230, 240),
        primary: Color::Rgb(255, 110, 170),
        primary_dim: Color::Rgb(180, 60, 110),
        primary_hot: Color::Rgb(255, 180, 210),
        wire_dim: Color::Rgb(32, 18, 26),
    }
}

pub fn crimson() -> Palette {
    Palette {
        bg: Color::Rgb(12, 6, 8),
        bg_raised: Color::Rgb(22, 10, 12),
        grey_deep: Color::Rgb(55, 28, 32),
        grey: Color::Rgb(130, 70, 75),
        grey_bright: Color::Rgb(190, 120, 125),
        text: Color::Rgb(255, 220, 220),
        primary: Color::Rgb(255, 70, 85),
        primary_dim: Color::Rgb(170, 40, 50),
        primary_hot: Color::Rgb(255, 150, 155),
        wire_dim: Color::Rgb(30, 14, 16),
    }
}

pub fn forest() -> Palette {
    Palette {
        bg: Color::Rgb(8, 12, 8),
        bg_raised: Color::Rgb(14, 22, 14),
        grey_deep: Color::Rgb(36, 52, 36),
        grey: Color::Rgb(80, 110, 80),
        grey_bright: Color::Rgb(140, 175, 140),
        text: Color::Rgb(220, 240, 215),
        primary: Color::Rgb(120, 200, 90),
        primary_dim: Color::Rgb(70, 130, 50),
        primary_hot: Color::Rgb(180, 240, 140),
        wire_dim: Color::Rgb(18, 28, 18),
    }
}

pub fn copper() -> Palette {
    Palette {
        bg: Color::Rgb(12, 10, 8),
        bg_raised: Color::Rgb(22, 18, 14),
        grey_deep: Color::Rgb(55, 45, 35),
        grey: Color::Rgb(130, 110, 85),
        grey_bright: Color::Rgb(190, 165, 130),
        text: Color::Rgb(255, 240, 215),
        primary: Color::Rgb(220, 140, 70),
        primary_dim: Color::Rgb(150, 90, 45),
        primary_hot: Color::Rgb(255, 190, 120),
        wire_dim: Color::Rgb(30, 24, 18),
    }
}

pub fn plasma() -> Palette {
    Palette {
        bg: Color::Rgb(10, 6, 14),
        bg_raised: Color::Rgb(18, 10, 24),
        grey_deep: Color::Rgb(50, 30, 65),
        grey: Color::Rgb(120, 90, 150),
        grey_bright: Color::Rgb(185, 150, 210),
        text: Color::Rgb(245, 230, 255),
        primary: Color::Rgb(255, 80, 200),
        primary_dim: Color::Rgb(160, 40, 140),
        primary_hot: Color::Rgb(100, 220, 255),
        wire_dim: Color::Rgb(28, 16, 36),
    }
}

/// Light ground — rare but useful for bright terminals.
pub fn paper() -> Palette {
    Palette {
        bg: Color::Rgb(242, 238, 228),
        bg_raised: Color::Rgb(255, 252, 245),
        grey_deep: Color::Rgb(180, 175, 165),
        grey: Color::Rgb(120, 115, 105),
        grey_bright: Color::Rgb(80, 75, 70),
        text: Color::Rgb(30, 28, 24),
        primary: Color::Rgb(30, 100, 160),
        primary_dim: Color::Rgb(50, 90, 130),
        primary_hot: Color::Rgb(10, 60, 120),
        wire_dim: Color::Rgb(200, 195, 185),
    }
}

pub fn indigo() -> Palette {
    Palette {
        bg: Color::Rgb(8, 8, 18),
        bg_raised: Color::Rgb(14, 14, 30),
        grey_deep: Color::Rgb(40, 40, 70),
        grey: Color::Rgb(95, 100, 150),
        grey_bright: Color::Rgb(150, 155, 200),
        text: Color::Rgb(225, 230, 255),
        primary: Color::Rgb(110, 140, 255),
        primary_dim: Color::Rgb(60, 80, 180),
        primary_hot: Color::Rgb(170, 190, 255),
        wire_dim: Color::Rgb(20, 20, 40),
    }
}

pub fn toxic() -> Palette {
    Palette {
        bg: Color::Rgb(8, 10, 4),
        bg_raised: Color::Rgb(14, 18, 6),
        grey_deep: Color::Rgb(40, 50, 20),
        grey: Color::Rgb(100, 120, 50),
        grey_bright: Color::Rgb(170, 200, 90),
        text: Color::Rgb(230, 255, 180),
        primary: Color::Rgb(200, 255, 40),
        primary_dim: Color::Rgb(120, 160, 20),
        primary_hot: Color::Rgb(240, 255, 140),
        wire_dim: Color::Rgb(20, 26, 10),
    }
}

// Active palette for this draw frame (set from App).
thread_local! {
    static ACTIVE: Cell<Palette> = Cell::new(nocturne());
}

/// Install the palette used by all style helpers this frame.
pub fn set_active(p: Palette) {
    ACTIVE.with(|c| c.set(p));
}

pub fn palette() -> Palette {
    ACTIVE.with(|c| c.get())
}

/// Canvas / chrome ground color (tiles clear interior to this).
pub fn bg() -> Color {
    palette().bg
}

fn base(fg: Color) -> Style {
    let p = palette();
    Style::default().fg(fg).bg(p.bg)
}

fn raised(fg: Color) -> Style {
    let p = palette();
    Style::default().fg(fg).bg(p.bg_raised)
}

// ── Semantic styles ─────────────────────────────────────────────────────────

pub fn quiet() -> Style {
    base(palette().grey)
}

pub fn focus_row() -> Style {
    let p = palette();
    Style::default()
        .fg(p.bg)
        .bg(p.primary)
        .add_modifier(Modifier::BOLD)
}

pub fn pending() -> Style {
    base(palette().primary).add_modifier(Modifier::BOLD)
}

pub fn canvas_bg() -> Style {
    let p = palette();
    Style::default().bg(p.bg).fg(p.grey_deep)
}

pub fn lattice_dot() -> Style {
    base(palette().grey_deep)
}

pub fn tile_label_idle() -> Style {
    base(palette().grey_deep)
}

pub fn tile_label_soft() -> Style {
    base(palette().grey)
}

pub fn tile_label_hot() -> Style {
    base(palette().grey_bright)
}

pub fn tile_label_armed() -> Style {
    base(palette().primary_dim)
}

pub fn tile_border_idle() -> Style {
    base(palette().grey_deep)
}

pub fn tile_border_soft() -> Style {
    base(palette().grey)
}

pub fn tile_border_hot() -> Style {
    base(palette().primary_dim)
}

pub fn tile_border_armed() -> Style {
    base(palette().primary).add_modifier(Modifier::BOLD)
}

pub fn tile_body_idle() -> Style {
    base(palette().grey_deep)
}

pub fn tile_body_soft() -> Style {
    base(palette().grey)
}

pub fn tile_body_hot() -> Style {
    base(palette().primary).add_modifier(Modifier::BOLD)
}

pub fn tile_body_armed() -> Style {
    base(palette().primary_hot).add_modifier(Modifier::BOLD)
}

pub fn cursor_style() -> Style {
    base(palette().text).add_modifier(Modifier::BOLD)
}

pub fn wire_idle() -> Style {
    base(palette().grey_deep)
}

pub fn wire_soft() -> Style {
    base(palette().grey)
}

pub fn wire_active() -> Style {
    base(palette().primary).add_modifier(Modifier::BOLD)
}

pub fn wire_hover() -> Style {
    base(palette().primary_hot).add_modifier(Modifier::BOLD)
}

pub fn wire_pending() -> Style {
    base(palette().primary_dim).add_modifier(Modifier::BOLD)
}

pub fn wire_dimmed() -> Style {
    base(palette().wire_dim)
}

pub fn menu_border() -> Style {
    raised(palette().grey_bright)
}

pub fn menu_title() -> Style {
    raised(palette().text).add_modifier(Modifier::BOLD)
}

pub fn menu_item() -> Style {
    raised(palette().text)
}

pub fn menu_selected() -> Style {
    focus_row()
}

pub fn menu_value() -> Style {
    raised(palette().grey)
}

pub fn menu_clear() -> Style {
    let p = palette();
    Style::default().bg(p.bg_raised).fg(p.text)
}

pub fn status_primary(pending_arm: bool) -> Style {
    if pending_arm {
        pending()
    } else {
        base(palette().text)
    }
}

pub fn status_secondary() -> Style {
    quiet()
}

pub fn status_rule() -> Style {
    base(palette().grey_deep)
}

pub fn banner_title() -> Style {
    base(palette().text).add_modifier(Modifier::BOLD)
}

pub fn banner_right() -> Style {
    base(palette().grey)
}

pub fn banner_rule() -> Style {
    base(palette().grey_deep)
}

pub fn param_value() -> Style {
    raised(palette().primary).add_modifier(Modifier::BOLD)
}

pub fn param_bar_fill() -> Style {
    raised(palette().primary)
}

pub fn param_bar_empty() -> Style {
    raised(palette().grey_deep)
}
