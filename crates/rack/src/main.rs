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
use futures::{FutureExt, StreamExt};
use ratatui::Terminal;
use synth_proto::synth_client::SynthClient;
use synth_proto::Empty;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use app::App;
use draw::draw;
use keys::handle_key;
use model::{DEFAULT_ENGINE_ADDR, DEFAULT_LOG_FILE};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log_path = std::env::var("RACK_LOG").unwrap_or_else(|_| DEFAULT_LOG_FILE.to_string());
    let log_dir = std::path::Path::new(&log_path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let log_name = std::path::Path::new(&log_path)
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new(DEFAULT_LOG_FILE));
    let file_appender = tracing_appender::rolling::never(log_dir, log_name);
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!(version = env!("CARGO_PKG_VERSION"), log_path = %log_path, "starting rack");

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
