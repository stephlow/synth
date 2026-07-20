//! Lattice placement: auto-layout for new modules, free-cell helpers.

use std::collections::HashSet;

use crate::model::{LatticePos, ModuleId, PlaceKey, UiModule};

/// Absolute minimum lattice size on tiny terminals.
pub const MIN_COLS: u16 = 8;
pub const MIN_ROWS: u16 = 6;
/// Default virtual canvas — room to organize a full patch.
pub const DEFAULT_GRID_COLS: u16 = 32;
pub const DEFAULT_GRID_ROWS: u16 = 20;

/// Voice modules pack loosely (every other column); utilities on a lower strip.
/// Used only as preferred seeds for modules without saved coordinates.
pub fn layout_modules(modules: &[UiModule], pack_cols: u16) -> Vec<Option<LatticePos>> {
    let mut positions = vec![None; modules.len()];

    let mut voices = Vec::new();
    let mut utils = Vec::new();
    for (i, m) in modules.iter().enumerate() {
        match &m.id {
            ModuleId::Voice(_) => voices.push(i),
            ModuleId::Io(_) | ModuleId::Scope | ModuleId::Meter | ModuleId::Keyboard => {
                utils.push(i);
            }
        }
    }

    // Spread out: leave empty cells between modules for cables / room to arrange.
    let voice_cols = pack_cols.clamp(2, 12);
    for (n, &idx) in voices.iter().enumerate() {
        let col = ((n as u16) % voice_cols) * 2;
        let row = ((n as u16) / voice_cols) * 2;
        positions[idx] = Some(LatticePos { x: col, y: row });
    }

    let voice_rows = if voices.is_empty() {
        0
    } else {
        (((voices.len() as u16) + voice_cols - 1) / voice_cols) * 2
    };
    let util_row = voice_rows.saturating_add(2).max(4);

    for (n, &idx) in utils.iter().enumerate() {
        let col = (n as u16) * 2;
        positions[idx] = Some(LatticePos {
            x: col,
            y: util_row,
        });
    }

    positions
}

/// Bounding lattice size from placements, expanded to a large default canvas.
pub fn lattice_bounds(
    positions: &[Option<LatticePos>],
    viewport_cols: u16,
    viewport_rows: u16,
    canvas_w: Option<u16>,
    canvas_h: Option<u16>,
) -> (u16, u16) {
    let mut max_c = 0u16;
    let mut max_r = 0u16;
    for p in positions.iter().flatten() {
        max_c = max_c.max(p.x);
        max_r = max_r.max(p.y);
    }
    let cols = (max_c + 4)
        .max(MIN_COLS)
        .max(DEFAULT_GRID_COLS)
        .max(viewport_cols)
        .max(canvas_w.unwrap_or(0));
    let rows = (max_r + 3)
        .max(MIN_ROWS)
        .max(DEFAULT_GRID_ROWS)
        .max(viewport_rows)
        .max(canvas_h.unwrap_or(0));
    (cols, rows)
}

/// Module index at lattice cell, if any.
pub fn module_at(positions: &[Option<LatticePos>], pos: LatticePos) -> Option<usize> {
    positions.iter().position(|p| *p == Some(pos))
}

/// Suggested voice pack columns from canvas width and cell step.
pub fn pack_cols_for(canvas_w: u16, cell_step_x: u16) -> u16 {
    let fit = (canvas_w / cell_step_x.max(1)).max(1);
    match fit {
        1..=2 => 1,
        3..=4 => 2,
        5..=7 => 3,
        8..=11 => 4,
        12..=15 => 5,
        _ => 6,
    }
}

/// First free cell near `prefer`, scanning outward in a spiral-ish row-major ring.
pub fn first_free_near(
    occupied: &HashSet<LatticePos>,
    prefer: LatticePos,
    cols: u16,
    rows: u16,
) -> LatticePos {
    if !occupied.contains(&prefer) && prefer.x < cols && prefer.y < rows {
        return prefer;
    }
    let max_r = cols.max(rows).saturating_add(8) as i32;
    for radius in 0..=max_r {
        for dr in -radius..=radius {
            for dc in -radius..=radius {
                if dr.abs() != radius && dc.abs() != radius {
                    continue;
                }
                let c = prefer.x as i32 + dc;
                let r = prefer.y as i32 + dr;
                if c < 0 || r < 0 {
                    continue;
                }
                let pos = LatticePos {
                    x: c as u16,
                    y: r as u16,
                };
                if !occupied.contains(&pos) {
                    return pos;
                }
            }
        }
    }
    LatticePos {
        x: 0,
        y: rows.saturating_add(1),
    }
}

pub fn place_key(m: &UiModule) -> PlaceKey {
    PlaceKey::from_module_id(&m.id)
}
