//! Kind-specific audio symbols for module tile interiors.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

/// What to paint inside a module body.
#[derive(Clone, Copy, Debug)]
pub enum GlyphKind {
    Sine,
    Triangle,
    /// Available for future wave-shape variants / aliases.
    #[allow(dead_code)]
    Saw,
    #[allow(dead_code)]
    Square,
    Filter,
    Adsr,
    Vca,
    Mixer,
    Noise,
    SampleHold,
    Sequencer,
    Quantizer,
    Attenuverter,
    MidiCv,
    Scope,
    Meter,
    Keyboard,
    Io,
    Slew,
    ClockDiv,
    Delay,
    Wavefolder,
    Lpg,
    Function,
    RingMod,
    Generic,
}

impl GlyphKind {
    pub fn from_module_kind(kind: &str) -> Self {
        match kind.to_ascii_lowercase().as_str() {
            "vco" | "osc" => GlyphKind::Sine,
            "lfo" => GlyphKind::Triangle,
            "vcf" | "filter" => GlyphKind::Filter,
            "vca" => GlyphKind::Vca,
            "envelope" | "env" | "adsr" => GlyphKind::Adsr,
            "mixer" | "mix" => GlyphKind::Mixer,
            "noise" => GlyphKind::Noise,
            "sample_hold" | "sample-hold" | "s&h" => GlyphKind::SampleHold,
            "sequencer" | "seq" => GlyphKind::Sequencer,
            "quantizer" | "quant" => GlyphKind::Quantizer,
            "attenuverter" | "atv" => GlyphKind::Attenuverter,
            "midi_to_cv" | "m2cv" | "miditocv" => GlyphKind::MidiCv,
            "oscilloscope" | "scope" => GlyphKind::Scope,
            "meter" => GlyphKind::Meter,
            "keyboard" | "keys" => GlyphKind::Keyboard,
            "io" => GlyphKind::Io,
            "slew" | "lag" | "portamento" => GlyphKind::Slew,
            "clock_div" | "clock-div" | "divider" => GlyphKind::ClockDiv,
            "delay" => GlyphKind::Delay,
            "wavefolder" | "folder" | "fold" => GlyphKind::Wavefolder,
            "lpg" | "lowpass_gate" | "low-pass-gate" => GlyphKind::Lpg,
            "function" | "slope" | "ad" => GlyphKind::Function,
            "ring_mod" | "ringmod" | "ring" => GlyphKind::RingMod,
            _ => GlyphKind::Generic,
        }
    }
}

pub struct GlyphPaint<'a> {
    pub kind: GlyphKind,
    pub intensity: f32,
    pub style: Style,
    /// Live samples for Scope (and optional fallback wave for Generic).
    pub samples: Option<&'a [f32]>,
    /// Quantizer: 12-bit pitch-class mask for scale glyph (None = Major).
    pub scale_mask: Option<u16>,
}

