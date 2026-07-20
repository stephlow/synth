//! `rack`: a ratatui TUI client for a running `engine` process. Connects
//! over gRPC (see `synth-proto`), renders the engine's controls as a rack of
//! bordered module panels (one panel per module, grouped by the
//! `ListControls` response's `module`/`module_name`), each drawn in a style
//! that fits its `module_kind` — knobs for a filter/VCA, a vertical fader for
//! the LFO, horizontal bars for everything else — plus patch-owned IO
//! modules (meters + device picker), a level-meter panel, and a toggle-style
//! on-screen-keyboard panel.

use std::collections::HashSet;

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use futures::{FutureExt, StreamExt};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::{Frame, Terminal};
use synth_proto::synth_client::SynthClient;
use synth_proto::{
    AudioDevice, ControlDescriptor, Empty, IoChannelMeter, IoEndpointDescriptor, NoteOffRequest,
    NoteOnRequest, SetIoDeviceRequest, SetParamRequest,
};
use tracing::{debug, info, trace, warn};
use tracing_subscriber::EnvFilter;

/// On-screen "keyboard": each key toggles the given MIDI note on/off (no
/// reliable key-release event without the terminal's kitty protocol, so a
/// toggle is the simplest thing that always works).
const NOTE_KEYS: &[(char, u8)] = &[
    ('z', 60), // C4
    ('x', 62), // D4
    ('c', 64), // E4
    ('v', 65), // F4
    ('b', 67), // G4
    ('n', 69), // A4
    ('m', 71), // B4
];

const DEFAULT_ENGINE_ADDR: &str = "http://127.0.0.1:50051";
/// Default log file name (in the current directory); override with
/// `RACK_LOG=/path/to/file`. Stdout is unusable for logging while the TUI
/// owns the terminal (raw mode + alternate screen), so logs go to a file
/// instead — `tail -f rack.log` in another terminal is the way to watch
/// them live.
const DEFAULT_LOG_FILE: &str = "rack.log";

/// Fixed width of every module panel in the rack, borders included.
const PANEL_WIDTH: u16 = 22;

/// How a module's controls are drawn — chosen per `module_kind`, so the rack
/// reads like real hardware (a filter's knobs look different from an LFO's
/// fader) rather than one uniform list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlStyle {
    Knob,
    Fader,
    Bar,
}

impl ControlStyle {
    fn for_kind(kind: &str) -> Self {
        match kind {
            "vco" | "vcf" | "vca" => ControlStyle::Knob,
            "lfo" => ControlStyle::Fader,
            "io" => ControlStyle::Bar,
            _ => ControlStyle::Bar,
        }
    }

    /// Rows of panel interior a single control needs when drawn in this
    /// style (label + glyph/bar + value, or a taller fader column).
    fn row_height(self) -> u16 {
        match self {
            ControlStyle::Knob => 3,
            ControlStyle::Fader => 6,
            ControlStyle::Bar => 1,
        }
    }
}

/// One bordered rack panel: a module's display name/kind plus the indices
/// (into `App::controls`/`App::values`) of the controls it owns. Built once
/// from the flat `ListControls` response, grouping consecutive controls that
/// share a module index — the engine already emits them grouped that way.
struct ModulePanel {
    module: u32,
    module_name: String,
    style: ControlStyle,
    control_indices: Vec<usize>,
}

fn build_panels(controls: &[ControlDescriptor]) -> Vec<ModulePanel> {
    let mut panels: Vec<ModulePanel> = Vec::new();
    for (i, c) in controls.iter().enumerate() {
        match panels.last_mut() {
            Some(last) if last.module == c.module => last.control_indices.push(i),
            _ => panels.push(ModulePanel {
                module: c.module,
                module_name: c.module_name.clone(),
                style: ControlStyle::for_kind(&c.module_kind),
                control_indices: vec![i],
            }),
        }
    }
    panels
}

impl ModulePanel {
    fn height(&self) -> u16 {
        2 + self.control_indices.len() as u16 * self.style.row_height()
    }
}

