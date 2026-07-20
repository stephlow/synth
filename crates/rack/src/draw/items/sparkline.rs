//! Pure sparkline — block characters, no color.

const BARS: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];

pub fn sparkline(samples: &[f32], max_cols: usize) -> String {
    samples
        .iter()
        .take(max_cols)
        .map(|s| {
            let i = ((s.abs().clamp(0.0, 1.0) * 7.0) as usize).min(7);
            BARS[i]
        })
        .collect()
}