/// Paint audio glyph into the **inner** rect (already inside borders).
pub fn paint_glyph(buf: &mut Buffer, inner: Rect, paint: &GlyphPaint<'_>) {
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    // Floor so kind glyphs always read; callers may already floor higher.
    let i = paint.intensity.clamp(0.18, 1.0);

    // Prefer live samples whenever the engine published a real block.
    if let Some(s) = paint.samples {
        if s.iter().any(|v| v.abs() > 1e-5) {
            match paint.kind {
                // Keep structural glyphs (not live scope traces).
                GlyphKind::Keyboard | GlyphKind::Io | GlyphKind::Quantizer => {}
                GlyphKind::Meter => {
                    meter_bars(buf, inner, i, paint.style);
                    return;
                }
                _ => {
                    // Live block through the module → scope-style trace.
                    scope_trace(buf, inner, s, i, paint.style);
                    return;
                }
            }
        }
    }

    match paint.kind {
        GlyphKind::Sine => wave(buf, inner, i, paint.style, WaveShape::Sine, 2.0),
        GlyphKind::Triangle => wave(buf, inner, i, paint.style, WaveShape::Triangle, 1.2),
        GlyphKind::Saw => wave(buf, inner, i, paint.style, WaveShape::Saw, 2.0),
        GlyphKind::Square => wave(buf, inner, i, paint.style, WaveShape::Square, 2.0),
        GlyphKind::Filter => filter_curve(buf, inner, i, paint.style),
        GlyphKind::Adsr => adsr(buf, inner, i, paint.style),
        GlyphKind::Vca => vca_gate(buf, inner, i, paint.style),
        GlyphKind::Mixer => mixer(buf, inner, i, paint.style),
        GlyphKind::Noise => noise(buf, inner, i, paint.style),
        GlyphKind::SampleHold => sample_hold(buf, inner, i, paint.style),
        GlyphKind::Sequencer => sequencer(buf, inner, i, paint.style),
        GlyphKind::Quantizer => {
            quantizer(buf, inner, i, paint.style, paint.scale_mask.unwrap_or(0x0AB5))
        }
        GlyphKind::Attenuverter => attenuverter(buf, inner, i, paint.style),
        GlyphKind::MidiCv => midi_cv(buf, inner, i, paint.style),
        GlyphKind::Scope => wave(buf, inner, i, paint.style, WaveShape::Sine, 3.0),
        GlyphKind::Meter => meter_bars(buf, inner, i, paint.style),
        GlyphKind::Keyboard => keyboard(buf, inner, i, paint.style),
        GlyphKind::Io => io_jacks(buf, inner, i, paint.style),
        GlyphKind::Slew => slew_glyph(buf, inner, i, paint.style),
        GlyphKind::ClockDiv => clock_div_glyph(buf, inner, i, paint.style),
        GlyphKind::Delay => delay_glyph(buf, inner, i, paint.style),
        GlyphKind::Wavefolder => wavefolder_glyph(buf, inner, i, paint.style),
        GlyphKind::Lpg => lpg_glyph(buf, inner, i, paint.style),
        GlyphKind::Function => function_glyph(buf, inner, i, paint.style),
        GlyphKind::RingMod => ring_mod_glyph(buf, inner, i, paint.style),
        GlyphKind::Generic => wave(buf, inner, i, paint.style, WaveShape::Sine, 1.5),
    }
}

#[derive(Clone, Copy)]
enum WaveShape {
    Sine,
    Triangle,
    Saw,
    Square,
}