struct App {
    controls: Vec<ControlDescriptor>,
    values: Vec<f32>,
    panels: Vec<ModulePanel>,
    selected: usize,
    active_voices: u32,
    output_peak: f32,
    /// Most recent block of the mixed output, for the oscilloscope panel.
    scope: Vec<f32>,
    /// Patch-owned IO endpoints from `ListIoEndpoints`.
    io_endpoints: Vec<IoEndpointDescriptor>,
    /// Live meters keyed by endpoint id (from `StreamState`).
    io_meters: Vec<IoChannelMeter>,
    /// Host devices for the device picker (`ListAudioDevices`).
    audio_devices: Vec<AudioDevice>,
    /// Which IO endpoint is focused for device cycling (`[` / `]`).
    selected_io: usize,
    held_notes: HashSet<u8>,
    quit: bool,
    state_stream_ended: bool,
}

impl App {
    fn new(
        controls: Vec<ControlDescriptor>,
        io_endpoints: Vec<IoEndpointDescriptor>,
        audio_devices: Vec<AudioDevice>,
    ) -> Self {
        let values = controls.iter().map(|c| c.default_value).collect();
        let panels = build_panels(&controls);
        Self {
            controls,
            values,
            panels,
            selected: 0,
            active_voices: 0,
            output_peak: 0.0,
            scope: Vec::new(),
            io_endpoints,
            io_meters: Vec::new(),
            audio_devices,
            selected_io: 0,
            held_notes: HashSet::new(),
            quit: false,
            state_stream_ended: false,
        }
    }

    fn selected_io_endpoint(&self) -> Option<&IoEndpointDescriptor> {
        self.io_endpoints.get(self.selected_io)
    }

    /// Devices that match the selected endpoint's direction.
    fn devices_for_selected_io(&self) -> Vec<&AudioDevice> {
        let Some(ep) = self.selected_io_endpoint() else {
            return Vec::new();
        };
        let want_input = ep.direction == "input";
        self.audio_devices
            .iter()
            .filter(|d| if want_input { d.is_input } else { d.is_output })
            .collect()
    }

    /// Cycle the selected IO endpoint to the next matching host device.
    fn next_device_name_for_selected_io(&self) -> Option<String> {
        let ep = self.selected_io_endpoint()?;
        let devices = self.devices_for_selected_io();
        if devices.is_empty() {
            return None;
        }
        let current = self
            .io_meters
            .iter()
            .find(|m| m.endpoint_id == ep.id)
            .map(|m| m.device_name.as_str())
            .unwrap_or(ep.device_name.as_str());
        let idx = devices
            .iter()
            .position(|d| d.name == current)
            .map(|i| (i + 1) % devices.len())
            .unwrap_or(0);
        Some(devices[idx].name.clone())
    }

    fn select_prev(&mut self) {
        if !self.controls.is_empty() {
            self.selected = self.selected.saturating_sub(1);
            trace!(selected = self.selected, "select_prev");
        }
    }

    fn select_next(&mut self) {
        if !self.controls.is_empty() {
            self.selected = (self.selected + 1).min(self.controls.len() - 1);
            trace!(selected = self.selected, "select_next");
        }
    }

    fn panel_of(&self, control: usize) -> usize {
        self.panels
            .iter()
            .position(|p| p.control_indices.contains(&control))
            .unwrap_or(0)
    }

    /// Jumps selection to the previous/next module panel's first control —
    /// Up/Down moves across panels, distinct from Left/Right's flat
    /// control-by-control movement.
    fn select_prev_panel(&mut self) {
        let panel = self.panel_of(self.selected);
        if panel > 0 {
            self.selected = self.panels[panel - 1].control_indices[0];
            trace!(panel = panel - 1, selected = self.selected, "select_prev_panel");
        }
    }

    fn select_next_panel(&mut self) {
        let panel = self.panel_of(self.selected);
        if panel + 1 < self.panels.len() {
            self.selected = self.panels[panel + 1].control_indices[0];
            trace!(panel = panel + 1, selected = self.selected, "select_next_panel");
        }
    }

