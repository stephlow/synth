//! Kind-specific audio symbols for module tile interiors.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

/// What to paint inside a module body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphKind {
    Sine,
    Triangle,
    Saw,
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
    Comparator,
    Logic,
    Edge,
    Bernoulli,
    Crossfade,
    Euclidean,
    ClockMult,
    RandomTrig,
    Switch,
    MinMax,
    Abs,
    Clip,
    Saturator,
    Bitcrush,
    EnvFollow,
    TrackHold,
    Link,
    Generic,
}

/// Param-derived paint state so glyphs reflect real module knobs.
#[derive(Clone, Copy, Debug, Default)]
pub enum GlyphDetail {
    #[default]
    None,
    Quantizer {
        mask: u16,
    },
    Comparator {
        thresh: f32,
    },
    Logic {
        mode: u8,
    },
    Crossfade {
        fade: f32,
    },
    Edge {
        width_ms: f32,
    },
    Bernoulli {
        chance: f32,
    },
    Euclidean {
        steps: u8,
        fill: u8,
        rotate: u8,
    },
    RandomTrig {
        rate_hz: f32,
    },
    Switch {
        n_steps: u8,
    },
    Clip {
        level: f32,
    },
    Saturator {
        drive: f32,
        mix: f32,
    },
    Bitcrush {
        bits: f32,
        rate: f32,
    },
    EnvFollow {
        rise_s: f32,
        fall_s: f32,
    },
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
            "comparator" => GlyphKind::Comparator,
            "logic" => GlyphKind::Logic,
            "edge" => GlyphKind::Edge,
            "bernoulli" => GlyphKind::Bernoulli,
            "crossfade" => GlyphKind::Crossfade,
            "euclidean" => GlyphKind::Euclidean,
            "clock_mult" | "clock-mult" => GlyphKind::ClockMult,
            "random_trig" | "random-trig" => GlyphKind::RandomTrig,
            "switch" => GlyphKind::Switch,
            "minmax" | "min_max" => GlyphKind::MinMax,
            "abs" => GlyphKind::Abs,
            "clip" => GlyphKind::Clip,
            "saturator" => GlyphKind::Saturator,
            "bitcrush" => GlyphKind::Bitcrush,
            "env_follow" | "env-follow" => GlyphKind::EnvFollow,
            "track_hold" | "track-hold" => GlyphKind::TrackHold,
            "link" | "ableton_link" | "ableton-link" => GlyphKind::Link,
            _ => GlyphKind::Generic,
        }
    }

    /// Structural glyphs keep identity; do not replace with live scope.
    fn keep_structural(self) -> bool {
        matches!(
            self,
            GlyphKind::Keyboard
                | GlyphKind::Io
                | GlyphKind::Quantizer
                | GlyphKind::Comparator
                | GlyphKind::Logic
                | GlyphKind::Edge
                | GlyphKind::Bernoulli
                | GlyphKind::Euclidean
                | GlyphKind::ClockMult
                | GlyphKind::RandomTrig
                | GlyphKind::Switch
                | GlyphKind::MinMax
                | GlyphKind::Abs
                | GlyphKind::Clip
                | GlyphKind::TrackHold
                | GlyphKind::Link
                | GlyphKind::ClockDiv
                | GlyphKind::Sequencer
                | GlyphKind::Attenuverter
                | GlyphKind::MidiCv
        )
    }
}

pub struct GlyphPaint<'a> {
    pub kind: GlyphKind,
    pub intensity: f32,
    pub style: Style,
    /// Live samples for Scope (and audio-path modules).
    pub samples: Option<&'a [f32]>,
    /// Param-derived state for kind-specific drawing.
    pub detail: GlyphDetail,
}

