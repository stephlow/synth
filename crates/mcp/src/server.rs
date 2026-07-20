//! MCP tool + resource surface over a gRPC `SynthClient`.

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult,
        Resource, ResourceContents, ServerCapabilities, ServerInfo,
    },
    schemars, service::RequestContext, tool, tool_handler, tool_router,
};
use synth_proto::synth_client::SynthClient;
use synth_proto::{
    AddModuleRequest, CalibrationRequest, ConnectRequest, DisconnectRequest, Empty,
    LoadPatchRequest, ModuleCoordinatesEntry, NoteOffRequest, NoteOnRequest, PatchCanvas,
    RemoveModuleRequest, SetCoordinatesRequest, SetIoDeviceRequest, SetModuleCoordinatesRequest,
    SetParamRequest,
};
use tokio_stream::StreamExt;
use tonic::transport::Channel;
use tracing::debug;

use crate::convert;

type Client = SynthClient<Channel>;

#[derive(Clone)]
pub struct SynthMcp {
    /// Cloned per RPC; tonic `Channel` is multiplexed.
    client: Client,
    /// Read by `#[tool_handler]` generated code.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl SynthMcp {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            tool_router: Self::tool_router(),
        }
    }

    fn client(&self) -> Client {
        self.client.clone()
    }
}

