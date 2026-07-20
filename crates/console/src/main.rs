//! `console`: a ratatui TUI client for a running `engine` process. Connects
//! over gRPC (see `synth-proto`), renders the engine's controls as a rack of
//! bordered module panels (one panel per module, grouped by the
//! `ListControls` response's `module`/`module_name`), each drawn in a style
//! that fits its `module_kind` — knobs for a filter/VCA, a vertical fader for
//! the LFO, horizontal bars for everything else — plus a level-meter panel
//! and a toggle-style on-screen-keyboard panel.

use std::collections::HashSet;

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use futures::{FutureExt, StreamExt};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::{Frame, Terminal};
use synth_proto::synth_client::SynthClient;
use synth_proto::{ControlDescriptor, Empty, NoteOffRequest, NoteOnRequest, SetParamRequest};
use tracing::{info, warn};
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
/// `CONSOLE_LOG=/path/to/file`. Stdout is unusable for logging while the TUI
/// owns the terminal (raw mode + alternate screen), so logs go to a file
/// instead — `tail -f console.log` in another terminal is the way to watch
/// them live.
const DEFAULT_LOG_FILE: &str = "console.log";

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
    held_notes: HashSet<u8>,
    quit: bool,
    state_stream_ended: bool,
}

impl App {
    fn new(controls: Vec<ControlDescriptor>) -> Self {
        let values = controls.iter().map(|c| c.default_value).collect();
        let panels = build_panels(&controls);
        Self {
            controls,
            values,
            panels,
            selected: 0,
            active_voices: 0,
            output_peak: 0.0,
            held_notes: HashSet::new(),
            quit: false,
            state_stream_ended: false,
        }
    }

    fn select_prev(&mut self) {
        if !self.controls.is_empty() {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    fn select_next(&mut self) {
        if !self.controls.is_empty() {
            self.selected = (self.selected + 1).min(self.controls.len() - 1);
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
        }
    }

    fn select_next_panel(&mut self) {
        let panel = self.panel_of(self.selected);
        if panel + 1 < self.panels.len() {
            self.selected = self.panels[panel + 1].control_indices[0];
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
        Some((control.module, control.param, *value))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Log to a file, never stdout: the TUI owns the terminal (raw mode +
    // alternate screen) for the whole run, so any stray stdout write would
    // corrupt the display. `_guard` must stay alive for the whole process —
    // dropping it flushes the non-blocking writer.
    let log_path = std::env::var("CONSOLE_LOG").unwrap_or_else(|_| DEFAULT_LOG_FILE.to_string());
    let log_dir = std::path::Path::new(&log_path).parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| std::path::Path::new("."));
    let log_name = std::path::Path::new(&log_path).file_name().unwrap_or_else(|| std::ffi::OsStr::new(DEFAULT_LOG_FILE));
    let file_appender = tracing_appender::rolling::never(log_dir, log_name);
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let addr = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_ENGINE_ADDR.to_string());
    info!(addr, "connecting to engine");
    let mut client = SynthClient::connect(addr).await?;

    let controls = client.list_controls(Empty {}).await?.into_inner().controls;
    info!(count = controls.len(), "fetched control list");
    let mut app = App::new(controls);

    let mut state_stream = client.stream_state(Empty {}).await?.into_inner();

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
                            app.active_voices = state.active_voices;
                            app.output_peak = state.output_peak;
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
                if let Err(err) = client.set_param(SetParamRequest { module, param, value }).await {
                    warn!(%err, module, param, value, "set_param failed");
                }
            }
        }
        KeyCode::Char('=') | KeyCode::Char('.') => {
            if let Some((module, param, value)) = app.adjust_selected(1.0) {
                if let Err(err) = client.set_param(SetParamRequest { module, param, value }).await {
                    warn!(%err, module, param, value, "set_param failed");
                }
            }
        }
        KeyCode::Char(c) => {
            if let Some(&(_, note)) = NOTE_KEYS.iter().find(|(k, _)| *k == c) {
                if app.held_notes.remove(&note) {
                    if let Err(err) = client.note_off(NoteOffRequest { note: note as u32 }).await {
                        warn!(%err, note, "note_off failed");
                    }
                } else {
                    app.held_notes.insert(note);
                    if let Err(err) = client
                        .note_on(NoteOnRequest { note: note as u32, velocity: 100 })
                        .await
                    {
                        warn!(%err, note, "note_on failed");
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

    let mut constraints: Vec<Constraint> = row_heights.iter().map(|h| Constraint::Length(*h)).collect();
    constraints.push(Constraint::Length(3)); // meter panel
    constraints.push(Constraint::Length(3)); // keyboard panel
    constraints.push(Constraint::Min(0)); // leftover space

    let outer = Layout::default().direction(Direction::Vertical).constraints(constraints).split(area);

    for (row_idx, row) in rows.iter().enumerate() {
        let mut col_constraints: Vec<Constraint> = row.iter().map(|_| Constraint::Length(PANEL_WIDTH)).collect();
        col_constraints.push(Constraint::Min(0));
        let cols = Layout::default().direction(Direction::Horizontal).constraints(col_constraints).split(outer[row_idx]);

        for (col_idx, panel) in row.iter().enumerate() {
            draw_panel(frame, app, panel, cols[col_idx]);
        }
    }

    draw_meter(frame, app, outer[rows.len()]);
    draw_keyboard(frame, app, outer[rows.len() + 1]);
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
            .title("Keyboard \u{2014} \u{2190}/\u{2192} select, -/= adjust, \u{2191}/\u{2193} panel, q quit"),
    );
    frame.render_widget(keyboard, area);
}
