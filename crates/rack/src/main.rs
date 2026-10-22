//! `rack`: lattice modular TUI for a running `engine`.
//!
//! Interaction model: move a **target** on the lattice (`arrows` / `hjkl`),
//! **Enter** opens a nested command menu for the cell, **Esc** pops the mode
//! stack. Patch cables are drawn in gutters between module tiles.
//!
//! Topology stays in sync with external clients (MCP, other racks): the
//! state stream carries `patch_generation`; when it advances, rack refetches
//! `GetPatch` and rebuilds the lattice.

mod app;
mod build;
mod draw;
mod keys;
mod menu_build;
mod model;
mod param_fmt;
mod place;
mod scale_ui;
mod settings;

use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use directories::ProjectDirs;
use futures::{FutureExt, StreamExt};
use ratatui::Terminal;
use synth_proto::synth_client::SynthClient;
use synth_proto::Empty;
use tracing::{info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use app::App;
use draw::draw;
use keys::handle_key;
use model::DEFAULT_ENGINE_ADDR;

/// Holds the tracing worker guard for the lifetime of the application.
/// When dropped, flushes any buffered log events.
struct LogGuard(#[allow(dead_code)] WorkerGuard);

fn init_tracing() -> LogGuard {
    // Platform-standard data directory: ~/.local/share/synth/logs (Linux),
    // ~/Library/Application Support/synth/logs (macOS), %LOCALAPPDATA%\synth\logs (Windows)
    let proj_dirs = ProjectDirs::from("com", "stephlow", "synth")
        .expect("Failed to get project directories");
    let log_dir = proj_dirs.data_local_dir().join("logs");
    std::fs::create_dir_all(&log_dir).expect("Failed to create log directory");

    // Daily rolling with max 5 files (tracing-appender 0.2 doesn't support size-based rotation;
    // logroller crate would be needed for true size-based rotation).
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("synth")
        .filename_suffix("log")
        .max_log_files(5)
        .build(&log_dir)
        .expect("Failed to create rolling file appender");

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // JSON structured logging for machine parsing
    let json_layer = fmt::layer()
        .json()
        .with_ansi(false)
        .with_current_span(true)
        .with_span_list(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .with_writer(non_blocking);

    // Env filter: RUST_LOG=synth=trace,dsp=debug,engine=info,rack=debug
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(json_layer)
        .init();

    LogGuard(guard)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing FIRST, before any other logging
    let _log_guard = init_tracing();

    info!(version = env!("CARGO_PKG_VERSION"), "starting rack");

    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_ENGINE_ADDR.to_string());
    info!(addr, "connecting to engine");
    let mut client = SynthClient::connect(addr.clone()).await?;
    info!(addr, "connected");

    let module_types = client.list_module_types(Empty {}).await?.into_inner().types;
    let audio_devices = client.list_audio_devices(Empty {}).await?.into_inner().devices;
    let snap = client.get_patch(Empty {}).await?.into_inner();
    let mut app = App::new(module_types, audio_devices, snap);

    let mut state_stream = client.stream_state(Empty {}).await?.into_inner();

    enable_raw_mode()?;
    std::io::stdout().execute(EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut events = EventStream::new();

    let result = async {
        while !app.quit {
            if let Ok(size) = terminal.size() {
                app.set_term_size(size.width, size.height);
            }
            terminal.draw(|frame| draw(frame, &mut app))?;

            tokio::select! {
                maybe_event = events.next().fuse() => {
                    if let Some(Ok(Event::Key(key))) = maybe_event {
                        if key.kind == KeyEventKind::Press {
                            handle_key(&mut app, &mut client, key.code).await;
                        }
                    }
                }
                maybe_state = state_stream.next(), if !app.state_stream_ended => {
                    match maybe_state {
                        Some(Ok(state)) => {
                            app.active_voices = state.active_voices;
                            app.output_peak = state.output_peak;
                            app.scope = state.waveform;
                            app.io_meters = state.io_meters;
                            app.apply_live_signals(state.module_signals, state.cable_signals);

                            // External clients (MCP) can mutate topology while
                            // rack is open — pull a fresh snapshot when the
                            // generation counter advances.
                            if state.patch_generation != app.patch_generation {
                                match client.get_patch(Empty {}).await {
                                    Ok(resp) => {
                                        info!(
                                            from = app.patch_generation,
                                            to = state.patch_generation,
                                            "patch generation changed; applying snapshot"
                                        );
                                        app.apply_snapshot(resp.into_inner());
                                        app.patch_generation = state.patch_generation;
                                        // Leave menus if topology moved under them.
                                        if !app.menu_stack.is_empty() || app.param_edit.is_some() {
                                            app.clear_menus();
                                            app.status = "patch updated (external)".into();
                                        }
                                    }
                                    Err(err) => {
                                        warn!(%err, "get_patch after generation change failed");
                                    }
                                }
                            }
                        }
                        Some(Err(err)) => warn!(%err, "state stream error"),
                        None => {
                            warn!("state stream ended");
                            app.state_stream_ended = true;
                        }
                    }
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    disable_raw_mode()?;
    std::io::stdout().execute(LeaveAlternateScreen)?;
    result
}