/// Paint audio glyph into the **inner** rect (already inside borders).
pub fn paint_glyph(buf: &mut Buffer, inner: Rect, paint: &GlyphPaint<'_>) {
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let i = paint.intensity.clamp(0.18, 1.0);

    if let Some(s) = paint.samples {
        if s.iter().any(|v| v.abs() > 1e-5) {
            match paint.kind {
                GlyphKind::Meter => {
                    meter_bars(buf, inner, i, paint.style);
                    return;
                }
                k if k.keep_structural() => {}
                _ => {
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
            let mask = match paint.detail {
                GlyphDetail::Quantizer { mask } => mask,
                _ => 0x0AB5,
            };
            quantizer(buf, inner, i, paint.style, mask)
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
        GlyphKind::Comparator => {
            let thresh = match paint.detail {
                GlyphDetail::Comparator { thresh } => thresh,
                _ => 0.0,
            };
            comparator_glyph(buf, inner, i, paint.style, thresh);
        }
        GlyphKind::Logic => {
            let mode = match paint.detail {
                GlyphDetail::Logic { mode } => mode,
                _ => 0,
            };
            logic_glyph(buf, inner, i, paint.style, mode);
        }
        GlyphKind::Edge => {
            let width_ms = match paint.detail {
                GlyphDetail::Edge { width_ms } => width_ms,
                _ => 5.0,
            };
            edge_glyph(buf, inner, i, paint.style, width_ms);
        }
        GlyphKind::Bernoulli => {
            let chance = match paint.detail {
                GlyphDetail::Bernoulli { chance } => chance,
                _ => 0.5,
            };
            bernoulli_glyph(buf, inner, i, paint.style, chance);
        }
        GlyphKind::Crossfade => {
            let fade = match paint.detail {
                GlyphDetail::Crossfade { fade } => fade,
                _ => 0.5,
            };
            crossfade_glyph(buf, inner, i, paint.style, fade);
        }
        GlyphKind::Euclidean => {
            let (steps, fill, rotate) = match paint.detail {
                GlyphDetail::Euclidean {
                    steps,
                    fill,
                    rotate,
                } => (steps, fill, rotate),
                _ => (16, 4, 0),
            };
            euclidean_glyph(buf, inner, i, paint.style, steps, fill, rotate);
        }
        GlyphKind::ClockMult => clock_mult_glyph(buf, inner, i, paint.style),
        GlyphKind::Link => link_glyph(buf, inner, i, paint.style),
        GlyphKind::RandomTrig => {
            let rate = match paint.detail {
                GlyphDetail::RandomTrig { rate_hz } => rate_hz,
                _ => 4.0,
            };
            random_trig_glyph(buf, inner, i, paint.style, rate);
        }
        GlyphKind::Switch => {
            let n = match paint.detail {
                GlyphDetail::Switch { n_steps } => n_steps,
                _ => 4,
            };
            switch_glyph(buf, inner, i, paint.style, n);
        }
        GlyphKind::MinMax => minmax_glyph(buf, inner, i, paint.style),
        GlyphKind::Abs => abs_glyph(buf, inner, i, paint.style),
        GlyphKind::Clip => {
            let level = match paint.detail {
                GlyphDetail::Clip { level } => level,
                _ => 1.0,
            };
            clip_glyph(buf, inner, i, paint.style, level);
        }
        GlyphKind::Saturator => {
            let (drive, mix) = match paint.detail {
                GlyphDetail::Saturator { drive, mix } => (drive, mix),
                _ => (2.0, 1.0),
            };
            saturator_glyph(buf, inner, i, paint.style, drive, mix);
        }
        GlyphKind::Bitcrush => {
            let (bits, rate) = match paint.detail {
                GlyphDetail::Bitcrush { bits, rate } => (bits, rate),
                _ => (8.0, 0.25),
            };
            bitcrush_glyph(buf, inner, i, paint.style, bits, rate);
        }
        GlyphKind::EnvFollow => {
            let (rise, fall) = match paint.detail {
                GlyphDetail::EnvFollow { rise_s, fall_s } => (rise_s, fall_s),
                _ => (0.01, 0.1),
            };
            env_follow_glyph(buf, inner, i, paint.style, rise, fall);
        }
        GlyphKind::TrackHold => track_hold_glyph(buf, inner, i, paint.style),
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
    // Floor peak at 1.0 so Y Scale / true amplitude are visible; only
    // compress when the signal (or scaled display) exceeds ±1.
    let peak = samples
        .iter()
        .map(|s| s.abs())
        .fold(1.0_f32, f32::max);

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

fn comparator_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style, thresh: f32) {
    let h = area.height.max(1) as f32;
    // thresh -1..1 → row from bottom (low) to top (high) inverted for screen y
    let t = thresh.clamp(-1.0, 1.0);
    let yn = (t + 1.0) * 0.5; // 0 bottom-ish as high voltage top
    let row = ((1.0 - yn) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
    // Input-ish rising saw crossing threshold
    for x in 0..area.width {
        let ramp = x as f32 / area.width.max(1) as f32 * 2.0 - 1.0;
        let ry = ((1.0 - (ramp + 1.0) * 0.5) * (h - 1.0))
            .round()
            .clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + ry, '·', style);
        if ramp > t {
            // fill above threshold (gate region)
            let (lo, hi) = if ry <= row { (ry, row) } else { (row, ry) };
            for y in lo..=hi {
                if y == row {
                    continue;
                }
                set(buf, area.x + x, area.y + y, '░', style);
            }
        }
    }
    for x in 0..area.width {
        set(buf, area.x + x, area.y + row, '═', style);
    }
    if intensity > 0.5 && area.width > 2 {
        set(buf, area.x + area.width - 1, area.y + row, '▶', style);
    }
}

fn logic_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style, mode: u8) {
    let mid = area.height / 2;
    let sym = match mode {
        1 => "OR",
        2 => "XOR",
        3 => "NOT",
        _ => "AND",
    };
    // Input stubs
    if area.width >= 3 {
        set(buf, area.x, area.y + mid.saturating_sub(1).min(area.height - 1), '─', style);
        if mode != 3 && area.height > 1 {
            set(buf, area.x, area.y + (mid + 1).min(area.height - 1), '─', style);
        }
    }
    // Center label
    let start = area.width.saturating_sub(sym.len() as u16) / 2;
    for (i, ch) in sym.chars().enumerate() {
        let x = start + i as u16;
        if x < area.width {
            set(buf, area.x + x, area.y + mid, ch, style);
        }
    }
    // Out
    if area.width >= 2 {
        set(
            buf,
            area.x + area.width - 1,
            area.y + mid,
            if intensity > 0.45 { '▶' } else { '─' },
            style,
        );
    }
}

fn edge_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style, width_ms: f32) {
    let h = area.height.max(1) as f32;
    let mid = (h / 2.0) as u16;
    // Gate step then pulse; pulse width ∝ width_ms (0.5..50)
    let frac = ((width_ms - 0.5) / 49.5).clamp(0.0, 1.0);
    let pulse_w = (1.0 + frac * (area.width as f32 * 0.45)).round().max(1.0) as u16;
    let step_x = (area.width as f32 * 0.35) as u16;
    for x in 0..area.width {
        if x < step_x {
            // low then high step
            let row = if x < step_x / 2 {
                (h * 0.7) as u16
            } else {
                (h * 0.25) as u16
            };
            set(
                buf,
                area.x + x,
                area.y + row.min(area.height - 1),
                '─',
                style,
            );
        } else if x < step_x.saturating_add(pulse_w) {
            set(buf, area.x + x, area.y + mid.saturating_sub(1).min(area.height - 1), '▀', style);
            if intensity > 0.4 {
                set(buf, area.x + x, area.y + mid.min(area.height - 1), '█', style);
            }
        } else {
            set(buf, area.x + x, area.y + (h * 0.7) as u16, '─', style);
        }
    }
}

fn bernoulli_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style, chance: f32) {
    let mid = area.height / 2;
    let c = chance.clamp(0.0, 1.0);
    let split = area.width / 2;
    // In from left
    for x in 0..split.min(area.width) {
        set(buf, area.x + x, area.y + mid, '─', style);
    }
    if split < area.width {
        set(buf, area.x + split, area.y + mid, '┤', style);
    }
    // A up-right, B down-right; thickness by chance
    let a_w = ((area.width.saturating_sub(split) as f32) * c).round() as u16;
    let b_w = ((area.width.saturating_sub(split) as f32) * (1.0 - c)).round() as u16;
    let y_a = mid.saturating_sub(1);
    let y_b = (mid + 1).min(area.height - 1);
    for x in 0..a_w {
        let px = split + 1 + x;
        if px < area.width {
            set(buf, area.x + px, area.y + y_a, if intensity > 0.5 { '═' } else { '─' }, style);
        }
    }
    for x in 0..b_w {
        let px = split + 1 + x;
        if px < area.width {
            set(buf, area.x + px, area.y + y_b, '─', style);
        }
    }
    if split + 1 < area.width {
        set(buf, area.x + split + 1, area.y + y_a, 'A', style);
        set(buf, area.x + split + 1, area.y + y_b, 'B', style);
    }
}

fn crossfade_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style, fade: f32) {
    let f = fade.clamp(0.0, 1.0);
    let h = area.height.max(1) as f32;
    let mid = area.height / 2;
    // A and B levels as short bars left
    let a_row = ((1.0 - (1.0 - f) * 0.7) * (h - 1.0)).round() as u16;
    let b_row = ((1.0 - f * 0.7) * (h - 1.0)).round() as u16;
    set(buf, area.x, area.y + a_row.min(area.height - 1), 'A', style);
    if area.height > 1 {
        set(buf, area.x, area.y + b_row.min(area.height - 1), 'B', style);
    }
    // Fade marker
    let mx = 1 + ((area.width.saturating_sub(3) as f32) * f).round() as u16;
    for x in 1..area.width.saturating_sub(1) {
        let t = (x - 1) as f32 / area.width.saturating_sub(2).max(1) as f32;
        let y = (a_row as f32 * (1.0 - t) + b_row as f32 * t).round() as u16;
        set(buf, area.x + x, area.y + y.min(area.height - 1), '─', style);
    }
    set(buf, area.x + mx.min(area.width - 1), area.y + mid, '●', style);
    if area.width > 1 {
        set(
            buf,
            area.x + area.width - 1,
            area.y + mid,
            if intensity > 0.4 { '▶' } else { '─' },
            style,
        );
    }
}

