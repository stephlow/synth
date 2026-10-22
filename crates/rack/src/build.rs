//! Pure builders: engine snapshot → UI modules and jack helpers.

use synth_proto::{CableDescriptor, ModuleInstanceDescriptor, ModuleType};

use crate::model::{BuildInput, Item, ModuleId, UiModule, MASTER_IO_MODULE};

/// Build lattice tiles **only** from the patch snapshot:
/// voice modules (`patch.modules`) + hardware endpoints (`patch.io_endpoints`).
/// Never invent Scope/Meter/Keys or other furniture.
pub fn build_ui_modules(input: BuildInput<'_>) -> Vec<UiModule> {
    let mut ui = Vec::new();

    for vm in input.voice_modules {
        let type_info = input.module_types.iter().find(|t| t.kind == vm.kind);
        let mut items = Vec::new();

        for (ci, c) in input.controls.iter().enumerate() {
            if c.module == vm.id {
                items.push(Item::Param { control_index: ci });
            }
        }

        if vm.kind == "io" {
            // Ports depend on instance channel layout, not static registry.
            // Send = graph inputs, capture = graph outputs (after send mirrors).
            // Empty default: send=0, capture=2 → ch0/ch1 outs only.
            // We don't have send/capture on ModuleInstanceDescriptor from proto —
            // use type_info outs as fallback (ch0, ch1).
            if let Some(t) = type_info {
                for p in &t.outputs {
                    items.push(Item::Output {
                        port: p.index,
                        name: p.name.clone(),
                    });
                }
            }
        } else if let Some(t) = type_info {
            for p in &t.inputs {
                items.push(Item::Input {
                    port: p.index,
                    name: p.name.clone(),
                });
            }
            for p in &t.outputs {
                items.push(Item::Output {
                    port: p.index,
                    name: p.name.clone(),
                });
            }
        }

        ui.push(UiModule {
            id: ModuleId::Voice(vm.id),
            title: format!("{} #{}", vm.name, vm.id),
            kind: vm.kind.clone(),
            items,
        });
    }

    // Hardware endpoints from the patch document only. Skip an input endpoint
    // when a voice `io` module already represents it (avoids double Audio In).
    for ep in input.io_endpoints {
        if ep.direction == "input" {
            let has_voice = input
                .voice_modules
                .iter()
                .any(|m| m.kind == "io" && m.name == ep.name);
            if has_voice {
                continue;
            }
        }

        let mut items = Vec::new();
        let param_module = if ep.direction == "output" {
            Some(MASTER_IO_MODULE)
        } else {
            input
                .voice_modules
                .iter()
                .find(|m| m.kind == "io" && m.name == ep.name)
                .map(|m| m.id)
                .or_else(|| {
                    input
                        .voice_modules
                        .iter()
                        .find(|m| m.kind == "io")
                        .map(|m| m.id)
                })
        };
        if let Some(mid) = param_module {
            for (ci, c) in input.controls.iter().enumerate() {
                if c.module == mid {
                    items.push(Item::Param { control_index: ci });
                }
            }
        }
        // Master Audio Out: main bus assign (SetOutputTap) — dual mono L/R.
        if ep.direction == "output" {
            items.push(Item::Input {
                port: 0,
                name: "Audio L/R".into(),
            });
        }
        if ep.direction == "input" {
            for ch in 0..ep.channels.max(1) {
                items.push(Item::Output {
                    port: ch,
                    name: format!("ch{ch}"),
                });
            }
        }
        ui.push(UiModule {
            id: ModuleId::Io(ep.id.clone()),
            title: ep.name.clone(),
            kind: "io".into(),
            items,
        });
    }

    ui
}

pub fn port_name(
    voice_modules: &[ModuleInstanceDescriptor],
    module_types: &[ModuleType],
    module_id: u32,
    port: u32,
    is_output: bool,
) -> String {
    let kind = voice_modules
        .iter()
        .find(|m| m.id == module_id)
        .map(|m| m.kind.as_str())
        .unwrap_or("?");
    let t = module_types.iter().find(|t| t.kind == kind);
    let ports = t.map(|t| {
        if is_output {
            t.outputs.as_slice()
        } else {
            t.inputs.as_slice()
        }
    });
    if let Some(ports) = ports {
        if let Some(p) = ports.iter().find(|p| p.index == port) {
            return p.name.clone();
        }
    }
    format!("p{port}")
}

pub fn module_short(voice_modules: &[ModuleInstanceDescriptor], id: u32) -> String {
    voice_modules
        .iter()
        .find(|m| m.id == id)
        .map(|m| format!("{}#{}", m.name, m.id))
        .unwrap_or_else(|| format!("m{id}"))
}

pub fn jack_peer_hint(
    voice_modules: &[ModuleInstanceDescriptor],
    module_types: &[ModuleType],
    cables: &[CableDescriptor],
    module_id: u32,
    port: u32,
    is_output: bool,
) -> String {
    let peers: Vec<String> = cables
        .iter()
        .filter(|c| {
            if is_output {
                c.src_module == module_id && c.src_port == port
            } else {
                c.dst_module == module_id && c.dst_port == port
            }
        })
        .map(|c| {
            if is_output {
                format!(
                    "→{}.{}",
                    module_short(voice_modules, c.dst_module),
                    port_name(voice_modules, module_types, c.dst_module, c.dst_port, false)
                )
            } else {
                format!(
                    "←{}.{}",
                    module_short(voice_modules, c.src_module),
                    port_name(voice_modules, module_types, c.src_module, c.src_port, true)
                )
            }
        })
        .collect();
    if peers.is_empty() {
        String::new()
    } else {
        peers.join(" ")
    }
}


