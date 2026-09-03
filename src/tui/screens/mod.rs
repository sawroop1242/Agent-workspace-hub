//! Screen registry and dispatch.
//!
//! Each screen lives in its own module with a `handle_key` + `draw`
//! pair operating on shared [`App`] state plus its own UI struct held
//! in [`ScreenState`]. Dashboard, Projects, Files, Editor, Git, and
//! Terminal are fully interactive; later phases fill in the rest.
pub mod context;
pub mod editor;
pub mod files;
pub mod git;
pub mod logs;
pub mod memory;
pub mod projects;
pub mod skills;
pub mod terminal;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Paragraph};

use super::app::App;
use super::backend::WorkspaceBackend;

/// All navigable screens, in tab-ring order (spec Section 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenId {
    Dashboard,
    Projects,
    Files,
    Editor,
    Git,
    Terminal,
    Mcp,
    Context,
    Memory,
    Skills,
    Logs,
    Settings,
    Remote,
    Help,
}

/// Static screen metadata; also drives the tab ring.
pub struct ScreenMeta {
    pub id: ScreenId,
    pub title: &'static str,
    /// One-line description shown on the Help screen.
    pub blurb: &'static str,
}

pub const SCREENS: &[ScreenMeta] = &[
    ScreenMeta {
        id: ScreenId::Dashboard,
        title: "Dashboard",
        blurb: "Workspace overview: projects, Git state, sessions, warnings",
    },
    ScreenMeta {
        id: ScreenId::Projects,
        title: "Projects",
        blurb: "Create, open, delete projects (destructive actions confirm)",
    },
    ScreenMeta {
        id: ScreenId::Files,
        title: "Files",
        blurb: "Browse, read, and edit files within workspace boundaries",
    },
    ScreenMeta {
        id: ScreenId::Editor,
        title: "Editor",
        blurb: "Text editor with dirty state and safe large-file refusal",
    },
    ScreenMeta {
        id: ScreenId::Git,
        title: "Git",
        blurb: "Status, diff, log, stage, commit over structured Git",
    },
    ScreenMeta {
        id: ScreenId::Terminal,
        title: "Terminal",
        blurb: "Bounded argv command execution with output capture",
    },
    ScreenMeta {
        id: ScreenId::Mcp,
        title: "MCP",
        blurb: "MCP server status, tools, connectors",
    },
    ScreenMeta {
        id: ScreenId::Context,
        title: "Context",
        blurb: "Context engine items, budgets, offloading",
    },
    ScreenMeta {
        id: ScreenId::Memory,
        title: "Memory",
        blurb: "Long-term memory entries by scope",
    },
    ScreenMeta {
        id: ScreenId::Skills,
        title: "Skills",
        blurb: "Installed and project-referenced skills",
    },
    ScreenMeta {
        id: ScreenId::Logs,
        title: "Logs",
        blurb: "Recent runtime and audit logs",
    },
    ScreenMeta {
        id: ScreenId::Settings,
        title: "Settings",
        blurb: "Workspace configuration and preferences",
    },
    ScreenMeta {
        id: ScreenId::Remote,
        title: "Remote",
        blurb: "Connect to LAN or cloud AWH over HTTPS",
    },
    ScreenMeta {
        id: ScreenId::Help,
        title: "Help",
        blurb: "Keybindings and navigation",
    },
];

/// Per-screen mutable UI state (selections, inputs, buffers).
#[derive(Default)]
pub struct ScreenState {
    pub projects: ratatui::widgets::ListState,
    pub files: ratatui::widgets::ListState,
    pub git: ratatui::widgets::ListState,
    pub projects_ui: projects::ProjectsUi,
    pub files_ui: files::FilesUi,
    pub editor_ui: editor::EditorUi,
    pub git_ui: git::GitUi,
    pub terminal_ui: terminal::TerminalUi,
    pub context_ui: context::ContextUi,
    pub memory_ui: memory::MemoryUi,
    pub skills_ui: skills::SkillsUi,
    /// Bounded view over the shared audit ring for the Logs screen.
    pub logs: Vec<crate::services::audit::AuditEntry>,
    /// Content produced by a DiscardChanges action, adopted by the
    /// Editor screen on its next draw.
    pub reload_content: Option<(String, String)>,
}

/// Dispatches a key press to the active screen.
pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: crossterm::event::KeyEvent) {
    // Errors are dismissed from any screen.
    if app.error.is_some() && key.code == KeyCode::Char('x') {
        app.error = None;
        return;
    }
    match app.screen() {
        ScreenId::Dashboard => dashboard_key(app, key),
        ScreenId::Projects => projects::handle_key(app, key),
        ScreenId::Files => files::handle_key(app, key),
        ScreenId::Editor => editor::handle_key(app, key),
        ScreenId::Git => git::handle_key(app, key),
        ScreenId::Terminal => terminal::handle_key(app, key),
        ScreenId::Logs => logs::handle_key(app, key),
        ScreenId::Context => context::handle_key(app, key),
        ScreenId::Memory => memory::handle_key(app, key),
        ScreenId::Skills => skills::handle_key(app, key),
        _ => {}
    }
}