fn wave(buf: &mut Buffer, area: Rect, intensity: f32, style: Style, shape: WaveShape, cycles: f32) {
    let w = area.width as f32;
    let h = area.height as f32;
    let mid = (area.height.saturating_sub(1)) as f32 / 2.0;
    let amp = (mid.max(0.5)) * (0.25 + 0.75 * intensity);
    let phase = intensity * std::f32::consts::TAU * 0.15;

    for col in 0..area.width {
        let t = col as f32 / w.max(1.0);
        let x = t * cycles * std::f32::consts::TAU + phase;
        let s = match shape {
            WaveShape::Sine => x.sin(),
            WaveShape::Triangle => {
                let p = (t * cycles + intensity * 0.1).fract();
                if p < 0.5 {
                    p * 4.0 - 1.0
                } else {
                    3.0 - p * 4.0
                }
            }
            WaveShape::Saw => 2.0 * (t * cycles + intensity * 0.05).fract() - 1.0,
            WaveShape::Square => {
                if (t * cycles + intensity * 0.05).fract() < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        };
        let yf = mid - s * amp;
        plot_wave_col(buf, area, col, yf, h, style);
    }
}

fn plot_wave_col(buf: &mut Buffer, area: Rect, col: u16, yf: f32, h: f32, style: Style) {
    let y0 = yf.floor().clamp(0.0, (h - 1.0).max(0.0));
    let frac = yf - y0;
    let row = y0 as u16;
    let ch = if h < 2.0 {
        if frac < 0.33 {
            '▀'
        } else if frac > 0.66 {
            '▄'
        } else {
            '─'
        }
    } else if frac < 0.25 {
        '▀'
    } else if frac > 0.75 {
        '▄'
    } else {
        '█'
    };
    set(buf, area.x + col, area.y + row, ch, style);
    // Light vertical fill toward midline for thicker hot waves.
    if h >= 3.0 {
        let mid = (h / 2.0).round() as u16;
        let r = row.min(area.height.saturating_sub(1));
        let (lo, hi) = if r <= mid { (r, mid) } else { (mid, r) };
        for y in lo..=hi {
            if y == r {
                continue;
            }
            let soft = if (y as i32 - r as i32).unsigned_abs() == 1 {
                '│'
            } else {
                continue;
            };
            set(buf, area.x + col, area.y + y, soft, style);
        }
    }
}

fn scope_trace(buf: &mut Buffer, area: Rect, samples: &[f32], intensity: f32, style: Style) {
    let n = samples.len().max(1);
    let mid = (area.height.saturating_sub(1)) as f32 / 2.0;
    let amp = mid.max(0.5) * (0.35 + 0.65 * intensity);
    // Peak normalize
    let peak = samples
        .iter()
        .map(|s| s.abs())
        .fold(1e-6_f32, f32::max)
        .max(1e-6);

    for col in 0..area.width {
        let idx = (col as usize * n) / area.width.max(1) as usize;
        let s = (samples.get(idx).copied().unwrap_or(0.0) / peak).clamp(-1.0, 1.0);
        let yf = mid - s * amp;
        plot_wave_col(buf, area, col, yf, area.height as f32, style);
    }
}

fn filter_curve(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Resonant lowpass hump: rises then falls; peak position shifts with intensity.
    let w = area.width.max(1) as f32;
    let h = area.height.max(1) as f32;
    let peak_x = 0.25 + 0.45 * intensity;
    for col in 0..area.width {
        let t = col as f32 / w;
        let d = (t - peak_x).abs() * 2.2;
        let env = (-d * d * 3.5).exp();
        let y_norm = 1.0 - env * (0.35 + 0.65 * intensity);
        let row = (y_norm * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + col, area.y + row, '▀', style);
        // Skirt
        if row + 1 < area.height && env > 0.2 {
            set(buf, area.x + col, area.y + row + 1, '·', style);
        }
    }
}

fn adsr(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Attack / decay / sustain / release polyline in unit height (0 bottom, 1 top).
    let w = area.width.max(1) as f32;
    let h = area.height.max(1) as f32;
    let sus = 0.35 + 0.4 * intensity;
    // breakpoints as fraction of width
    let a = 0.18;
    let d = 0.38;
    let s0 = 0.62;
    let r = 1.0;
    for col in 0..area.width {
        let t = col as f32 / w;
        let level = if t < a {
            t / a
        } else if t < d {
            1.0 - (t - a) / (d - a) * (1.0 - sus)
        } else if t < s0 {
            sus
        } else {
            sus * (1.0 - (t - s0) / (r - s0).max(0.01))
        };
        let y_from_top = (1.0 - level * (0.4 + 0.6 * intensity)) * (h - 1.0);
        let row = y_from_top.round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + col, area.y + row, '●', style);
    }
}

fn vca_gate(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Vertical diamond / gate that opens with intensity.
    let cx = area.width / 2;
    let mid = area.height / 2;
    let open = ((area.width as f32 * 0.15) + (area.width as f32 * 0.35 * intensity)).round() as u16;
    for row in 0..area.height {
        let dy = (row as i32 - mid as i32).unsigned_abs() as f32;
        let max_dy = mid.max(1) as f32;
        let half = ((1.0 - dy / max_dy) * open as f32).round() as i32;
        if half <= 0 {
            set(buf, area.x + cx, area.y + row, '│', style);
            continue;
        }
        for dx in -half..=half {
            let x = cx as i32 + dx;
            if x < 0 || x >= area.width as i32 {
                continue;
            }
            let ch = if dx == 0 {
                '█'
            } else if dx.abs() == half {
                '│'
            } else {
                '▓'
            };
            set(buf, area.x + x as u16, area.y + row, ch, style);
        }
    }
}

