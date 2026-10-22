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
    Aurora,
    Midnight,
    Dusk,
    Sunset,
    Fog,
    Graphite,
    Sand,
    Jade,
    Magenta,
    Honey,
    Storm,
    Wine,
    Cobalt,
    Lava,
    Bone,
    Ink,
    Moss,
    Steel,
    Peach,
    Void,
}

impl ThemeId {
    pub const ALL: [ThemeId; 35] = [
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
        ThemeId::Aurora,
        ThemeId::Midnight,
        ThemeId::Dusk,
        ThemeId::Sunset,
        ThemeId::Fog,
        ThemeId::Graphite,
        ThemeId::Sand,
        ThemeId::Jade,
        ThemeId::Magenta,
        ThemeId::Honey,
        ThemeId::Storm,
        ThemeId::Wine,
        ThemeId::Cobalt,
        ThemeId::Lava,
        ThemeId::Bone,
        ThemeId::Ink,
        ThemeId::Moss,
        ThemeId::Steel,
        ThemeId::Peach,
        ThemeId::Void,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Nocturne => "Nocturne",
            ThemeId::Phosphor => "Phosphor",
            ThemeId::Ember => "Ember",
            ThemeId::Violet => "Violet",
            ThemeId::Mono => "Mono",
            ThemeId::Ice => "Ice",
            ThemeId::Ocean => "Ocean",
            ThemeId::Rose => "Rose",
            ThemeId::Crimson => "Crimson",
            ThemeId::Forest => "Forest",
            ThemeId::Copper => "Copper",
            ThemeId::Plasma => "Plasma",
            ThemeId::Paper => "Paper",
            ThemeId::Indigo => "Indigo",
            ThemeId::Toxic => "Toxic",
            ThemeId::Aurora => "Aurora",
            ThemeId::Midnight => "Midnight",
            ThemeId::Dusk => "Dusk",
            ThemeId::Sunset => "Sunset",
            ThemeId::Fog => "Fog",
            ThemeId::Graphite => "Graphite",
            ThemeId::Sand => "Sand",
            ThemeId::Jade => "Jade",
            ThemeId::Magenta => "Magenta",
            ThemeId::Honey => "Honey",
            ThemeId::Storm => "Storm",
            ThemeId::Wine => "Wine",
            ThemeId::Cobalt => "Cobalt",
            ThemeId::Lava => "Lava",
            ThemeId::Bone => "Bone",
            ThemeId::Ink => "Ink",
            ThemeId::Moss => "Moss",
            ThemeId::Steel => "Steel",
            ThemeId::Peach => "Peach",
            ThemeId::Void => "Void",
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
            ThemeId::Aurora => aurora(),
            ThemeId::Midnight => midnight(),
            ThemeId::Dusk => dusk(),
            ThemeId::Sunset => sunset(),
            ThemeId::Fog => fog(),
            ThemeId::Graphite => graphite(),
            ThemeId::Sand => sand(),
            ThemeId::Jade => jade(),
            ThemeId::Magenta => magenta(),
            ThemeId::Honey => honey(),
            ThemeId::Storm => storm(),
            ThemeId::Wine => wine(),
            ThemeId::Cobalt => cobalt(),
            ThemeId::Lava => lava(),
            ThemeId::Bone => bone(),
            ThemeId::Ink => ink(),
            ThemeId::Moss => moss(),
            ThemeId::Steel => steel(),
            ThemeId::Peach => peach(),
            ThemeId::Void => void_theme(),
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

pub fn aurora() -> Palette {
    Palette {
        bg: Color::Rgb(6, 12, 12),
        bg_raised: Color::Rgb(10, 20, 20),
        grey_deep: Color::Rgb(28, 55, 55),
        grey: Color::Rgb(70, 125, 120),
        grey_bright: Color::Rgb(130, 190, 180),
        text: Color::Rgb(210, 250, 240),
        primary: Color::Rgb(60, 230, 180),
        primary_dim: Color::Rgb(35, 140, 120),
        primary_hot: Color::Rgb(160, 255, 220),
        wire_dim: Color::Rgb(14, 26, 26),
    }
}

pub fn midnight() -> Palette {
    Palette {
        bg: Color::Rgb(6, 8, 16),
        bg_raised: Color::Rgb(12, 16, 28),
        grey_deep: Color::Rgb(32, 42, 70),
        grey: Color::Rgb(80, 100, 150),
        grey_bright: Color::Rgb(140, 160, 210),
        text: Color::Rgb(220, 230, 255),
        primary: Color::Rgb(100, 150, 255),
        primary_dim: Color::Rgb(55, 90, 180),
        primary_hot: Color::Rgb(170, 200, 255),
        wire_dim: Color::Rgb(16, 20, 36),
    }
}

pub fn dusk() -> Palette {
    Palette {
        bg: Color::Rgb(12, 10, 16),
        bg_raised: Color::Rgb(20, 16, 28),
        grey_deep: Color::Rgb(50, 42, 65),
        grey: Color::Rgb(115, 100, 140),
        grey_bright: Color::Rgb(175, 155, 200),
        text: Color::Rgb(235, 225, 245),
        primary: Color::Rgb(170, 130, 220),
        primary_dim: Color::Rgb(110, 80, 155),
        primary_hot: Color::Rgb(210, 180, 250),
        wire_dim: Color::Rgb(28, 24, 36),
    }
}

pub fn sunset() -> Palette {
    Palette {
        bg: Color::Rgb(14, 8, 10),
        bg_raised: Color::Rgb(24, 14, 16),
        grey_deep: Color::Rgb(60, 35, 40),
        grey: Color::Rgb(140, 90, 95),
        grey_bright: Color::Rgb(210, 150, 145),
        text: Color::Rgb(255, 230, 220),
        primary: Color::Rgb(255, 120, 90),
        primary_dim: Color::Rgb(180, 70, 60),
        primary_hot: Color::Rgb(255, 180, 150),
        wire_dim: Color::Rgb(32, 18, 20),
    }
}

pub fn fog() -> Palette {
    Palette {
        bg: Color::Rgb(14, 16, 18),
        bg_raised: Color::Rgb(22, 24, 28),
        grey_deep: Color::Rgb(48, 52, 58),
        grey: Color::Rgb(110, 118, 128),
        grey_bright: Color::Rgb(165, 172, 182),
        text: Color::Rgb(220, 225, 230),
        primary: Color::Rgb(150, 175, 195),
        primary_dim: Color::Rgb(95, 115, 135),
        primary_hot: Color::Rgb(200, 215, 230),
        wire_dim: Color::Rgb(28, 30, 34),
    }
}

pub fn graphite() -> Palette {
    Palette {
        bg: Color::Rgb(10, 11, 12),
        bg_raised: Color::Rgb(20, 21, 23),
        grey_deep: Color::Rgb(50, 52, 56),
        grey: Color::Rgb(115, 118, 125),
        grey_bright: Color::Rgb(175, 178, 185),
        text: Color::Rgb(230, 232, 236),
        primary: Color::Rgb(185, 190, 200),
        primary_dim: Color::Rgb(120, 125, 135),
        primary_hot: Color::Rgb(240, 242, 248),
        wire_dim: Color::Rgb(30, 32, 34),
    }
}

/// Warm light ground.
pub fn sand() -> Palette {
    Palette {
        bg: Color::Rgb(240, 230, 210),
        bg_raised: Color::Rgb(252, 244, 228),
        grey_deep: Color::Rgb(185, 170, 145),
        grey: Color::Rgb(130, 115, 95),
        grey_bright: Color::Rgb(90, 78, 65),
        text: Color::Rgb(40, 32, 24),
        primary: Color::Rgb(160, 90, 40),
        primary_dim: Color::Rgb(130, 80, 45),
        primary_hot: Color::Rgb(100, 55, 20),
        wire_dim: Color::Rgb(210, 198, 175),
    }
}

pub fn jade() -> Palette {
    Palette {
        bg: Color::Rgb(6, 12, 10),
        bg_raised: Color::Rgb(12, 22, 18),
        grey_deep: Color::Rgb(30, 55, 48),
        grey: Color::Rgb(70, 120, 105),
        grey_bright: Color::Rgb(130, 185, 165),
        text: Color::Rgb(210, 245, 230),
        primary: Color::Rgb(70, 210, 160),
        primary_dim: Color::Rgb(40, 130, 100),
        primary_hot: Color::Rgb(150, 245, 200),
        wire_dim: Color::Rgb(14, 26, 22),
    }
}

pub fn magenta() -> Palette {
    Palette {
        bg: Color::Rgb(12, 6, 14),
        bg_raised: Color::Rgb(22, 10, 24),
        grey_deep: Color::Rgb(55, 30, 60),
        grey: Color::Rgb(130, 80, 140),
        grey_bright: Color::Rgb(200, 140, 210),
        text: Color::Rgb(255, 230, 255),
        primary: Color::Rgb(255, 60, 200),
        primary_dim: Color::Rgb(170, 30, 140),
        primary_hot: Color::Rgb(255, 150, 230),
        wire_dim: Color::Rgb(30, 16, 34),
    }
}

pub fn honey() -> Palette {
    Palette {
        bg: Color::Rgb(12, 10, 6),
        bg_raised: Color::Rgb(22, 18, 10),
        grey_deep: Color::Rgb(55, 48, 28),
        grey: Color::Rgb(130, 115, 70),
        grey_bright: Color::Rgb(195, 175, 110),
        text: Color::Rgb(255, 245, 200),
        primary: Color::Rgb(240, 190, 50),
        primary_dim: Color::Rgb(170, 130, 30),
        primary_hot: Color::Rgb(255, 230, 120),
        wire_dim: Color::Rgb(30, 26, 14),
    }
}

pub fn storm() -> Palette {
    Palette {
        bg: Color::Rgb(10, 12, 16),
        bg_raised: Color::Rgb(18, 20, 26),
        grey_deep: Color::Rgb(42, 48, 60),
        grey: Color::Rgb(100, 110, 130),
        grey_bright: Color::Rgb(155, 165, 185),
        text: Color::Rgb(225, 230, 240),
        primary: Color::Rgb(120, 155, 200),
        primary_dim: Color::Rgb(70, 100, 140),
        primary_hot: Color::Rgb(180, 205, 235),
        wire_dim: Color::Rgb(24, 28, 34),
    }
}

pub fn wine() -> Palette {
    Palette {
        bg: Color::Rgb(12, 6, 10),
        bg_raised: Color::Rgb(22, 10, 16),
        grey_deep: Color::Rgb(55, 28, 42),
        grey: Color::Rgb(125, 70, 95),
        grey_bright: Color::Rgb(185, 120, 145),
        text: Color::Rgb(255, 220, 230),
        primary: Color::Rgb(200, 60, 110),
        primary_dim: Color::Rgb(140, 35, 75),
        primary_hot: Color::Rgb(240, 130, 165),
        wire_dim: Color::Rgb(30, 14, 22),
    }
}

pub fn cobalt() -> Palette {
    Palette {
        bg: Color::Rgb(6, 8, 18),
        bg_raised: Color::Rgb(12, 14, 30),
        grey_deep: Color::Rgb(30, 40, 75),
        grey: Color::Rgb(75, 95, 160),
        grey_bright: Color::Rgb(130, 150, 210),
        text: Color::Rgb(220, 230, 255),
        primary: Color::Rgb(60, 110, 255),
        primary_dim: Color::Rgb(35, 70, 180),
        primary_hot: Color::Rgb(140, 175, 255),
        wire_dim: Color::Rgb(14, 18, 40),
    }
}

pub fn lava() -> Palette {
    Palette {
        bg: Color::Rgb(10, 6, 4),
        bg_raised: Color::Rgb(20, 10, 6),
        grey_deep: Color::Rgb(55, 30, 20),
        grey: Color::Rgb(130, 75, 50),
        grey_bright: Color::Rgb(195, 125, 90),
        text: Color::Rgb(255, 225, 200),
        primary: Color::Rgb(255, 90, 30),
        primary_dim: Color::Rgb(180, 50, 20),
        primary_hot: Color::Rgb(255, 160, 80),
        wire_dim: Color::Rgb(28, 14, 10),
    }
}

/// Warm off-white light ground.
pub fn bone() -> Palette {
    Palette {
        bg: Color::Rgb(236, 232, 222),
        bg_raised: Color::Rgb(248, 245, 238),
        grey_deep: Color::Rgb(175, 170, 158),
        grey: Color::Rgb(115, 110, 100),
        grey_bright: Color::Rgb(75, 70, 62),
        text: Color::Rgb(28, 26, 22),
        primary: Color::Rgb(90, 70, 50),
        primary_dim: Color::Rgb(110, 95, 75),
        primary_hot: Color::Rgb(50, 40, 28),
        wire_dim: Color::Rgb(200, 195, 185),
    }
}

/// Near-black high contrast.
pub fn ink() -> Palette {
    Palette {
        bg: Color::Rgb(0, 0, 0),
        bg_raised: Color::Rgb(16, 16, 16),
        grey_deep: Color::Rgb(48, 48, 48),
        grey: Color::Rgb(120, 120, 120),
        grey_bright: Color::Rgb(180, 180, 180),
        text: Color::Rgb(255, 255, 255),
        primary: Color::Rgb(255, 255, 255),
        primary_dim: Color::Rgb(160, 160, 160),
        primary_hot: Color::Rgb(255, 255, 255),
        wire_dim: Color::Rgb(24, 24, 24),
    }
}

pub fn moss() -> Palette {
    Palette {
        bg: Color::Rgb(10, 12, 8),
        bg_raised: Color::Rgb(18, 22, 14),
        grey_deep: Color::Rgb(42, 50, 32),
        grey: Color::Rgb(100, 115, 80),
        grey_bright: Color::Rgb(155, 175, 125),
        text: Color::Rgb(225, 235, 205),
        primary: Color::Rgb(140, 170, 70),
        primary_dim: Color::Rgb(90, 115, 45),
        primary_hot: Color::Rgb(190, 220, 120),
        wire_dim: Color::Rgb(22, 28, 18),
    }
}

pub fn steel() -> Palette {
    Palette {
        bg: Color::Rgb(10, 12, 14),
        bg_raised: Color::Rgb(18, 20, 24),
        grey_deep: Color::Rgb(45, 52, 60),
        grey: Color::Rgb(105, 115, 128),
        grey_bright: Color::Rgb(160, 170, 185),
        text: Color::Rgb(230, 235, 242),
        primary: Color::Rgb(140, 165, 190),
        primary_dim: Color::Rgb(85, 105, 130),
        primary_hot: Color::Rgb(190, 210, 230),
        wire_dim: Color::Rgb(26, 30, 34),
    }
}

pub fn peach() -> Palette {
    Palette {
        bg: Color::Rgb(16, 10, 10),
        bg_raised: Color::Rgb(28, 16, 16),
        grey_deep: Color::Rgb(60, 40, 40),
        grey: Color::Rgb(140, 105, 100),
        grey_bright: Color::Rgb(210, 165, 155),
        text: Color::Rgb(255, 235, 225),
        primary: Color::Rgb(255, 150, 120),
        primary_dim: Color::Rgb(180, 95, 80),
        primary_hot: Color::Rgb(255, 200, 175),
        wire_dim: Color::Rgb(34, 20, 20),
    }
}

pub fn void_theme() -> Palette {
    Palette {
        bg: Color::Rgb(4, 2, 8),
        bg_raised: Color::Rgb(10, 6, 16),
        grey_deep: Color::Rgb(32, 24, 48),
        grey: Color::Rgb(85, 70, 110),
        grey_bright: Color::Rgb(140, 120, 170),
        text: Color::Rgb(220, 210, 240),
        primary: Color::Rgb(140, 90, 200),
        primary_dim: Color::Rgb(85, 50, 130),
        primary_hot: Color::Rgb(190, 150, 240),
        wire_dim: Color::Rgb(12, 8, 22),
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