fn euclid_hit(step: u32, steps: u32, fill: u32, rotate: u32) -> bool {
    if steps == 0 || fill == 0 {
        return false;
    }
    let fill = fill.min(steps);
    let s = (step + rotate) % steps;
    (s * fill) % steps < fill
}

fn euclidean_glyph(
    buf: &mut Buffer,
    area: Rect,
    intensity: f32,
    style: Style,
    steps: u8,
    fill: u8,
    rotate: u8,
) {
    let steps = steps.max(1) as u32;
    let fill = fill as u32;
    let rotate = rotate as u32;
    let n = steps.min(area.width as u32).max(1);
    let slot = (area.width as u32 / n).max(1) as u16;
    let mid = area.height / 2;
    for s in 0..n {
        let hit = euclid_hit(s, steps, fill, rotate);
        let x = (s as u16 * slot).min(area.width.saturating_sub(1));
        let ch = if hit {
            if intensity > 0.5 {
                '●'
            } else {
                '○'
            }
        } else {
            '·'
        };
        set(buf, area.x + x, area.y + mid, ch, style);
        // optional second row tick for hits
        if hit && area.height > 2 {
            set(buf, area.x + x, area.y + mid.saturating_sub(1), '│', style);
        }
    }
}

fn clock_mult_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    // Inverse of clock_div: denser trains on lower rows.
    let h = area.height.max(1);
    let rows = [0u16, h / 2, h.saturating_sub(1)];
    let periods = [8u16, 4, 2];
    for (ri, &row) in rows.iter().enumerate() {
        if row >= h {
            continue;
        }
        let p = periods[ri.min(2)];
        for x in 0..area.width {
            if (x / p) % 2 == 0 {
                let ch = if ri == 2 && intensity > 0.4 { '▀' } else { '▄' };
                set(buf, area.x + x, area.y + row, ch, style);
            }
        }
    }
}