fn mixer(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Three inputs converge to one output on the right.
    let mid = area.height / 2;
    let lanes = [0u16, mid, area.height.saturating_sub(1)];
    let join_x = area.width.saturating_sub(2).max(1);
    for &ly in &lanes {
        for x in 0..join_x {
            // Ease toward mid
            let t = x as f32 / join_x.max(1) as f32;
            let yy = (ly as f32 * (1.0 - t) + mid as f32 * t).round() as u16;
            set(buf, area.x + x, area.y + yy.min(area.height - 1), '─', style);
        }
    }
    // Sum node + out
    set(buf, area.x + join_x, area.y + mid, '┼', style);
    if join_x + 1 < area.width {
        set(buf, area.x + join_x + 1, area.y + mid, '▶', style);
    }
    // Intensity as little ticks
    if intensity > 0.5 && mid > 0 {
        set(buf, area.x + join_x, area.y + mid.saturating_sub(1), '·', style);
    }
}

fn noise(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    let density = 0.08 + 0.55 * intensity;
    for y in 0..area.height {
        for x in 0..area.width {
            let h = hash_u32((x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ ((intensity * 1000.0) as u32));
            let r = (h % 1000) as f32 / 1000.0;
            if r < density {
                let ch = match h % 5 {
                    0 => '·',
                    1 => ':',
                    2 => '░',
                    3 => '▒',
                    _ => '∙',
                };
                set(buf, area.x + x, area.y + y, ch, style);
            }
        }
    }
}

fn sample_hold(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    let steps = (3 + (intensity * 4.0) as u16).min(area.width.max(1));
    let step_w = (area.width / steps).max(1);
    let h = area.height.max(1) as f32;
    for s in 0..steps {
        let level = hash_u32(s as u32 * 97 + (intensity * 50.0) as u32) % 1000;
        let yn = (level as f32 / 1000.0) * (0.3 + 0.7 * intensity);
        let row = ((1.0 - yn) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        let x0 = s * step_w;
        let x1 = if s + 1 == steps {
            area.width
        } else {
            ((s + 1) * step_w).min(area.width)
        };
        for x in x0..x1 {
            set(buf, area.x + x, area.y + row, '─', style);
        }
        // vertical riser to next
        if s + 1 < steps {
            let next_level = hash_u32((s + 1) as u32 * 97 + (intensity * 50.0) as u32) % 1000;
            let yn2 = (next_level as f32 / 1000.0) * (0.3 + 0.7 * intensity);
            let row2 = ((1.0 - yn2) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
            let (lo, hi) = if row <= row2 { (row, row2) } else { (row2, row) };
            let x = x1.saturating_sub(1);
            for y in lo..=hi {
                set(buf, area.x + x, area.y + y, '│', style);
            }
        }
    }
}

fn sequencer(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Vertical step bars like a classic step-sequencer panel.
    let n = (4 + (intensity * 4.0) as u16).min(8).min(area.width.max(1));
    let gap = 1u16;
    let usable = area.width.saturating_sub(gap.saturating_mul(n.saturating_sub(1)));
    let bar_w = (usable / n).max(1);
    let h = area.height.max(1) as f32;
    let active = ((intensity * n as f32) as u16).min(n.saturating_sub(1));
    for s in 0..n {
        let level = 0.25 + 0.7 * (((s as f32 * 0.37 + intensity).sin() * 0.5) + 0.5);
        let bar_h = ((level * h).round() as u16).clamp(1, area.height);
        let x0 = s * (bar_w + gap);
        if x0 >= area.width {
            break;
        }
        let x1 = (x0 + bar_w).min(area.width);
        let y0 = area.height.saturating_sub(bar_h);
        let ch = if s == active { '█' } else { '▄' };
        for x in x0..x1 {
            for y in y0..area.height {
                set(buf, area.x + x, area.y + y, ch, style);
            }
        }
    }
}

fn quantizer(buf: &mut Buffer, area: Rect, intensity: f32, style: Style, mask: u16) {
    // 12 pitch-class slots: filled = in scale, dim dots = out. Reads like a
    // miniature keyboard / degree strip for the selected quantizer scale.
    let mask = if mask & 0x0FFF == 0 { 0x0FFF } else { mask & 0x0FFF };
    let n = 12u16;
    let slot_w = (area.width / n).max(1);
    let h = area.height.max(1);

    // Baseline.
    let base_y = area.y + h.saturating_sub(1);
    for x in 0..area.width {
        set(buf, area.x + x, base_y, '─', style);
    }

    for pc in 0..12u16 {
        let on = mask & (1 << pc) != 0;
        let x0 = pc * slot_w;
        if x0 >= area.width {
            break;
        }
        let x1 = ((pc + 1) * slot_w).min(area.width);
        // Black-key-ish positions sit slightly higher when out of scale.
        let is_black = matches!(pc, 1 | 3 | 6 | 8 | 10);
        let bar_h = if on {
            if is_black {
                (h * 2 / 3).max(1)
            } else {
                h.saturating_sub(1).max(1)
            }
        } else {
            1
        };
        let y0 = area.y + h.saturating_sub(bar_h);
        let ch = if on {
            if intensity > 0.55 {
                '█'
            } else {
                '▄'
            }
        } else {
            '·'
        };
        for x in x0..x1 {
            for y in y0..area.y + h {
                if y == base_y && !on {
                    continue;
                }
                set(buf, area.x + x, y, ch, style);
            }
        }
    }
}

fn slew_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Smooth ramp from low-left to high-right (portamento).
    let h = area.height.max(1) as f32;
    for x in 0..area.width {
        let t = x as f32 / area.width.max(1) as f32;
        let yn = t * (0.2 + 0.8 * intensity);
        let row = ((1.0 - yn) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + row, '─', style);
    }
}

fn clock_div_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Nested pulse trains (/2 /4).
    let h = area.height.max(1);
    let mid = h / 2;
    for x in 0..area.width {
        let on2 = (x / 2) % 2 == 0;
        let on4 = (x / 4) % 2 == 0;
        if on2 {
            set(buf, area.x + x, area.y + mid.saturating_sub(1), '▀', style);
        }
        if on4 && intensity > 0.3 {
            set(buf, area.x + x, area.y + mid.min(h - 1), '▄', style);
        }
    }
}

fn delay_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Echo dots receding to the right.
    let taps = (2 + (intensity * 3.0) as u16).min(5);
    let h = area.height.max(1) as f32;
    for t in 0..taps {
        let x = ((t as f32 + 0.5) / taps as f32 * area.width as f32) as u16;
        let yn = 0.3 + 0.5 * (1.0 - t as f32 / taps as f32) * intensity;
        let row = ((1.0 - yn) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        let ch = if t == 0 { '●' } else { '·' };
        if x < area.width {
            set(buf, area.x + x, area.y + row, ch, style);
        }
    }
}

fn wavefolder_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Zigzag fold of a rising line.
    let h = area.height.max(1) as f32;
    let folds = 2 + (intensity * 2.0) as i32;
    for x in 0..area.width {
        let t = x as f32 / area.width.max(1) as f32;
        let zig = (t * folds as f32).fract();
        let tri = if zig < 0.5 { zig * 2.0 } else { 2.0 - zig * 2.0 };
        let row = ((1.0 - tri * 0.85) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + row, '╱', style);
    }
}

