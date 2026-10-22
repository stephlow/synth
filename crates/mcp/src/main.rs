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
use tracing_subscriber::EnvFilter;

use server::SynthMcp;

const DEFAULT_ENGINE_ADDR: &str = "http://127.0.0.1:50051";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
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
