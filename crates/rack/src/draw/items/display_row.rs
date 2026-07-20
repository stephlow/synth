//! Scope / meter lines — flat, no neon.

use crate::draw::items::sparkline::sparkline;

pub enum DisplayKind {
    Scope { samples: Vec<f32> },
    Meter { peak: f32, voices: u32 },
    Empty,
}

pub struct DisplayRowView<'a> {
    pub mark: &'a str,
    pub kind: DisplayKind,
}

pub fn format_display_row(v: DisplayRowView<'_>) -> String {
    match v.kind {
        DisplayKind::Scope { samples } => {
            let spark = sparkline(&samples, 20);
            format!("{}{}", v.mark, spark)
        }
        DisplayKind::Meter { peak, voices } => {
            let bar = peak_bar(peak, 10);
            format!("{}{}  {:.2}   {} voices", v.mark, bar, peak, voices)
        }
        DisplayKind::Empty => format!("{}—", v.mark),
    }
}

fn peak_bar(peak: f32, width: usize) -> String {
    let t = peak.clamp(0.0, 1.0);
    let filled = ((t * width as f32).round() as usize).min(width);
    let mut s = String::new();
    for i in 0..width {
        s.push(if i < filled { '━' } else { '─' });
    }
    s
}
