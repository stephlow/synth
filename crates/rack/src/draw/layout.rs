//! Pure grid geometry: responsive panel size + viewport pagination.

use ratatui::layout::Rect;

use crate::draw::panel::{panel_height, ModulePanelView};
use crate::model::{PANEL_GAP_MAX, PANEL_GAP_MIN, PANEL_WIDTH_MAX, PANEL_WIDTH_MIN};

/// Result of packing modules into the available terminal region.
#[derive(Clone, Debug)]
pub struct GridLayout {
    pub panel_w: u16,
    pub gap: u16,
    pub cols: usize,
    /// Inclusive start, exclusive end into the panels slice.
    pub visible_start: usize,
    pub visible_end: usize,
    pub page: usize,
    pub pages: usize,
    pub total: usize,
}

impl GridLayout {
    pub fn visible_range(&self) -> std::ops::Range<usize> {
        self.visible_start..self.visible_end
    }

    pub fn visible_count(&self) -> usize {
        self.visible_end.saturating_sub(self.visible_start)
    }
}

/// Compute columns and panel width for `area` (grid paint region only).
pub fn compute_geometry(area: Rect) -> (u16, u16, usize) {
    let margin = 2u16; // left+right outer margin total
    let usable = area.width.saturating_sub(margin).max(PANEL_WIDTH_MIN);

    // Start with min width to see how many columns fit.
    let mut gap = PANEL_GAP_MIN;
    let mut cols = (usable / (PANEL_WIDTH_MIN + gap)).max(1) as usize;

    // Prefer slightly fewer columns and wider panels when we have room.
    if usable >= (PANEL_WIDTH_MIN + gap) * 3 + 8 {
        // leave room for gaps between
        cols = cols.min(3);
    }
    if usable < PANEL_WIDTH_MIN * 2 + gap {
        cols = 1;
    }

    // Distribute width: panel_w * cols + gap * (cols+1) ≈ usable
    // (gaps as side margins + between)
    let gaps = (cols as u16).saturating_add(1);
    let mut panel_w = usable
        .saturating_sub(gap * gaps)
        .checked_div(cols as u16)
        .unwrap_or(PANEL_WIDTH_MIN)
        .clamp(PANEL_WIDTH_MIN, PANEL_WIDTH_MAX);

    // Recompute gap from leftover so spacing is even.
    let used_panels = panel_w * cols as u16;
    let leftover = usable.saturating_sub(used_panels);
    gap = (leftover / gaps).clamp(PANEL_GAP_MIN, PANEL_GAP_MAX);

    // If leftover is large, grow panels toward max.
    let leftover2 = usable.saturating_sub(panel_w * cols as u16 + gap * gaps);
    if leftover2 > 0 && panel_w < PANEL_WIDTH_MAX {
        let grow = (leftover2 / cols as u16).min(PANEL_WIDTH_MAX - panel_w);
        panel_w += grow;
    }

    // Final cols re-check for very wide: allow 4 if each still ≥ min
    let max_cols = (usable / (PANEL_WIDTH_MIN + PANEL_GAP_MIN)).max(1) as usize;
    cols = cols.min(max_cols).max(1);

    (panel_w, gap, cols)
}

/// How many modules fit starting at `origin`, given heights.
pub fn compute_grid(
    area: Rect,
    panels: &[ModulePanelView],
    origin: usize,
) -> GridLayout {
    let total = panels.len();
    if total == 0 || area.height == 0 || area.width == 0 {
        return GridLayout {
            panel_w: PANEL_WIDTH_MIN,
            gap: PANEL_GAP_MIN,
            cols: 1,
            visible_start: 0,
            visible_end: 0,
            page: 0,
            pages: 1,
            total: 0,
        };
    }

    let (panel_w, gap, cols) = compute_geometry(area);
    let origin = origin.min(total.saturating_sub(1));

    // Pack rows from origin until height exhausted.
    let mut end = origin;
    let mut y_used = 0u16;
    let max_h = area.height;

    while end < total {
        let row_end = (end + cols).min(total);
        let row_h = panels[end..row_end]
            .iter()
            .map(panel_height)
            .max()
            .unwrap_or(6)
            .min(max_h)
            .saturating_add(gap);

        if end > origin && y_used.saturating_add(row_h) > max_h {
            break;
        }
        // Always take at least one row even if tall.
        y_used = y_used.saturating_add(row_h);
        end = row_end;
        if y_used >= max_h {
            break;
        }
    }

    if end == origin {
        end = (origin + 1).min(total);
    }

    // Page count: greedy pack from 0.
    let pages = count_pages(panels, cols, gap, max_h).max(1);
    let page = page_index_for_origin(panels, cols, gap, max_h, origin).min(pages - 1);

    GridLayout {
        panel_w,
        gap,
        cols,
        visible_start: origin,
        visible_end: end,
        page,
        pages,
        total,
    }
}

