//! MCP screen: view over the global MCP server registry — the same
//! store the `awh mcp` CLI manages and `/api/v1/mcp` serves. Rows show
//! id, transport, enabled flag, and installed version; the screen is
//! deliberately read-only (registry mutations go through the CLI,
//! which validates and audits them).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Paragraph};

use super::hint_line;
use crate::tui::app::App;
use crate::tui::backend::WorkspaceBackend;

#[derive(Default)]
pub struct McpUi {
    /// Registry rows loaded on demand.
    pub servers: Vec<crate::tui::remote::McpInfo>,
}

impl McpUi {
    pub fn load<B: WorkspaceBackend>(&mut self, backend: &B) -> Result<(), String> {
        let servers = backend
            .list_mcp_servers()
            .map_err(|e| format!("read MCP registry: {e:#}"))?;
        self.servers = servers;
        Ok(())
    }
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    if key.code == KeyCode::Char('r') {
        if let Err(msg) = app.ui.mcp_ui.load(&app.backend) {
            app.set_error(msg);
        } else {
            app.set_message("MCP registry reloaded");
        }
    }
}

pub fn draw<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: Block<'_>,
) {
    if app.ui.mcp_ui.servers.is_empty() {
        if let Err(msg) = app.ui.mcp_ui.load(&app.backend) {
            app.set_error(msg);
        }
    }

    let ui = &app.ui.mcp_ui;
    let mut lines = vec![ratatui::text::Line::from(ratatui::text::Span::styled(
        "id                    transport       enabled  version",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))];
    if ui.servers.is_empty() {
        lines.push(ratatui::text::Line::from("no MCP servers registered"));
        lines.push(ratatui::text::Line::from(
            "add one with: awh mcp add <id> --transport stdio --command <cmd>",
        ));
    }
    for s in &ui.servers {
        lines.push(ratatui::text::Line::from(vec![
            ratatui::text::Span::raw(format!("{:<21} ", truncate(&s.id, 21))),
            ratatui::text::Span::raw(format!("{:<16} ", truncate(&s.transport, 16))),
            ratatui::text::Span::styled(
                format!("{:<9}", if s.enabled { "yes" } else { "no" }),
                Style::default().fg(if s.enabled {
                    Color::Green
                } else {
                    Color::DarkGray
                }),
            ),
            ratatui::text::Span::raw(truncate(&s.version, 30)),
        ]));
    }
    lines.push(ratatui::text::Line::from(""));
    lines.push(ratatui::text::Line::from(format!(
        "{} registered server(s)",
        ui.servers.len()
    )));

    hint_line(
        frame,
        shrink(area),
        "[r] refresh  [Esc] back  manage via: awh mcp --help",
    );
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{cut}\u{2026}")
    }
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
