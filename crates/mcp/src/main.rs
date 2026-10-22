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
    // ──────────────────────────────────────────────────────────────────────
    // MCP LOGGING ARCHITECTURE: stderr only (stdout reserved for JSON-RPC)
    // ──────────────────────────────────────────────────────────────────────
    // The MCP protocol uses stdio for JSON-RPC communication:
    //   - stdout: JSON-RPC requests/responses (MUST be valid JSON, no extra output)
    //   - stderr: Free for logging, diagnostics, progress
    //
    // This is a hard requirement — any output on stdout breaks the MCP client.
    // We use a direct stderr writer (NOT non-blocking) because:
    //   ✅ MCP server is not real-time; no audio callback to protect
    //   ✅ Lower latency for stderr (no extra thread hop)
    //   ✅ Simpler shutdown (no guard needed)
    //   ✅ stderr is typically line-buffered or unbuffered to terminal
    //
    // If this were a high-throughput service, we'd use non_blocking here too.
    //
    // JSON STRUCTURED LOGGING (same as rack/engine):
    //   - timestamp, level, fields, target, file, line, threadId, threadName, spans
    //   - Machine-parseable for log aggregators
    //   - RUST_LOG env var controls level at runtime
    let json_layer = fmt::layer()
        .json()                          // JSON output
        .with_ansi(false)                // No ANSI color codes
        .with_current_span(true)         // Include current span fields
        .with_span_list(true)            // Include parent span fields
        .with_thread_ids(true)           // Thread ID
        .with_thread_names(true)         // Thread name
        .with_file(true)                 // Source file
        .with_line_number(true)          // Source line
        .with_writer(std::io::stderr);   // Direct stderr (no non-blocking wrapper)

    // ──────────────────────────────────────────────────────────────────────
    // LEVEL FILTERING: Runtime control via RUST_LOG env var
    // ──────────────────────────────────────────────────────────────────────
    // Examples:
    //   RUST_LOG=info                           # Default: INFO and above
    //   RUST_LOG=trace                          # Everything (verbose!)
    //   RUST_LOG=mcp=trace,engine=debug         # Per-module control
    //
    // PERFORMANCE NOTE:
    //   - Disabled macros (trace! when level=info) compile to near-no-op
    //   - Filtered macros check level at runtime
    //   - Actually logging serializes JSON + writes to stderr
    //   - MCP server is not latency-critical, but avoid trace in hot loops
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // Registry combines filter + layer.
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