// --- tool parameter types ---

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SetParamArgs {
    /// Stable patch module id (`u32::MAX` for master Audio Out calibration knobs).
    module: u32,
    param: u32,
    value: f32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct CalibrationArgs {
    channel: u32,
    scale: f32,
    offset: f32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct NoteOnArgs {
    /// MIDI note number 0..127.
    note: u32,
    /// MIDI velocity 1..127.
    velocity: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct NoteOffArgs {
    /// MIDI note number 0..127.
    note: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SetIoDeviceArgs {
    /// Patch endpoint id, e.g. `audio_in` / `audio_out`.
    endpoint_id: String,
    /// cpal device name, or empty / `default` for host default.
    device_name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct AddModuleArgs {
    /// Module kind key from `list_module_types` (e.g. `vco`, `lfo`, `vcf`).
    kind: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct RemoveModuleArgs {
    module_id: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ConnectArgs {
    src_module: u32,
    src_port: u32,
    dst_module: u32,
    dst_port: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct DisconnectArgs {
    /// Prefer this when known.
    #[serde(default)]
    cable_id: Option<u32>,
    #[serde(default)]
    src_module: u32,
    #[serde(default)]
    src_port: u32,
    #[serde(default)]
    dst_module: u32,
    #[serde(default)]
    dst_port: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct LoadPatchArgs {
    /// Full patch document JSON (same format as `get_patch` / `save_patch`).
    document_json: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SetModuleCoordinatesArgs {
    /// Stable patch module id.
    module_id: u32,
    /// Lattice x (column).
    x: u32,
    /// Lattice y (row).
    y: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ModuleCoordinateArg {
    module_id: u32,
    x: u32,
    y: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SetCoordinatesArgs {
    /// Voice module lattice positions.
    #[serde(default)]
    modules: Vec<ModuleCoordinateArg>,
    /// Optional preferred canvas width in cells.
    #[serde(default)]
    canvas_width: Option<u32>,
    /// Optional preferred canvas height in cells.
    #[serde(default)]
    canvas_height: Option<u32>,
}

#[tool_router]
impl SynthMcp {
    #[tool(description = "List module parameter controls exposed by the current patch")]
    async fn list_controls(&self) -> Result<String, String> {
        debug!("list_controls");
        let list = self
            .client()
            .list_controls(Empty {})
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::controls(&list.controls)))
    }

    #[tool(description = "List module kinds available to add (palette)")]
    async fn list_module_types(&self) -> Result<String, String> {
        debug!("list_module_types");
        let list = self
            .client()
            .list_module_types(Empty {})
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::module_types(&list.types)))
    }

    #[tool(description = "Get full patch snapshot (modules, cables, roles, document JSON)")]
    async fn get_patch(&self) -> Result<String, String> {
        debug!("get_patch");
        let snap = self
            .client()
            .get_patch(Empty {})
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::patch_snapshot(&snap)))
    }

    #[tool(description = "Set a single module parameter value")]
    async fn set_param(
        &self,
        Parameters(args): Parameters<SetParamArgs>,
    ) -> Result<String, String> {
        debug!(
            module = args.module,
            param = args.param,
            value = args.value,
            "set_param"
        );
        self.client()
            .set_param(SetParamRequest {
                module: args.module,
                param: args.param,
                value: args.value,
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(convert::json_pretty(&convert::ok()))
    }

    #[tool(description = "Calibrate a hardware output channel (scale/offset)")]
    async fn set_output_calibration(
        &self,
        Parameters(args): Parameters<CalibrationArgs>,
    ) -> Result<String, String> {
        debug!(
            channel = args.channel,
            scale = args.scale,
            offset = args.offset,
            "set_output_calibration"
        );
        self.client()
            .set_output_calibration(CalibrationRequest {
                channel: args.channel,
                scale: args.scale,
                offset: args.offset,
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(convert::json_pretty(&convert::ok()))
    }

    #[tool(description = "Play a note (soft MIDI, no hardware required)")]
    async fn note_on(&self, Parameters(args): Parameters<NoteOnArgs>) -> Result<String, String> {
        debug!(note = args.note, velocity = args.velocity, "note_on");
        self.client()
            .note_on(NoteOnRequest {
                note: args.note,
                velocity: args.velocity,
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(convert::json_pretty(&convert::ok()))
    }

    #[tool(description = "Stop a note")]
    async fn note_off(&self, Parameters(args): Parameters<NoteOffArgs>) -> Result<String, String> {
        debug!(note = args.note, "note_off");
        self.client()
            .note_off(NoteOffRequest { note: args.note })
            .await
            .map_err(|e| e.to_string())?;
        Ok(convert::json_pretty(&convert::ok()))
    }

    #[tool(description = "List patch-owned audio I/O endpoints and device bindings")]
    async fn list_io_endpoints(&self) -> Result<String, String> {
        debug!("list_io_endpoints");
        let list = self
            .client()
            .list_io_endpoints(Empty {})
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::io_endpoints(
            &list.endpoints,
        )))
    }

    #[tool(description = "Enumerate host audio devices for I/O pickers")]
    async fn list_audio_devices(&self) -> Result<String, String> {
        debug!("list_audio_devices");
        let list = self
            .client()
            .list_audio_devices(Empty {})
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::audio_devices(&list.devices)))
    }

    #[tool(description = "Bind a patch I/O endpoint to a host audio device")]
    async fn set_io_device(
        &self,
        Parameters(args): Parameters<SetIoDeviceArgs>,
    ) -> Result<String, String> {
        debug!(
            endpoint_id = %args.endpoint_id,
            device_name = %args.device_name,
            "set_io_device"
        );
        self.client()
            .set_io_device(SetIoDeviceRequest {
                endpoint_id: args.endpoint_id,
                device_name: args.device_name,
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(convert::json_pretty(&convert::ok()))
    }

    #[tool(description = "Add a module to the patch by kind; returns updated patch snapshot")]
    async fn add_module(
        &self,
        Parameters(args): Parameters<AddModuleArgs>,
    ) -> Result<String, String> {
        debug!(kind = %args.kind, "add_module");
        let snap = self
            .client()
            .add_module(AddModuleRequest { kind: args.kind })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::patch_snapshot(&snap)))
    }

    #[tool(description = "Remove a module by id; returns updated patch snapshot")]
    async fn remove_module(
        &self,
        Parameters(args): Parameters<RemoveModuleArgs>,
    ) -> Result<String, String> {
        debug!(module_id = args.module_id, "remove_module");
        let snap = self
            .client()
            .remove_module(RemoveModuleRequest {
                module_id: args.module_id,
            })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::patch_snapshot(&snap)))
    }

    #[tool(description = "Connect two module ports with a cable; returns updated patch snapshot")]
    async fn patch_connect(
        &self,
        Parameters(args): Parameters<ConnectArgs>,
    ) -> Result<String, String> {
        debug!(
            src = args.src_module,
            dst = args.dst_module,
            "patch_connect"
        );
        let snap = self
            .client()
            .patch_connect(ConnectRequest {
                src_module: args.src_module,
                src_port: args.src_port,
                dst_module: args.dst_module,
                dst_port: args.dst_port,
            })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::patch_snapshot(&snap)))
    }

    #[tool(
        description = "Disconnect a cable by cable_id or by endpoints; returns updated patch snapshot"
    )]
    async fn patch_disconnect(
        &self,
        Parameters(args): Parameters<DisconnectArgs>,
    ) -> Result<String, String> {
        let use_cable_id = args.cable_id.is_some();
        debug!(use_cable_id, "patch_disconnect");
        let snap = self
            .client()
            .patch_disconnect(DisconnectRequest {
                cable_id: args.cable_id.unwrap_or(0),
                src_module: args.src_module,
                src_port: args.src_port,
                dst_module: args.dst_module,
                dst_port: args.dst_port,
                use_cable_id,
            })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::patch_snapshot(&snap)))
    }

    #[tool(description = "Load a full patch document JSON into the engine; returns snapshot")]
    async fn load_patch(
        &self,
        Parameters(args): Parameters<LoadPatchArgs>,
    ) -> Result<String, String> {
        debug!(bytes = args.document_json.len(), "load_patch");
        let snap = self
            .client()
            .load_patch(LoadPatchRequest {
                document_json: args.document_json,
            })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::patch_snapshot(&snap)))
    }

    #[tool(description = "Export the current patch document as JSON")]
    async fn save_patch(&self) -> Result<String, String> {
        debug!("save_patch");
        let resp = self
            .client()
            .save_patch(Empty {})
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(resp.document_json)
    }

    #[tool(
        description = "Set lattice coordinates {x,y} for one module (control surface only; ignored by DSP)"
    )]
    async fn set_module_coordinates(
        &self,
        Parameters(args): Parameters<SetModuleCoordinatesArgs>,
    ) -> Result<String, String> {
        debug!(
            module_id = args.module_id,
            x = args.x,
            y = args.y,
            "set_module_coordinates"
        );
        let snap = self
            .client()
            .set_module_coordinates(SetModuleCoordinatesRequest {
                module_id: args.module_id,
                x: args.x,
                y: args.y,
            })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::patch_snapshot(&snap)))
    }

    #[tool(
        description = "Batch-set module lattice coordinates and optional canvas size (control surface only)"
    )]
    async fn set_coordinates(
        &self,
        Parameters(args): Parameters<SetCoordinatesArgs>,
    ) -> Result<String, String> {
        debug!(n = args.modules.len(), "set_coordinates");
        let modules = args
            .modules
            .into_iter()
            .map(|m| ModuleCoordinatesEntry {
                module_id: m.module_id,
                x: m.x,
                y: m.y,
            })
            .collect();
        let canvas = match (args.canvas_width, args.canvas_height) {
            (Some(width), Some(height)) => Some(PatchCanvas { width, height }),
            (Some(width), None) => Some(PatchCanvas {
                width,
                height: 20,
            }),
            (None, Some(height)) => Some(PatchCanvas {
                width: 32,
                height,
            }),
            (None, None) => None,
        };
        let snap = self
            .client()
            .set_coordinates(SetCoordinatesRequest { modules, canvas })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        Ok(convert::json_pretty(&convert::patch_snapshot(&snap)))
    }

    #[tool(
        description = "One-shot live engine meters (voices, peak, I/O). Not a continuous stream."
    )]
    async fn get_state(&self) -> Result<String, String> {
        debug!("get_state");
        let mut stream = self
            .client()
            .stream_state(Empty {})
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        let state = stream
            .next()
            .await
            .ok_or_else(|| "state stream ended without data".to_string())?
            .map_err(|e| e.to_string())?;
        Ok(convert::json_pretty(&convert::engine_state(&state)))
    }
}