    /// Adjusts the selected control's value by a fraction of its range,
    /// clamped to `[min, max]`. Returns the new value if a control is
    /// selected, so the caller can push it to the engine.
    fn adjust_selected(&mut self, direction: f32) -> Option<(u32, u32, f32)> {
        let control = self.controls.get(self.selected)?;
        let step = (control.max - control.min) * 0.02;
        let value = &mut self.values[self.selected];
        *value = (*value + direction * step).clamp(control.min, control.max);
        debug!(name = control.name, value = *value, "adjust_selected");
        Some((control.module, control.param, *value))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Log to a file, never stdout: the TUI owns the terminal (raw mode +
    // alternate screen) for the whole run, so any stray stdout write would
    // corrupt the display. `_guard` must stay alive for the whole process —
    // dropping it flushes the non-blocking writer.
    let log_path = std::env::var("RACK_LOG").unwrap_or_else(|_| DEFAULT_LOG_FILE.to_string());
    let log_dir = std::path::Path::new(&log_path).parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| std::path::Path::new("."));
    let log_name = std::path::Path::new(&log_path).file_name().unwrap_or_else(|| std::ffi::OsStr::new(DEFAULT_LOG_FILE));
    let file_appender = tracing_appender::rolling::never(log_dir, log_name);
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    info!(version = env!("CARGO_PKG_VERSION"), log_path = %log_path, "starting rack");

    let addr = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_ENGINE_ADDR.to_string());
    info!(addr, "connecting to engine");
    let mut client = SynthClient::connect(addr.clone()).await?;
    info!(addr, "connected to engine");

    let controls = client.list_controls(Empty {}).await?.into_inner().controls;
    info!(count = controls.len(), "fetched control list");
    let io_endpoints = client
        .list_io_endpoints(Empty {})
        .await?
        .into_inner()
        .endpoints;
    info!(count = io_endpoints.len(), "fetched IO endpoints");
    let audio_devices = client
        .list_audio_devices(Empty {})
        .await?
        .into_inner()
        .devices;
    info!(count = audio_devices.len(), "fetched audio devices");
    let mut app = App::new(controls, io_endpoints, audio_devices);
    debug!(panels = app.panels.len(), "built rack panels");

    let mut state_stream = client.stream_state(Empty {}).await?.into_inner();
    debug!("subscribed to engine state stream");