/// Two interlocked rings (chain-link) — Ableton Link peers sharing a session.
fn link_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    let _ = intensity;
    let mid_y = area.y + area.height / 2;
    let mid_x = area.x + area.width / 2;
    let r = (area.width.min(area.height) / 3).max(1);
    // Left ring.
    let cx0 = mid_x.saturating_sub(r);
    // Right ring (overlaps).
    let cx1 = mid_x.saturating_add(r.saturating_sub(1));
    for &(cx, cy) in &[(cx0, mid_y), (cx1, mid_y)] {
        // Diamond-ish circle outline.
        for dy in -(r as i16)..=(r as i16) {
            for dx in -(r as i16)..=(r as i16) {
                let d = (dx * dx + dy * dy) as f32;
                let r2 = (r * r) as f32;
                let inner = ((r.saturating_sub(1)) * (r.saturating_sub(1))) as f32;
                if d <= r2 && d >= inner * 0.55 {
                    let x = cx as i16 + dx;
                    let y = cy as i16 + dy;
                    if x >= area.x as i16
                        && y >= area.y as i16
                        && x < (area.x + area.width) as i16
                        && y < (area.y + area.height) as i16
                    {
                        set(buf, x as u16, y as u16, '○', style);
                    }
                }
            }
        }
    }
    // Center bridge.
    if area.width > 2 {
        set(buf, mid_x, mid_y, '═', style);
    }
}