#[tool_handler]
impl ServerHandler for SynthMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(rmcp::model::Implementation::new(
            "mcp",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "Manage a running modular synth engine over gRPC. \
             Use list_module_types / get_patch / list_controls to inspect, \
             then set_param, add_module, patch_connect, note_on, \
             set_coordinates, etc. to edit. \
             Module layout uses coordinates {x,y} and optional canvas; ignored by DSP. \
             Engine must already be running.",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![
                Resource::new("synth://patch", "patch")
                    .with_description("Current patch document JSON")
                    .with_mime_type("application/json"),
                Resource::new("synth://module-types", "module-types")
                    .with_description("Module kinds available to add")
                    .with_mime_type("application/json"),
                Resource::new("synth://controls", "controls")
                    .with_description("Current control descriptor table")
                    .with_mime_type("application/json"),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let text = match request.uri.as_str() {
            "synth://patch" => {
                let snap = self
                    .client()
                    .get_patch(Empty {})
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                snap.into_inner().document_json
            }
            "synth://module-types" => {
                let list = self
                    .client()
                    .list_module_types(Empty {})
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                convert::json_pretty(&convert::module_types(&list.into_inner().types))
            }
            "synth://controls" => {
                let list = self
                    .client()
                    .list_controls(Empty {})
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                convert::json_pretty(&convert::controls(&list.into_inner().controls))
            }
            _ => {
                return Err(McpError::resource_not_found(
                    "resource_not_found",
                    Some(serde_json::json!({ "uri": request.uri })),
                ));
            }
        };

        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            text, &request.uri,
        )
        .with_mime_type("application/json")]))
    }
}