    debug!("entering raw mode / alternate screen");
    enable_raw_mode()?;
    std::io::stdout().execute(EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut events = EventStream::new();

    let result = async {
        while !app.quit {
            terminal.draw(|frame| draw(frame, &app))?;

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
                            trace!(active_voices = state.active_voices, output_peak = state.output_peak, "engine state update");
                            app.active_voices = state.active_voices;
                            app.output_peak = state.output_peak;
                            app.scope = state.waveform;
                            app.io_meters = state.io_meters;
                        }
                        Some(Err(err)) => {
                            warn!(%err, "state stream error");
                        }
                        None => {
                            // Engine dropped the stream (e.g. process exited);
                            // keep the UI up but stop polling this branch.
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

async fn handle_key(app: &mut App, client: &mut SynthClient<tonic::transport::Channel>, code: KeyCode) {
    trace!(?code, "key press");
    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            info!("quit requested");
            app.quit = true;
        }
        // Left/Right (h/l): move between controls in flat order.
        KeyCode::Left | KeyCode::Char('h') => app.select_prev(),
        KeyCode::Right | KeyCode::Char('l') => app.select_next(),
        // Up/Down (k/j): jump to the previous/next module panel.
        KeyCode::Up | KeyCode::Char('k') => app.select_prev_panel(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next_panel(),
        // -/= (and ,/.): adjust the selected control's value.
        KeyCode::Char('-') | KeyCode::Char(',') => {
            if let Some((module, param, value)) = app.adjust_selected(-1.0) {
                match client.set_param(SetParamRequest { module, param, value }).await {
                    Ok(_) => trace!(module, param, value, "set_param ok"),
                    Err(err) => warn!(%err, module, param, value, "set_param failed"),
                }
            }
        }
        KeyCode::Char('=') | KeyCode::Char('.') => {
            if let Some((module, param, value)) = app.adjust_selected(1.0) {
                match client.set_param(SetParamRequest { module, param, value }).await {
                    Ok(_) => trace!(module, param, value, "set_param ok"),
                    Err(err) => warn!(%err, module, param, value, "set_param failed"),
                }
            }
        }
        // [ / ]: select IO endpoint for device cycling.
        KeyCode::Char('[') => {
            if !app.io_endpoints.is_empty() {
                app.selected_io = app.selected_io.saturating_sub(1);
            }
        }
        KeyCode::Char(']') => {
            if !app.io_endpoints.is_empty() {
                app.selected_io = (app.selected_io + 1).min(app.io_endpoints.len() - 1);
            }
        }
        // d: cycle device for the focused IO endpoint (patch binding).
        KeyCode::Char('d') => {
            if let (Some(ep), Some(device_name)) = (
                app.selected_io_endpoint().map(|e| e.id.clone()),
                app.next_device_name_for_selected_io(),
            ) {
                match client
                    .set_io_device(SetIoDeviceRequest {
                        endpoint_id: ep.clone(),
                        device_name: device_name.clone(),
                    })
                    .await
                {
                    Ok(_) => {
                        info!(endpoint = %ep, device = %device_name, "set_io_device ok");
                        // Refresh endpoint list so the panel shows the new binding.
                        if let Ok(resp) = client.list_io_endpoints(Empty {}).await {
                            app.io_endpoints = resp.into_inner().endpoints;
                        }
                        if let Ok(resp) = client.list_audio_devices(Empty {}).await {
                            app.audio_devices = resp.into_inner().devices;
                        }
                    }
                    Err(err) => warn!(%err, endpoint = %ep, device = %device_name, "set_io_device failed"),
                }
            }
        }
        KeyCode::Char(c) => {
            if let Some(&(_, note)) = NOTE_KEYS.iter().find(|(k, _)| *k == c) {
                if app.held_notes.remove(&note) {
                    match client.note_off(NoteOffRequest { note: note as u32 }).await {
                        Ok(_) => debug!(note, "note_off ok"),
                        Err(err) => warn!(%err, note, "note_off failed"),
                    }
                } else {
                    app.held_notes.insert(note);
                    match client.note_on(NoteOnRequest { note: note as u32, velocity: 100 }).await {
                        Ok(_) => debug!(note, "note_on ok"),
                        Err(err) => warn!(%err, note, "note_on failed"),
                    }
                }
            }
        }
        _ => {}
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let columns = (area.width / PANEL_WIDTH).max(1) as usize;

    // Chunk panels into rack rows of `columns` panels each; a row's height
    // is its tallest panel's height, so shorter panels in the same row just
    // leave blank space below their border.
    let rows: Vec<&[ModulePanel]> = app.panels.chunks(columns).collect();
    let row_heights: Vec<u16> = rows
        .iter()
        .map(|row| row.iter().map(ModulePanel::height).max().unwrap_or(0))
        .collect();

    let io_row_height: u16 = 4;
    let mut constraints: Vec<Constraint> = row_heights.iter().map(|h| Constraint::Length(*h)).collect();
    constraints.push(Constraint::Length(io_row_height)); // IO modules
    constraints.push(Constraint::Length(3)); // meter panel
    constraints.push(Constraint::Length(3)); // keyboard panel
    constraints.push(Constraint::Min(0)); // leftover / scope

    let outer = Layout::default().direction(Direction::Vertical).constraints(constraints).split(area);

    for (row_idx, row) in rows.iter().enumerate() {
        let mut col_constraints: Vec<Constraint> = row.iter().map(|_| Constraint::Length(PANEL_WIDTH)).collect();
        col_constraints.push(Constraint::Min(0));
        let cols = Layout::default().direction(Direction::Horizontal).constraints(col_constraints).split(outer[row_idx]);

        for (col_idx, panel) in row.iter().enumerate() {
            draw_panel(frame, app, panel, cols[col_idx]);
        }
    }

    let io_idx = rows.len();
    draw_io_row(frame, app, outer[io_idx]);
    draw_meter(frame, app, outer[io_idx + 1]);
    draw_keyboard(frame, app, outer[io_idx + 2]);
    if outer.len() > io_idx + 3 {
        draw_scope(frame, app, outer[io_idx + 3]);
    }
}

fn draw_io_row(frame: &mut Frame, app: &App, area: Rect) {
    if app.io_endpoints.is_empty() {
        let empty = Paragraph::new("No IO endpoints").block(
            Block::default()
                .borders(Borders::ALL)
                .title("IO (patch)"),
        );
        frame.render_widget(empty, area);
        return;
    }

    let n = app.io_endpoints.len();
    let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Ratio(1, n as u32)).collect();
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, ep) in app.io_endpoints.iter().enumerate() {
        draw_io_panel(frame, app, ep, i == app.selected_io, cols[i]);
    }
}

fn draw_io_panel(
    frame: &mut Frame,
    app: &App,
    ep: &IoEndpointDescriptor,
    selected: bool,
    area: Rect,
) {
    let meter = app
        .io_meters
        .iter()
        .find(|m| m.endpoint_id == ep.id);
    let device = meter
        .map(|m| m.device_name.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(ep.device_name.as_str());
    let connected = meter.map(|m| m.connected).unwrap_or(false);
    let peaks = meter.map(|m| m.peak.as_slice()).unwrap_or(&[]);

    let peak_label = if peaks.is_empty() {
        "—".to_string()
    } else {
        peaks
            .iter()
            .map(|p| format!("{:.2}", p))
            .collect::<Vec<_>>()
            .join(" ")
    };

    let status = if connected { "ok" } else { "off" };
    let title = if selected {
        format!("▶ {} [{}]", ep.name, status)
    } else {
        format!("{} [{}]", ep.name, status)
    };

    let border_style = if selected {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let max_peak = peaks.iter().copied().fold(0.0f32, f32::max).clamp(0.0, 1.0);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let device_line = Paragraph::new(Line::from(Span::raw(truncate_str(device, rows[0].width as usize))));
    frame.render_widget(device_line, rows[0]);

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(if connected {
            Color::Green
        } else {
            Color::DarkGray
        }))
        .label(peak_label)
        .ratio(max_peak as f64);
    frame.render_widget(gauge, rows[1]);
}

fn truncate_str(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else if max <= 1 {
        s.chars().take(max).collect()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

fn draw_panel(frame: &mut Frame, app: &App, panel: &ModulePanel, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(panel.module_name.clone());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let row_height = panel.style.row_height();
    let constraints: Vec<Constraint> = panel.control_indices.iter().map(|_| Constraint::Length(row_height)).collect();
    let rows = Layout::default().direction(Direction::Vertical).constraints(constraints).split(inner);

    for (row, &control_idx) in panel.control_indices.iter().enumerate() {
        draw_control(frame, app, control_idx, panel.style, rows[row]);
    }
}

fn draw_control(frame: &mut Frame, app: &App, control_idx: usize, style: ControlStyle, area: Rect) {
    let control = &app.controls[control_idx];
    let value = app.values[control_idx];
    let ratio = ((value - control.min) / (control.max - control.min)).clamp(0.0, 1.0);
    let selected = control_idx == app.selected;

    match style {
        ControlStyle::Knob => draw_knob(frame, control, value, ratio, selected, area),
        ControlStyle::Fader => draw_fader(frame, control, value, ratio, selected, area),
        ControlStyle::Bar => draw_bar(frame, control, value, ratio, selected, area),
    }
}

/// A rotary-pointer glyph, chosen from `ratio` — the filter/VCA control
/// style, closest to an analog knob.
const KNOB_POINTERS: [&str; 8] = ["↑", "↗", "→", "↘", "↓", "↙", "←", "↖"];

fn draw_knob(frame: &mut Frame, control: &ControlDescriptor, value: f32, ratio: f32, selected: bool, area: Rect) {
    let pointer = KNOB_POINTERS[((ratio * (KNOB_POINTERS.len() - 1) as f32).round() as usize).min(KNOB_POINTERS.len() - 1)];
    let style = if selected { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
    let text = vec![
        Line::from(Span::styled(control.name.clone(), style)),
        Line::from(Span::styled(format!("  ({pointer})  "), style)),
        Line::from(Span::styled(format!("{value:.2}"), style)),
    ];
    frame.render_widget(Paragraph::new(text), area);
}

/// A vertical fill column — the LFO control style, read top-to-bottom like a
/// mixer-strip fader.
fn draw_fader(frame: &mut Frame, control: &ControlDescriptor, value: f32, ratio: f32, selected: bool, area: Rect) {
    let style = if selected { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
    let bar_rows = 4u16;
    let filled = (ratio * bar_rows as f32).round() as u16;

    let mut lines = vec![Line::from(Span::styled(control.name.clone(), style))];
    for row in 0..bar_rows {
        let is_filled = row >= bar_rows - filled;
        let glyph = if is_filled { "██████" } else { "░░░░░░" };
        lines.push(Line::from(Span::styled(glyph, style)));
    }
    lines.push(Line::from(Span::styled(format!("{value:.2}"), style)));

    frame.render_widget(Paragraph::new(lines), area);
}

/// A single-row horizontal fill bar — the default control style (envelopes,
/// attenuverters, anything without a more specific look).
fn draw_bar(frame: &mut Frame, control: &ControlDescriptor, value: f32, ratio: f32, selected: bool, area: Rect) {
    let color = if selected { Color::Yellow } else { Color::Green };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(color))
        .label(format!("{} {:.2}", control.name, value))
        .ratio(ratio as f64);
    frame.render_widget(gauge, area);
}

fn draw_meter(frame: &mut Frame, app: &App, area: Rect) {
    let meter_ratio = app.output_peak.clamp(0.0, 1.0) as f64;
    let meter = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(format!(
            "Output ({} voice{} active)",
            app.active_voices,
            if app.active_voices == 1 { "" } else { "s" }
        )))
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(meter_ratio);
    frame.render_widget(meter, area);
}

fn draw_keyboard(frame: &mut Frame, app: &App, area: Rect) {
    let held: Vec<Span> = NOTE_KEYS
        .iter()
        .map(|(k, note)| {
            let style = if app.held_notes.contains(note) {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default()
            };
            Span::styled(format!(" {k} "), style)
        })
        .collect();
    let keyboard = Paragraph::new(Line::from(held)).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Keyboard — ←/→ select, -/= adjust, ↑/↓ panel, [/] IO, d device, q quit"),
    );
    frame.render_widget(keyboard, area);
}

/// A live waveform view of the mixed output: the most recent block of
/// samples, drawn into a character grid as a single trace. Renders nothing
/// (just the bordered panel) until the first block arrives.
fn draw_scope(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Oscilloscope");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let height = inner.height as usize;
    if width == 0 || height == 0 || app.scope.is_empty() {
        return;
    }

    // One marker column per cell of width; sample the block evenly so a
    // wide panel just shows more of the trace.
    let n = app.scope.len();
    let markers: Vec<(usize, usize)> = (0..width)
        .map(|x| {
            let idx = (x * n) / width;
            let v = app.scope[idx].clamp(-1.0, 1.0);
            // Map [-1,1] -> row [height-1 (bottom) .. 0 (top)].
            let row = (((1.0 - (v * 0.5 + 0.5)) * (height - 1) as f32).round() as usize).min(height - 1);
            (x, row)
        })
        .collect();

    let mut grid = vec![vec![' '; width]; height];
    for (x, row) in markers {
        grid[row][x] = '\u{2022}'; // bullet
    }

    let lines: Vec<Line> = grid
        .into_iter()
        .map(|row| Line::from(row.into_iter().collect::<String>()))
        .collect();
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}
