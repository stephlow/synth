//! Designed modular module panel: chassis + faceplate (IN | KNOBS | OUT).

mod chrome;
mod jacks;
mod scope;

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

pub use chrome::{draw_chrome, PanelChromeView};
pub use jacks::{faceplate_row, header_row, rule_row, CenterCell, ColWidths, JackFace};

use crate::draw::style::{line_style, quiet, strong};

#[derive(Clone, Debug)]
pub struct PanelLine {
    pub text: String,
    pub focused: bool,
    pub highlight: bool,
}

#[derive(Clone, Debug)]
pub struct ModulePanelView {
    pub title: String,
    pub kind: String,
    pub focused: bool,
    pub inputs: Vec<JackFace>,
    pub outputs: Vec<JackFace>,
    pub body: Vec<PanelLine>,
    pub show_jacks: bool,
    /// When set, panel paints a full-area oscilloscope instead of a list.
    pub scope_samples: Option<Vec<f32>>,
}

pub fn draw_module_panel(frame: &mut Frame, area: Rect, view: &ModulePanelView) {
    let inner = draw_chrome(
        frame,
        area,
        PanelChromeView {
            title: &view.title,
            kind: &view.kind,
            focused: view.focused,
        },
    );
    if inner.width < 4 || inner.height == 0 {
        return;
    }

    // Scope owns the full inner widget rect.
    if let Some(samples) = &view.scope_samples {
        scope::draw_scope(frame, inner, samples, view.focused);
        return;
    }

    let w = inner.width as usize;
    let lines = if view.show_jacks {
        compose_faceplate(view, w)
    } else {
        compose_utility(view, w)
    };
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn compose_faceplate(view: &ModulePanelView, width: usize) -> Vec<Line<'static>> {
    let cols = ColWidths::from_total(width);
    let mut lines = Vec::new();

    // Faceplate legend
    lines.push(header_row(&cols));
    lines.push(rule_row(&cols));

    let n = view
        .inputs
        .len()
        .max(view.outputs.len())
        .max(view.body.len())
        .max(1);

    for i in 0..n {
        let left = view.inputs.get(i);
        let right = view.outputs.get(i);
        let center = view.body.get(i).map(|pl| CenterCell {
            text: pl.text.clone(),
            focused: pl.focused,
            highlight: pl.highlight,
        });
        lines.push(faceplate_row(left, center.as_ref(), right, &cols));
    }

    lines
}

fn compose_utility(view: &ModulePanelView, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Utility modules keep a clean list under a section band
    let band = match view.kind.as_str() {
        "add" => "── insert module ──",
        "keyboard" => "── keyboard ──",
        "scope" => "── scope ──",
        "meter" => "── level ──",
        "io" => "── io ──",
        _ => "── ──",
    };
    lines.push(Line::from(Span::styled(
        pad_center(band, width),
        strong(),
    )));

    if view.body.is_empty() {
        lines.push(Line::from(Span::styled(pad_right(" —", width), quiet())));
    } else {
        for pl in &view.body {
            lines.push(Line::from(Span::styled(
                pad_right(&pl.text, width),
                line_style(pl.focused, pl.highlight),
            )));
        }
    }
    lines
}

fn pad_right(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        s.chars().take(w).collect()
    } else {
        format!("{s}{}", " ".repeat(w - n))
    }
}

fn pad_center(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        s.chars().take(w).collect()
    } else {
        let left = (w - n) / 2;
        format!("{}{s}", " ".repeat(left))
    }
}

pub fn panel_height(view: &ModulePanelView) -> u16 {
    if view.scope_samples.is_some() {
        // Prefer a tall scope so the waveform has room (border included).
        14
    } else if view.show_jacks {
        let rows = view
            .inputs
            .len()
            .max(view.outputs.len())
            .max(view.body.len())
            .max(1);
        // header + rule + rows + border 2
        (2 + 2 + rows as u16).clamp(7, 28)
    } else {
        let body = view.body.len().max(1);
        // band + body + border
        (2 + 1 + body as u16).clamp(6, 28)
    }
}