fn lpg_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Filter curve that opens with intensity (vactrol-ish).
    filter_curve(buf, area, intensity, style);
}

fn function_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // AD spike: rise then fall.
    let h = area.height.max(1) as f32;
    let peak_x = (area.width as f32 * (0.2 + 0.2 * intensity)) as u16;
    for x in 0..area.width {
        let yn = if x <= peak_x {
            x as f32 / peak_x.max(1) as f32
        } else {
            1.0 - (x - peak_x) as f32 / (area.width - peak_x).max(1) as f32
        };
        let yn = yn.clamp(0.0, 1.0) * (0.4 + 0.6 * intensity);
        let row = ((1.0 - yn) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + row, '─', style);
    }
}

fn ring_mod_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Two crossing waves (×).
    let h = area.height.max(1) as f32;
    let w = area.width.max(1) as f32;
    for x in 0..area.width {
        let t = x as f32 / w;
        let a = ((t * 4.0 * std::f32::consts::PI).sin() * 0.5 + 0.5) * intensity;
        let b = ((t * 6.0 * std::f32::consts::PI).sin() * 0.5 + 0.5) * intensity;
        let ra = ((1.0 - a) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        let rb = ((1.0 - b) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + ra, '·', style);
        set(buf, area.x + x, area.y + rb, '·', style);
    }
}

