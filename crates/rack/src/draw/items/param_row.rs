//! Center-column knob / fader — designed meter, not raw ASCII.

pub struct ParamRowView<'a> {
    pub mark: &'a str,
    pub name: &'a str,
    pub value: f32,
    pub min: f32,
    pub max: f32,
}

/// Text sized for the KNOBS column.
pub fn format_param_row(v: ParamRowView<'_>) -> String {
    let range = (v.max - v.min).abs().max(1e-6);
    let t = ((v.value - v.min) / range).clamp(0.0, 1.0);
    let bar = fader_bar(t, 6);
    let name = if v.name.chars().count() > 7 {
        let mut s: String = v.name.chars().take(6).collect();
        s.push('·');
        format!("{s:<7}")
    } else {
        format!("{:<7}", v.name)
    };
    // mark: "›" or " " (single char for density in center)
    let m = if v.mark.starts_with('›') { "›" } else { " " };
    format!("{m}{name}{bar}{:>5.2}", v.value)
}

/// Vertical-ish fader silhouette using block elements.
fn fader_bar(t: f32, width: usize) -> String {
    let filled = ((t * width as f32).round() as usize).min(width);
    let mut s = String::from("[");
    for i in 0..width {
        if i < filled {
            s.push('█');
        } else {
            s.push('·');
        }
    }
    s.push(']');
    s
}