/// Dashboard keys: only `r` (manual refresh) — the snapshot is
/// otherwise refreshed on the bounded TTL.
fn dashboard_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    if key.code == KeyCode::Char('r') {
        app.invalidate_dashboard();
        app.set_message("refreshed");
    }
}

/// Renders the active screen into `area`.
pub fn draw<B: WorkspaceBackend>(frame: &mut ratatui::Frame, app: &mut App<B>, area: Rect) {
    let title = SCREENS
        .iter()
        .find(|s| s.id == app.screen())
        .map(|s| s.title)
        .unwrap_or("AWH");
    let block = Block::bordered().title(format!(" {title} ")).title_style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    match app.screen() {
        ScreenId::Dashboard => draw_dashboard(frame, app, area, block),
        ScreenId::Projects => projects::draw(frame, app, area, block),
        ScreenId::Files => files::draw(frame, app, area, block),
        ScreenId::Editor => editor::draw(frame, app, area, block),
        ScreenId::Git => git::draw(frame, app, area, block),
        ScreenId::Terminal => terminal::draw(frame, app, area, block),
        ScreenId::Logs => logs::draw(frame, app, area, block),
        ScreenId::Context => context::draw(frame, app, area, block),
        ScreenId::Memory => memory::draw(frame, app, area, block),
        ScreenId::Skills => skills::draw(frame, app, area, block),
        ScreenId::Help => draw_help(frame, area, block),
        _ => draw_placeholder(frame, app, area, block),
    }
}

fn draw_dashboard<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: Block<'_>,
) {
    let snapshot = app.dashboard_cached();
    let mut lines = vec![
        row("Workspace", snapshot.root.display().to_string()),
        row(
            "Project",
            snapshot
                .current_project
                .clone()
                .unwrap_or_else(|| "none selected".to_string()),
        ),
        row("Projects", snapshot.project_count.to_string()),
        row("Connection", "local".to_string()),
        row(
            "Git",
            if snapshot.is_git_repo {
                format!(
                    "{} ({} dirty)",
                    snapshot.branch.as_deref().unwrap_or("detached"),
                    snapshot.dirty_entries
                )
            } else {
                "not a repository".to_string()
            },
        ),
        row("Sessions", snapshot.running_sessions.to_string()),
        row("MCP plane", snapshot.mcp_status.clone()),
        row("Control API", snapshot.api_status.clone()),
    ];
    if !snapshot.warnings.is_empty() {
        lines.push(ratatui::text::Line::from(""));
        for w in &snapshot.warnings {
            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                format!("warning: {w}"),
                Style::default().fg(Color::Yellow),
            )));
        }
    }
    lines.push(ratatui::text::Line::from(""));
    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        "Recent activity",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    if snapshot.recent_activity.is_empty() {
        lines.push(ratatui::text::Line::from(
            "no security-relevant events recorded this session",
        ));
    } else {
        for entry in &snapshot.recent_activity {
            lines.push(ratatui::text::Line::from(format!("  {entry}")));
        }
    }
    hint_line(
        frame,
        inner(area),
        "[r] refresh  [Tab] next screen  [F1] help",
    );
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Shrinks a border by one cell on each side, for hint placement that
/// does not collide with the bordered content.
fn inner(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn row<'a>(label: &str, value: String) -> ratatui::text::Line<'a> {
    ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(format!("{label:<12}"), Style::default().fg(Color::Gray)),
        ratatui::text::Span::raw(value),
    ])
}

fn draw_help(frame: &mut ratatui::Frame, area: Rect, block: Block<'_>) {
    let mut all = vec![
        ratatui::text::Line::from("Global keys"),
        key_line("Tab / BackTab", "next / previous screen"),
        key_line("Esc", "back (quit from Dashboard)"),
        key_line("F1", "help"),
        key_line("C-q / F12", "quit"),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from("Screens"),
    ];
    for screen in SCREENS {
        all.push(ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(
                format!("{:<10}", screen.title),
                Style::default().fg(Color::Cyan),
            ),
            ratatui::text::Span::raw(screen.blurb),
        ]));
    }
    frame.render_widget(Paragraph::new(all).block(block), area);
}

fn key_line(key: &str, description: &str) -> ratatui::text::Line<'static> {
    ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(format!("  {key:<14}"), Style::default().fg(Color::Green)),
        ratatui::text::Span::raw(description.to_string()),
    ])
}

fn draw_placeholder<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: Block<'_>,
) {
    let screen = SCREENS.iter().find(|s| s.id == app.screen()).unwrap();
    frame.render_widget(
        Paragraph::new(vec![
            ratatui::text::Line::from(""),
            ratatui::text::Line::from(format!("{} foundation ready.", screen.title)),
            ratatui::text::Line::from(screen.blurb),
            ratatui::text::Line::from(""),
            ratatui::text::Line::from("This screen gains its full interface in a later phase."),
        ])
        .block(block),
        area,
    );
}

/// Renders a dim key-hint line at the bottom of a screen area.
pub(crate) fn hint_line(frame: &mut ratatui::Frame, area: Rect, hint: &str) {
    let [_, hint_area] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    frame.render_widget(
        Paragraph::new(ratatui::text::Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )),
        hint_area,
    );
}