fn attenuverter(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    let mid_y = area.height / 2;
    let mid_x = area.width / 2;
    // Zero line
    for x in 0..area.width {
        set(buf, area.x + x, area.y + mid_y, '─', style);
    }
    // Bipolar knob bar
    let reach = ((area.width as f32 * 0.15) + (area.width as f32 * 0.35 * intensity)).round() as u16;
    let lo = mid_x.saturating_sub(reach);
    let hi = (mid_x + reach).min(area.width.saturating_sub(1));
    for x in lo..=hi {
        set(buf, area.x + x, area.y + mid_y, '═', style);
    }
    set(buf, area.x + mid_x, area.y + mid_y, '●', style);
    if area.height >= 3 {
        set(buf, area.x, area.y, '−', style);
        set(buf, area.x + area.width - 1, area.y, '+', style);
    }
}

fn midi_cv(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Mini keyboard strip + rising CV line
    let key_h = if area.height >= 3 { 1u16 } else { 0 };
    let wave_h = area.height.saturating_sub(key_h);
    if wave_h > 0 {
        let sub = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: wave_h,
        };
        wave(buf, sub, intensity, style, WaveShape::Saw, 1.0);
    }
    if key_h > 0 {
        let y = area.y + area.height - 1;
        for x in 0..area.width {
            let black = matches!(x % 7, 1 | 2 | 4 | 5);
            let ch = if black { '▀' } else { '▄' };
            set(buf, area.x + x, y, ch, style);
        }
    }
}

fn meter_bars(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    let bars = area.width.min(8).max(1);
    let bar_w = (area.width / bars).max(1);
    for b in 0..bars {
        // Falling fake spectrum shaped by intensity
        let t = b as f32 / bars.max(1) as f32;
        let level = intensity * (1.0 - 0.55 * t) * (0.7 + 0.3 * ((b as f32 * 1.7).sin().abs()));
        let filled = ((level * area.height as f32).ceil() as u16).clamp(0, area.height);
        for row in 0..filled {
            let y = area.y + area.height - 1 - row;
            let x0 = b * bar_w;
            for dx in 0..bar_w.saturating_sub(0).min(bar_w) {
                if x0 + dx < area.width {
                    let ch = if row + 1 == filled { '▄' } else { '█' };
                    set(buf, area.x + x0 + dx, y, ch, style);
                }
            }
        }
    }
}

fn keyboard(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // White key bodies with black key overlays
    for x in 0..area.width {
        let is_black = matches!(x % 7, 1 | 2 | 4);
        for y in 0..area.height {
            let ch = if is_black && y < area.height.saturating_sub(1).max(1) {
                '█'
            } else if y == area.height - 1 {
                '▄'
            } else {
                '░'
            };
            // Held-looking keys when hot
            let on = intensity > 0.5 && matches!(x % 7, 0 | 3 | 6);
            let ch = if on && !is_black { '▓' } else { ch };
            set(buf, area.x + x, area.y + y, ch, style);
        }
    }
}

fn io_jacks(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    let mid = area.height / 2;
    // Left in, right out
    let left = "◀●";
    let right = "●▶";
    let y = area.y + mid;
    for (i, ch) in left.chars().enumerate() {
        if (i as u16) < area.width {
            set(buf, area.x + i as u16, y, ch, style);
        }
    }
    let rchars: Vec<char> = right.chars().collect();
    let start = area.width.saturating_sub(rchars.len() as u16);
    for (i, ch) in rchars.into_iter().enumerate() {
        set(buf, area.x + start + i as u16, y, ch, style);
    }
    if intensity > 0.3 && area.width > 4 {
        for x in 2..area.width.saturating_sub(2) {
            set(buf, area.x + x, y, '─', style);
        }
    }
}

fn hash_u32(n: u32) -> u32 {
    let mut x = n.wrapping_mul(0x9e37_79b9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 13;
    x
}

fn set(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch);
        cell.set_style(style);
    }
}