fn random_trig_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style, rate_hz: f32) {
    let density = (rate_hz / 40.0).clamp(0.05, 0.9) * (0.5 + 0.5 * intensity);
    let mid = area.height / 2;
    for x in 0..area.width {
        let h = hash_u32(x as u32 * 131 + (rate_hz * 10.0) as u32);
        let r = (h % 1000) as f32 / 1000.0;
        if r < density {
            set(buf, area.x + x, area.y + mid, '│', style);
            if area.height > 2 && r < density * 0.5 {
                set(buf, area.x + x, area.y + mid.saturating_sub(1), '▀', style);
            }
        } else {
            set(buf, area.x + x, area.y + mid, '·', style);
        }
    }
}

fn switch_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style, n_steps: u8) {
    let n = n_steps.clamp(2, 4) as u16;
    let mid = area.height / 2;
    let lane_h = if area.height >= n {
        1u16
    } else {
        0
    };
    for s in 0..n {
        let y = if lane_h > 0 {
            (s * (area.height - 1) / (n - 1).max(1)).min(area.height - 1)
        } else {
            mid
        };
        set(buf, area.x, area.y + y, '>', style);
        for x in 1..area.width.saturating_sub(1) {
            set(buf, area.x + x, area.y + y, '─', style);
        }
    }
    // Common out
    if area.width > 1 {
        set(
            buf,
            area.x + area.width - 1,
            area.y + mid,
            if intensity > 0.4 { '▶' } else { '┤' },
            style,
        );
    }
}

fn minmax_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    let h = area.height.max(1) as f32;
    for x in 0..area.width {
        let t = x as f32 / area.width.max(1) as f32;
        let a = (t * 3.0 * std::f32::consts::PI).sin() * 0.4;
        let b = (t * 5.0 * std::f32::consts::PI + 1.0).sin() * 0.4;
        let mx = a.max(b) * intensity.max(0.4);
        let mn = a.min(b) * intensity.max(0.4);
        let ry_max = ((0.5 - mx * 0.5) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        let ry_min = ((0.5 - mn * 0.5) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + ry_max, '─', style);
        set(buf, area.x + x, area.y + ry_min, '─', style);
    }
}

fn abs_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    let h = area.height.max(1) as f32;
    for x in 0..area.width {
        let t = x as f32 / area.width.max(1) as f32;
        // Full-wave: |sin|
        let s = ((t * 2.0 * std::f32::consts::PI).sin()).abs() * (0.3 + 0.7 * intensity);
        let row = ((1.0 - s) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + row, '─', style);
    }
    // zero line
    let mid = area.height / 2;
    for x in 0..area.width {
        if hash_u32(x as u32) % 3 == 0 {
            set(buf, area.x + x, area.y + mid, '·', style);
        }
    }
}

