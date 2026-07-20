//! Modular faceplate:  IN jacks  │  knobs/faders  │  OUT jacks
//!
//! Deliberate hardware-panel silhouette. Column Spans keep focus reverse
//! on one cell only. No forced theme colors.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::draw::style::{focus_row, pending, primary, strong};

#[derive(Clone, Debug)]
pub struct JackFace {
    pub name: String,
    pub connected: bool,
    pub focused: bool,
    pub pending: bool,
    pub peer: String,
}

#[derive(Clone, Debug)]
pub struct CenterCell {
    pub text: String,
    pub focused: bool,
    pub highlight: bool,
}

pub struct ColWidths {
    pub left: usize,
    pub mid: usize,
    pub right: usize,
}

impl ColWidths {
    pub fn from_total(total: usize) -> Self {
        // Side rails + center knobs; scales with responsive panel width.
        let left = ((total * 26) / 100).clamp(9, 14);
        let right = ((total * 26) / 100).clamp(9, 14);
        // two ║ separators
        let mid = total.saturating_sub(left + right + 2).max(12);
        Self { left, mid, right }
    }
}

/// Column headers — part of the designed faceplate chrome.
pub fn header_row(w: &ColWidths) -> Line<'static> {
    Line::from(vec![
        Span::styled(pad_right(" ◀ IN", w.left), strong()),
        Span::styled("║", strong()),
        Span::styled(pad_center("· KNOBS ·", w.mid), strong()),
        Span::styled("║", strong()),
        Span::styled(pad_left("OUT ▶ ", w.right), strong()),
    ])
}

/// Hairline under the header (same columns, decorative).
pub fn rule_row(w: &ColWidths) -> Line<'static> {
    let sep = "║";
    Line::from(vec![
        Span::styled("─".repeat(w.left), primary()),
        Span::styled(sep, primary()),
        Span::styled("─".repeat(w.mid), primary()),
        Span::styled(sep, primary()),
        Span::styled("─".repeat(w.right), primary()),
    ])
}

pub fn faceplate_row(
    left: Option<&JackFace>,
    center: Option<&CenterCell>,
    right: Option<&JackFace>,
    w: &ColWidths,
) -> Line<'static> {
    let left_sp = match left {
        Some(j) => Span::styled(
            format_in_jack(j, w.left),
            cell_style(j.focused, false, j.connected),
        ),
        None => Span::styled(pad_right("", w.left), primary()),
    };

    let mid_sp = match center {
        Some(c) => {
            let st = if c.focused {
                focus_row()
            } else if c.highlight {
                pending()
            } else {
                primary()
            };
            Span::styled(fit_pad(&c.text, w.mid), st)
        }
        None => Span::styled(pad_center("·", w.mid), primary()),
    };

    let right_sp = match right {
        Some(j) => Span::styled(
            format_out_jack(j, w.right),
            cell_style(j.focused, j.pending, j.connected),
        ),
        None => Span::styled(pad_right("", w.right), primary()),
    };

    Line::from(vec![
        left_sp,
        Span::styled("║", primary()),
        mid_sp,
        Span::styled("║", primary()),
        right_sp,
    ])
}

fn cell_style(focused: bool, pending_arm: bool, connected: bool) -> Style {
    if focused {
        focus_row()
    } else if pending_arm {
        pending()
    } else if connected {
        strong()
    } else {
        primary()
    }
}

/// Left rail: socket then label; cable tick when patched.
fn format_in_jack(j: &JackFace, width: usize) -> String {
    let sock = if j.connected { "●┤" } else { "○ " };
    let name = truncate(&j.name, width.saturating_sub(sock.chars().count() + 1));
    let core = if j.focused {
        format!("›{sock}{name}")
    } else {
        format!(" {sock}{name}")
    };
    pad_right(&core, width)
}

/// Right rail: label then socket; pending uses ◎.
fn format_out_jack(j: &JackFace, width: usize) -> String {
    let sock = if j.pending {
        "├◎"
    } else if j.connected {
        "├●"
    } else {
        " ○"
    };
    let name = truncate(&j.name, width.saturating_sub(sock.chars().count() + 1));
    let core = if j.focused {
        format!("{name}{sock}‹")
    } else {
        format!("{name}{sock} ")
    };
    pad_left(&core, width)
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else if max == 1 {
        s.chars().take(1).collect()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

fn pad_right(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        s.chars().take(w).collect()
    } else {
        format!("{s}{}", " ".repeat(w - n))
    }
}

fn pad_left(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        s.chars().skip(n.saturating_sub(w)).collect()
    } else {
        format!("{}{s}", " ".repeat(w - n))
    }
}

fn pad_center(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        s.chars().take(w).collect()
    } else {
        let left = (w - n) / 2;
        let right = w - n - left;
        format!("{}{s}{}", " ".repeat(left), " ".repeat(right))
    }
}

fn fit_pad(s: &str, w: usize) -> String {
    pad_right(&truncate(s, w), w)
}
