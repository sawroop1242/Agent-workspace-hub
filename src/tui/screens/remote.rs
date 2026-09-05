//! Remote screen: connection manager for LAN/cloud AWH instances.
//!
//! The screen performs a REAL handshake against a user-supplied
//! Control API base URL: `/healthz` for liveness, then authenticated
//! `/status` for compatibility (spec section 21's connection state
//! machine: local / connecting / connected / auth-failed /
//! unavailable / incompatible). Connecting switches the whole TUI to
//! the remote backend — restart with `awh tui --remote <url>` to
//! operate the remote workspace from every screen; this screen
//! diagnoses why a connection would or would not work first.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Paragraph};

use super::hint_line;
use crate::tui::app::App;
use crate::tui::backend::WorkspaceBackend;
use crate::tui::remote::{ConnectionState, RemoteBackend};

#[derive(Default)]
pub struct RemoteUi {
    /// Base URL being composed (e.g. http://127.0.0.1:8080).
    pub url_input: String,
    pub url_active: bool,
    /// Result of the most recent probe.
    pub state: Option<ConnectionState>,
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    let ui = &mut app.ui.remote_ui;
    if ui.url_active {
        match key.code {
            KeyCode::Enter => {
                ui.url_active = false;
                let url = ui.url_input.trim().to_owned();
                ui.url_input.clear();
                if url.is_empty() {
                    app.set_error("URL cannot be empty");
                } else {
                    ui.state = Some(ConnectionState::Connecting);
                    let backend = RemoteBackend::new(&url, "");
                    // The probe result distinguishes every failure mode;
                    // a missing key still lets liveness/version be tested.
                    let state = backend.probe();
                    if state == ConnectionState::AuthFailed {
                        // Liveness + version are knowable without auth.
                        ui.state = Some(ConnectionState::AuthFailed);
                        app.set_message("server reachable — API key rejected (or unset)");
                    } else {
                        ui.state = Some(state);
                    }
                }
            }
            KeyCode::Esc => {
                ui.url_active = false;
                ui.url_input.clear();
            }
            KeyCode::Backspace => {
                ui.url_input.pop();
            }
            KeyCode::Char(c) => ui.url_input.push(c),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('u') => ui.url_active = true,
        KeyCode::Esc => ui.state = None,
        _ => {}
    }
}

pub fn draw<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: Block<'_>,
) {
    let ui = &app.ui.remote_ui;
    let mut lines: Vec<ratatui::text::Line> = Vec::new();

    if ui.url_active {
        lines.push(plain("Target base URL for the remote Control API:"));
        lines.push(input_line("url> ", &ui.url_input));
        lines.push(plain("press Enter to test, Esc to cancel"));
    } else {
        lines.push(labeled(
            "Status",
            match &ui.state {
                None => "local (no probe this session)".to_owned(),
                Some(ConnectionState::Local) => "local".to_owned(),
                Some(ConnectionState::Connecting) => "connecting…".to_owned(),
                Some(ConnectionState::Connected { version, uptime }) => {
                    format!("connected — server {version}, up {uptime}s")
                }
                Some(ConnectionState::AuthFailed) => "auth failed — key rejected".to_owned(),
                Some(ConnectionState::Unavailable { reason }) => {
                    format!("unavailable — {reason}")
                }
                Some(ConnectionState::Incompatible { server_version }) => format!(
                    "incompatible — server {server_version} vs client {}",
                    env!("CARGO_PKG_VERSION")
                ),
            },
            state_color(&ui.state),
        ));
        if matches!(
            ui.state,
            Some(ConnectionState::Connected { .. }) | Some(ConnectionState::AuthFailed)
        ) {
            lines.push(plain(""));
            lines.push(labeled(
                "Connect",
                format!(
                    "awh tui --remote {} --api-key-env AWH_API_KEY",
                    "https://your-host:8080"
                ),
                Color::Cyan,
            ));
            lines.push(plain("restart the TUI on the remote backend; every"));
            lines.push(plain("screen then operates through the remote API."));
        }
        if matches!(ui.state, Some(ConnectionState::AuthFailed)) {
            lines.push(plain(""));
            lines.push(warn(
                "export AWH_API_KEY=<key> first — keys are never typed into this screen",
            ));
        }
        lines.push(plain(""));
        lines.push(labeled("Probe", "[u] enter URL", Color::Cyan));
        lines.push(labeled("Clear", "[Esc] forget result", Color::Cyan));
    }

    hint_line(
        frame,
        shrink(area),
        "[u] test URL  [Esc] back  probes healthz + status",
    );
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn state_color(state: &Option<ConnectionState>) -> Color {
    match state {
        Some(ConnectionState::Connected { .. }) => Color::Green,
        Some(ConnectionState::AuthFailed | ConnectionState::Incompatible { .. }) => Color::Red,
        Some(ConnectionState::Unavailable { .. }) => Color::Yellow,
        _ => Color::Gray,
    }
}

fn plain(s: &str) -> ratatui::text::Line<'static> {
    ratatui::text::Line::from(s.to_owned())
}

fn warn(s: &str) -> ratatui::text::Line<'static> {
    ratatui::text::Line::from(ratatui::text::Span::styled(
        s.to_owned(),
        Style::default().fg(Color::Yellow),
    ))
}

fn labeled(label: &str, value: impl Into<String>, color: Color) -> ratatui::text::Line<'static> {
    ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(
            format!("{label:<9} "),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        ratatui::text::Span::styled(value.into(), Style::default().fg(color)),
    ])
}

fn input_line(prompt: &str, buffer: &str) -> ratatui::text::Line<'static> {
    ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(prompt.to_owned(), Style::default().fg(Color::Cyan)),
        ratatui::text::Span::raw(buffer.to_owned()),
        ratatui::text::Span::styled("_", Style::default().fg(Color::DarkGray)),
    ])
}

/// Inner area one cell inside the border, for the hint line.
fn shrink(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}