fn clip_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style, level: f32) {
    let h = area.height.max(1) as f32;
    let lim = level.clamp(0.05, 2.0).min(1.0);
    for x in 0..area.width {
        let t = x as f32 / area.width.max(1) as f32;
        let s = ((t * 2.0 * std::f32::consts::PI).sin() * (0.5 + 0.5 * intensity)).clamp(-lim, lim);
        let row = ((0.5 - s * 0.5) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + row, '─', style);
    }
    // clip rails
    let top = ((0.5 - lim * 0.5) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
    let bot = ((0.5 + lim * 0.5) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
    for x in 0..area.width {
        if x % 2 == 0 {
            set(buf, area.x + x, area.y + top, '·', style);
            set(buf, area.x + x, area.y + bot, '·', style);
        }
    }
}

fn saturator_glyph(
    buf: &mut Buffer,
    area: Rect,
    intensity: f32,
    style: Style,
    drive: f32,
    mix: f32,
) {
    // Transfer curve: y = tanh(drive * x) mixed with x
    let h = area.height.max(1) as f32;
    let d = drive.clamp(0.1, 20.0);
    let m = mix.clamp(0.0, 1.0);
    for x in 0..area.width {
        let xin = x as f32 / area.width.max(1) as f32 * 2.0 - 1.0;
        let wet = (xin * d * 0.5).tanh();
        let y = xin * (1.0 - m) + wet * m;
        let y = y * (0.4 + 0.6 * intensity);
        let row = ((0.5 - y * 0.5) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + row, '─', style);
    }
}

fn bitcrush_glyph(
    buf: &mut Buffer,
    area: Rect,
    intensity: f32,
    style: Style,
    bits: f32,
    rate: f32,
) {
    let h = area.height.max(1) as f32;
    let levels = (2f32.powf(bits.clamp(1.0, 16.0)) - 1.0).max(1.0);
    let hold = (1.0 / rate.clamp(0.01, 1.0)).round().max(1.0) as u16;
    let mut held = 0.0f32;
    for x in 0..area.width {
        if x % hold == 0 {
            let t = x as f32 / area.width.max(1) as f32;
            let s = (t * 2.0 * std::f32::consts::PI).sin() * (0.4 + 0.6 * intensity);
            held = (s * levels).round() / levels;
        }
        let row = ((0.5 - held * 0.5) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + row, '─', style);
    }
}

fn env_follow_glyph(
    buf: &mut Buffer,
    area: Rect,
    intensity: f32,
    style: Style,
    rise_s: f32,
    fall_s: f32,
) {
    // Asymmetric envelope: rise fraction of width from rise/(rise+fall)
    let h = area.height.max(1) as f32;
    let r = rise_s.max(1e-4);
    let f = fall_s.max(1e-4);
    let peak_t = (r / (r + f)).clamp(0.1, 0.85);
    let peak_x = (area.width as f32 * peak_t) as u16;
    for x in 0..area.width {
        let yn = if x <= peak_x {
            x as f32 / peak_x.max(1) as f32
        } else {
            1.0 - (x - peak_x) as f32 / (area.width - peak_x).max(1) as f32
        };
        let yn = yn.clamp(0.0, 1.0) * (0.35 + 0.65 * intensity);
        let row = ((1.0 - yn) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + row, '─', style);
    }
}

fn track_hold_glyph(buf: &mut Buffer, area: Rect, intensity: f32, style: Style) {
    let h = area.height.max(1) as f32;
    let split = area.width * 2 / 3;
    for x in 0..area.width {
        let yn = if x < split {
            // tracking ramp
            let t = x as f32 / split.max(1) as f32;
            0.2 + 0.6 * t * intensity.max(0.4)
        } else {
            // hold flat
            0.2 + 0.6 * intensity.max(0.4)
        };
        let row = ((1.0 - yn) * (h - 1.0)).round().clamp(0.0, h - 1.0) as u16;
        set(buf, area.x + x, area.y + row, if x < split { '─' } else { '═' }, style);
    }
    if split < area.width {
        set(buf, area.x + split, area.y + area.height / 2, '┤', style);
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
