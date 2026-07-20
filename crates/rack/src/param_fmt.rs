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
    format!("{value:.3}")
}

/// Whether this control should step by whole units.
pub fn is_discrete(control: &ControlDescriptor, module_types: &[ModuleType]) -> bool {
    !choices_for(control, module_types).is_empty() || is_unit_step_param(control)
}

/// Integer unit knobs (not continuous CV): Steps, Root, etc.
fn is_unit_step_param(control: &ControlDescriptor) -> bool {
    matches!(
        control.name.as_str(),
        "Steps" | "Root" | "Scale"
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
    let step = (control.max - control.min) * if fine { 0.002 } else { 0.02 };
    (value + direction * step).clamp(control.min, control.max)
}
