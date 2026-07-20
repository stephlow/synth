//! Convert `synth-proto` messages into JSON for MCP tool/resource payloads.

use serde_json::{json, Value};
use synth_proto::{
    AudioDevice, CableDescriptor, ControlDescriptor, EngineState, IoChannelMeter,
    IoEndpointDescriptor, ModuleInstanceDescriptor, ModuleType, PatchRolesDescriptor,
    PatchSnapshot,
};

pub fn json_pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

pub fn control(c: &ControlDescriptor) -> Value {
    json!({
        "name": c.name,
        "module": c.module,
        "param": c.param,
        "min": c.min,
        "max": c.max,
        "default_value": c.default_value,
        "module_name": c.module_name,
        "module_kind": c.module_kind,
    })
}

pub fn controls(list: &[ControlDescriptor]) -> Value {
    Value::Array(list.iter().map(control).collect())
}

pub fn module_type(t: &ModuleType) -> Value {
    json!({
        "kind": t.kind,
        "display_name": t.display_name,
        "inputs": t.inputs.iter().map(|p| json!({
            "name": p.name,
            "index": p.index,
        })).collect::<Vec<_>>(),
        "outputs": t.outputs.iter().map(|p| json!({
            "name": p.name,
            "index": p.index,
        })).collect::<Vec<_>>(),
        "params": t.params.iter().map(|p| json!({
            "name": p.name,
            "index": p.index,
            "min": p.min,
            "max": p.max,
            "default_value": p.default_value,
        })).collect::<Vec<_>>(),
    })
}

pub fn module_types(list: &[ModuleType]) -> Value {
    Value::Array(list.iter().map(module_type).collect())
}

pub fn io_endpoint(e: &IoEndpointDescriptor) -> Value {
    let mut v = json!({
        "id": e.id,
        "name": e.name,
        "direction": e.direction,
        "channels": e.channels,
        "device_name": e.device_name,
    });
    if let Some(c) = e.coordinates.as_ref() {
        v.as_object_mut().unwrap().insert(
            "coordinates".into(),
            json!({ "x": c.x, "y": c.y }),
        );
    }
    v
}

pub fn io_endpoints(list: &[IoEndpointDescriptor]) -> Value {
    Value::Array(list.iter().map(io_endpoint).collect())
}

pub fn audio_device(d: &AudioDevice) -> Value {
    json!({
        "name": d.name,
        "is_input": d.is_input,
        "is_output": d.is_output,
        "max_input_channels": d.max_input_channels,
        "max_output_channels": d.max_output_channels,
    })
}

pub fn audio_devices(list: &[AudioDevice]) -> Value {
    Value::Array(list.iter().map(audio_device).collect())
}

fn module_instance(m: &ModuleInstanceDescriptor) -> Value {
    let mut v = json!({
        "id": m.id,
        "kind": m.kind,
        "name": m.name,
    });
    if let Some(c) = m.coordinates.as_ref() {
        v.as_object_mut().unwrap().insert(
            "coordinates".into(),
            json!({ "x": c.x, "y": c.y }),
        );
    }
    v
}

fn cable(c: &CableDescriptor) -> Value {
    json!({
        "id": c.id,
        "src_module": c.src_module,
        "src_port": c.src_port,
        "dst_module": c.dst_module,
        "dst_port": c.dst_port,
    })
}

fn roles(r: &PatchRolesDescriptor) -> Value {
    json!({
        "note_source": r.note_source,
        "activity_gate": r.activity_gate,
        "external_input": r.external_input,
        "output_tap_module": r.output_tap_module,
        "output_tap_port": r.output_tap_port,
    })
}

pub fn patch_snapshot(s: &PatchSnapshot) -> Value {
    let mut v = json!({
        "controls": controls(&s.controls),
        "cables": s.cables.iter().map(cable).collect::<Vec<_>>(),
        "modules": s.modules.iter().map(module_instance).collect::<Vec<_>>(),
        "roles": s.roles.as_ref().map(roles),
        "io_endpoints": io_endpoints(&s.io_endpoints),
        "document_json": s.document_json,
    });
    if let Some(c) = s.canvas.as_ref() {
        v.as_object_mut().unwrap().insert(
            "canvas".into(),
            json!({ "width": c.width, "height": c.height }),
        );
    }
    v
}

fn io_meter(m: &IoChannelMeter) -> Value {
    json!({
        "endpoint_id": m.endpoint_id,
        "peak": m.peak,
        "device_name": m.device_name,
        "connected": m.connected,
    })
}

/// One-shot engine meters. Waveform is omitted to keep the payload small.
pub fn engine_state(s: &EngineState) -> Value {
    json!({
        "active_voices": s.active_voices,
        "output_peak": s.output_peak,
        "io_meters": s.io_meters.iter().map(io_meter).collect::<Vec<_>>(),
        "patch_generation": s.patch_generation,
    })
}

pub fn ok() -> Value {
    json!({ "ok": true })
}
