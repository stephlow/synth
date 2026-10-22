//! Format and step module parameters for the TUI (discrete choices, etc.).

use synth_proto::{ControlDescriptor, ModuleType};

/// Look up discrete choice labels for a control via its module kind + param index.
pub fn choices_for<'a>(
    control: &ControlDescriptor,
    module_types: &'a [ModuleType],
) -> &'a [String] {
    module_types
        .iter()
        .find(|t| t.kind == control.module_kind)
        .and_then(|t| t.params.iter().find(|p| p.index == control.param))
        .map(|p| p.choices.as_slice())
        .unwrap_or(&[])
}

/// Human-readable value for menus / param edit / status.
pub fn format_value(
    control: &ControlDescriptor,
    value: f32,
    module_types: &[ModuleType],
) -> String {
    let choices = choices_for(control, module_types);
    if !choices.is_empty() {
        let idx = value.round().clamp(0.0, (choices.len() - 1) as f32) as usize;
        return choices[idx].clone();
    }
    if is_unit_step_param(control) {
        return format!("{}", value.round() as i32);
    }
    if is_log_time_param(control) {
        return format_time_ms(value);
    }
    format!("{value:.3}")
}

/// Scope / delay-style time: show µs, ms, or s depending on magnitude.
fn format_time_ms(ms: f32) -> String {
    let ms = ms.max(0.0);
    if ms < 1.0 {
        let us = ms * 1000.0;
        if us < 10.0 {
            format!("{us:.1} µs")
        } else {
            format!("{us:.0} µs")
        }
    } else if ms < 1000.0 {
        if ms < 10.0 {
            format!("{ms:.2} ms")
        } else if ms < 100.0 {
            format!("{ms:.1} ms")
        } else {
            format!("{ms:.0} ms")
        }
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}

/// Whether this control should step by whole units.
pub fn is_discrete(control: &ControlDescriptor, module_types: &[ModuleType]) -> bool {
    !choices_for(control, module_types).is_empty() || is_unit_step_param(control)
}

/// Integer unit knobs (not continuous CV): Steps, Root, etc.
fn is_unit_step_param(control: &ControlDescriptor) -> bool {
    matches!(
        control.name.as_str(),
        "Steps" | "Root" | "Scale" | "Fill" | "Rotate" | "Bits" | "Mode"
    )
}

/// Adjust a param: discrete params step by 1; continuous use fractional steps.
pub fn step_value(
    control: &ControlDescriptor,
    value: f32,
    direction: f32,
    fine: bool,
    module_types: &[ModuleType],
) -> f32 {
    let choices = choices_for(control, module_types);
    if !choices.is_empty() {
        let max_i = (choices.len() - 1) as f32;
        let cur = value.round().clamp(0.0, max_i);
        return (cur + direction.signum()).clamp(0.0, max_i);
    }
    if is_unit_step_param(control) {
        return (value.round() + direction.signum()).clamp(control.min, control.max);
    }
    // Wide time ranges (scope 50 µs…10 s) need geometric steps so both
    // audio cycles and LFO windows stay reachable.
    if is_log_time_param(control) {
        return step_log(value, direction, fine, control.min, control.max);
    }
    // Coarse ≈ 20 steps across the full range; fine ≈ 100 steps.
    let step = (control.max - control.min) * if fine { 0.01 } else { 0.05 };
    (value + direction * step).clamp(control.min, control.max)
}

/// True for Scope Time (and any continuous "Time" with span ≫ 100×).
fn is_log_time_param(control: &ControlDescriptor) -> bool {
    if control.name != "Time" {
        return false;
    }
    let lo = control.min.max(1e-9);
    let hi = control.max.max(lo);
    hi / lo >= 100.0
}

/// Multiplicative step: coarse ~×2 / ÷2, fine ~×1.15 / ÷1.15.
/// Aggressive so zooming *down* from long windows reaches audio cycles quickly.
fn step_log(value: f32, direction: f32, fine: bool, min: f32, max: f32) -> f32 {
    let ratio = if fine { 1.15_f32 } else { 2.0_f32 };
    let v = value.clamp(min, max).max(min.max(1e-9));
    let next = if direction >= 0.0 {
        v * ratio
    } else {
        v / ratio
    };
    next.clamp(min, max)
}
