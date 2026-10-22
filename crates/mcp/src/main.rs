//! MCP server for a running `engine`.
//!
//! Connects to the engine over gRPC (same control plane as `rack`) and exposes
//! management tools over MCP stdio. Logging goes to stderr only — stdout is
//! reserved for the MCP transport.
//!
//! Engine address: CLI arg, else `SYNTH_ENGINE_ADDR`, else
//! `http://127.0.0.1:50051`.
//!
//! Example host config:
//! ```json
//! {
//!   "mcpServers": {
//!     "synth": {
//!       "command": "path/to/mcp",
//!       "args": ["http://127.0.0.1:50051"]
//!     }
//!   }
//! }
//! ```

mod convert;
mod server;

use rmcp::{ServiceExt, transport::stdio};
use synth_proto::synth_client::SynthClient;
use tracing::info;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use server::SynthMcp;

const DEFAULT_ENGINE_ADDR: &str = "http://127.0.0.1:50051";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // JSON structured logging to stderr (stdout reserved for MCP transport)
    let json_layer = fmt::layer()
        .json()
        .with_ansi(false)
        .with_current_span(true)
        .with_span_list(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .with_writer(std::io::stderr);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(json_layer)
        .init();

    let addr = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("SYNTH_ENGINE_ADDR").ok())
        .unwrap_or_else(|| DEFAULT_ENGINE_ADDR.to_string());

    info!(
        version = env!("CARGO_PKG_VERSION"),
        %addr,
        "connecting to engine"
    );
    let client = SynthClient::connect(addr.clone())
        .await
        .map_err(|err| anyhow::anyhow!("failed to connect to engine at {addr}: {err}"))?;
    info!(%addr, "connected to engine");

    let service = SynthMcp::new(client).serve(stdio()).await?;
    info!("MCP server running on stdio");
    service.waiting().await?;
    Ok(())
}
