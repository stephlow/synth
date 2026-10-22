//! Responsive lattice geometry: scales tile/gutter with canvas size.

use ratatui::layout::Rect;

use crate::model::LatticePos;

/// Per-frame geometry derived from the lattice canvas.
#[derive(Clone, Copy, Debug)]
pub struct Geom {
    pub tile_w: u16,
    pub tile_h: u16,
    pub gutter_x: u16,
    pub gutter_y: u16,
}

impl Geom {
    /// Scale tiles and gutters from available canvas size.
    ///
    /// Sized so kind-specific glyphs (sine/tri waves, ADSR, filter curves)
    /// get a real interior: after top micro-label + box borders, prefer
    /// roughly 9×3+ cells for the paint surface on a normal terminal.
    pub fn from_canvas(canvas: Rect) -> Self {
        let w = canvas.width;
        let h = canvas.height;

        // Outer tile width (chars). Inner glyph ≈ width − 2 borders.
        let tile_w = match w {
            0..=40 => 7,
            41..=70 => 9,
            71..=100 => 11,
            101..=140 => 13,
            _ => 15,
        };

        // Outer tile height (rows). Budget: 1 label + 2 borders + glyph rows.
        // Want ≥3 glyph rows on normal screens → outer ≥ 6.
        let tile_h = match h {
            0..=12 => 4,
            13..=20 => 5,
            21..=30 => 6,
            31..=42 => 7,
            _ => 8,
        };

        // Gutters: wire lanes between large tiles.
        let gutter_x = match w {
            0..=50 => 2,
            51..=90 => 3,
            91..=130 => 4,
            _ => 5,
        };
        let gutter_y = match h {
            0..=18 => 1,
            19..=32 => 1,
            _ => 2,
        };

        Self {
            tile_w,
            tile_h,
            gutter_x,
            gutter_y,
        }
    }

    pub fn cell_step_x(self) -> u16 {
        self.tile_w.saturating_add(self.gutter_x).max(1)
    }

    pub fn cell_step_y(self) -> u16 {
        self.tile_h.saturating_add(self.gutter_y).max(1)
    }

    /// How many lattice cells fit fully in the canvas.
    pub fn visible_span(self, canvas: Rect) -> (u16, u16) {
        let cols = (canvas.width / self.cell_step_x()).max(1);
        let rows = (canvas.height / self.cell_step_y()).max(1);
        (cols, rows)
    }
}

/// Lattice paint area from full terminal (responsive chrome heights).
pub fn chrome_heights(term_h: u16) -> (u16, u16) {
    // (banner, status)
    if term_h < 12 {
        (1, 2)
    } else if term_h < 18 {
        (2, 2)
    } else {
        (2, 3)
    }
}

pub fn grid_area_from_term(width: u16, height: u16) -> Rect {
    let (banner, status) = chrome_heights(height);
    let h = height.saturating_sub(banner + status).max(1);
    Rect {
        x: 0,
        y: banner,
        width,
        height: h,
    }
}

/// Character origin of lattice cell relative to canvas.
pub fn cell_origin(pos: LatticePos, view: LatticePos, canvas: Rect, geom: Geom) -> (u16, u16) {
    let dx = pos.x.saturating_sub(view.x);
    let dy = pos.y.saturating_sub(view.y);
    let x = canvas.x.saturating_add(dx.saturating_mul(geom.cell_step_x()));
    let y = canvas.y.saturating_add(dy.saturating_mul(geom.cell_step_y()));
    (x, y)
}

/// Tile rect for a lattice cell (clipped to canvas).
pub fn tile_rect(pos: LatticePos, view: LatticePos, canvas: Rect, geom: Geom) -> Option<Rect> {
    // Cells scrolled off the left/top must not clamp onto the first column/row.
    if pos.x < view.x || pos.y < view.y {
        return None;
    }
    let (x, y) = cell_origin(pos, view, canvas, geom);
    if x >= canvas.x + canvas.width || y >= canvas.y + canvas.height {
        return None;
    }
    let w = geom.tile_w.min(canvas.x + canvas.width - x);
    let h = geom.tile_h.min(canvas.y + canvas.height - y);
    // Need enough room for a bordered glyph (not just a 2×2 stub).
    if w < 5 || h < 3 {
        return None;
    }
    Some(Rect {
        x,
        y,
        width: w,
        height: h,
    })
}

pub fn tile_out_port(pos: LatticePos, view: LatticePos, canvas: Rect, geom: Geom) -> Option<(u16, u16)> {
    let r = tile_rect(pos, view, canvas, geom)?;
    Some((r.x + r.width.saturating_sub(1), r.y + r.height / 2))
}

pub fn tile_in_port(pos: LatticePos, view: LatticePos, canvas: Rect, geom: Geom) -> Option<(u16, u16)> {
    let r = tile_rect(pos, view, canvas, geom)?;
    Some((r.x, r.y + r.height / 2))
}

pub fn cursor_rect(pos: LatticePos, view: LatticePos, canvas: Rect, geom: Geom) -> Option<Rect> {
    tile_rect(pos, view, canvas, geom)
}

/// Menu panel: scale with canvas; prefer center.
pub fn menu_rect(canvas: Rect, content_rows: u16, content_cols: u16) -> Rect {
    let max_w = canvas.width.saturating_sub(2).max(12);
    let max_h = canvas.height.saturating_sub(1).max(3);
    // Prefer up to ~70% width on large terms, full-ish on small.
    let prefer_w = if canvas.width < 40 {
        max_w
    } else {
        (canvas.width * 7 / 10).clamp(20, max_w)
    };
    let w = content_cols.min(prefer_w).max(12).min(max_w);
    let h = content_rows.min(max_h).max(3);
    let x = canvas.x + canvas.width.saturating_sub(w) / 2;
    let y = canvas.y + canvas.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}