fn count_pages(panels: &[ModulePanelView], cols: usize, gap: u16, max_h: u16) -> usize {
    if panels.is_empty() {
        return 1;
    }
    let mut pages = 0;
    let mut i = 0;
    while i < panels.len() {
        let mut y = 0u16;
        let start = i;
        while i < panels.len() {
            let row_end = (i + cols).min(panels.len());
            let row_h = panels[i..row_end]
                .iter()
                .map(panel_height)
                .max()
                .unwrap_or(6)
                .min(max_h)
                .saturating_add(gap);
            if i > start && y.saturating_add(row_h) > max_h {
                break;
            }
            y = y.saturating_add(row_h);
            i = row_end;
            if y >= max_h {
                break;
            }
        }
        if i == start {
            i += 1;
        }
        pages += 1;
    }
    pages.max(1)
}

fn page_index_for_origin(
    panels: &[ModulePanelView],
    cols: usize,
    gap: u16,
    max_h: u16,
    origin: usize,
) -> usize {
    if panels.is_empty() {
        return 0;
    }
    let mut page = 0;
    let mut i = 0;
    while i < panels.len() {
        if i == origin {
            return page;
        }
        let start = i;
        let mut y = 0u16;
        while i < panels.len() {
            let row_end = (i + cols).min(panels.len());
            let row_h = panels[i..row_end]
                .iter()
                .map(panel_height)
                .max()
                .unwrap_or(6)
                .min(max_h)
                .saturating_add(gap);
            if i > start && y.saturating_add(row_h) > max_h {
                break;
            }
            y = y.saturating_add(row_h);
            i = row_end;
            if y >= max_h {
                break;
            }
        }
        if i == start {
            i += 1;
        }
        if origin < i {
            return page;
        }
        page += 1;
    }
    page.saturating_sub(1)
}

/// Advance origin by one page (±1). Returns new origin.
pub fn page_origin(
    panels: &[ModulePanelView],
    area: Rect,
    origin: usize,
    delta: isize,
) -> usize {
    if panels.is_empty() {
        return 0;
    }
    let (panel_w, gap, cols) = compute_geometry(area);
    let _ = panel_w;
    let max_h = area.height;

    // Collect page starts
    let mut starts = Vec::new();
    let mut i = 0;
    while i < panels.len() {
        starts.push(i);
        let start = i;
        let mut y = 0u16;
        while i < panels.len() {
            let row_end = (i + cols).min(panels.len());
            let row_h = panels[i..row_end]
                .iter()
                .map(panel_height)
                .max()
                .unwrap_or(6)
                .min(max_h)
                .saturating_add(gap);
            if i > start && y.saturating_add(row_h) > max_h {
                break;
            }
            y = y.saturating_add(row_h);
            i = row_end;
            if y >= max_h {
                break;
            }
        }
        if i == start {
            i += 1;
        }
    }

    if starts.is_empty() {
        return 0;
    }

    // Current page = largest start <= origin
    let mut cur = 0;
    for (pi, &s) in starts.iter().enumerate() {
        if s <= origin {
            cur = pi;
        }
    }
    let next = (cur as isize + delta).clamp(0, starts.len() as isize - 1) as usize;
    starts[next]
}

/// Shift origin so `focus` is within the visible window.
pub fn origin_for_focus(
    panels: &[ModulePanelView],
    area: Rect,
    focus: usize,
    current_origin: usize,
) -> usize {
    if panels.is_empty() {
        return 0;
    }
    let focus = focus.min(panels.len() - 1);
    let layout = compute_grid(area, panels, current_origin);
    if focus >= layout.visible_start && focus < layout.visible_end {
        return current_origin;
    }
    // If focus is before window, set origin so focus is first on a page containing it.
    // Walk page starts and pick the page that contains focus.
    let (panel_w, gap, cols) = compute_geometry(area);
    let _ = panel_w;
    let max_h = area.height;
    let mut i = 0;
    let mut best = 0;
    while i < panels.len() {
        let start = i;
        let mut y = 0u16;
        while i < panels.len() {
            let row_end = (i + cols).min(panels.len());
            let row_h = panels[i..row_end]
                .iter()
                .map(panel_height)
                .max()
                .unwrap_or(6)
                .min(max_h)
                .saturating_add(gap);
            if i > start && y.saturating_add(row_h) > max_h {
                break;
            }
            y = y.saturating_add(row_h);
            i = row_end;
            if y >= max_h {
                break;
            }
        }
        if i == start {
            i += 1;
        }
        if focus >= start && focus < i {
            return start;
        }
        if focus >= start {
            best = start;
        }
    }
    best
}

/// Grid area from full terminal size (banner 2 + status 3).
pub fn grid_area_from_term(width: u16, height: u16) -> Rect {
    let banner = 2u16;
    let status = 3u16;
    let h = height.saturating_sub(banner + status).max(1);
    Rect {
        x: 0,
        y: banner,
        width,
        height: h,
    }
}
