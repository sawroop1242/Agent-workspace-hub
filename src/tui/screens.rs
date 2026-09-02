//! Screen registry and shared drawing helpers.
//!
//! Phase 2 provides the foundation: navigation across every screen with
//! visible key hints and backend-driven content where the services
//! already exist (Dashboard, Projects, Files). The remaining screens
//! render a foundation placeholder and gain functionality in Phases 3-8.

use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use super::app::App;
use super::backend::{DashboardSnapshot, WorkspaceBackend};

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

/// Per-screen mutable UI state (selections, listings).
#[derive(Default)]
pub struct ScreenState {
    pub projects: ListState,
    pub files: ListState,
    pub pending_project_name: String,
    pub git_output: String,
    pub terminal_output: String,
    pub terminal_program: String,
    pub terminal_args: String,
}

impl ScreenState {
    pub fn selected(&self, state: &ListState) -> usize {
        state.selected().unwrap_or(0)
    }
}

/// Dispatches a key press to the active screen.
pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: crossterm::event::KeyEvent) {
    let screen = app.screen();
    match (screen, key.code) {
        // Errors are dismissed from any screen.
        (_, KeyCode::Char('x')) if app.error.is_some() => {
            app.error = None;
        }
        (ScreenId::Projects, KeyCode::Char('n')) => {
            // Seed a placeholder name; the full form arrives in Phase 3.
            app.set_message("new project: press Enter to confirm name 'untitled'");
        }
        (ScreenId::Projects, KeyCode::Delete) => {
            app.request_action(super::app::PendingAction {
                id: "delete-project",
                description: "delete selected project".into(),
                destructive: true,
            });
        }
        (ScreenId::Terminal, KeyCode::Enter) => {
            app.set_message("terminal: full execution arrives in Phase 4");
        }
        _ => {}
    }
}

/// Renders the active screen into `area`.
pub fn draw<B: WorkspaceBackend>(frame: &mut ratatui::Frame, app: &mut App<B>, area: Rect) {
    let title = SCREENS
        .iter()
        .find(|s| s.id == app.screen())
        .map(|s| s.title)
        .unwrap_or("AWH");
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    match app.screen() {
        ScreenId::Dashboard => draw_dashboard(frame, app, area, block),
        ScreenId::Projects => draw_projects(frame, app, area, block),
        ScreenId::Files => draw_files(frame, app, area, block),
        ScreenId::Help => draw_help(frame, area, block),
        ScreenId::Editor
        | ScreenId::Git
        | ScreenId::Terminal
        | ScreenId::Mcp
        | ScreenId::Context
        | ScreenId::Memory
        | ScreenId::Skills
        | ScreenId::Logs
        | ScreenId::Settings
        | ScreenId::Remote => draw_placeholder(frame, app, area, block),
    }
}

fn draw_dashboard<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: Block<'_>,
) {
    let snapshot: DashboardSnapshot = app.backend.dashboard().unwrap_or_default();
    let mut lines = vec![
        row("Workspace", snapshot.root.display().to_string()),
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
        row("MCP server", "not running (use awh mcp serve)".to_string()),
    ];
    if !snapshot.warnings.is_empty() {
        lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
            format!("warnings: {}", snapshot.warnings.join("; ")),
            Style::default().fg(Color::Yellow),
        )));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn row<'a>(label: &str, value: String) -> ratatui::text::Line<'a> {
    ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(format!("{label:<12}"), Style::default().fg(Color::Gray)),
        ratatui::text::Span::raw(value),
    ])
}

fn draw_projects<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: Block<'_>,
) {
    let projects = app.backend.list_projects().unwrap_or_default();
    let items: Vec<ListItem> = projects
        .iter()
        .map(|name| ListItem::new(name.clone()))
        .collect();
    let empty = projects.is_empty();
    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_stateful_widget(list, area, &mut app.ui.projects);
    if empty {
        overlay_note(frame, area, "no projects — press n to create one");
    }
    let hint = Paragraph::new("[n] new  [Del] delete (confirms)  [Enter] open");
    let [_, hint_area] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    frame.render_widget(hint, hint_area);
}

fn draw_files<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: Block<'_>,
) {
    let entries = app.backend.list_dir("").unwrap_or_default();
    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let marker = if entry.is_dir { "dir " } else { "file" };
            ListItem::new(format!("{marker}  {}", entry.name))
        })
        .collect();
    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_stateful_widget(list, area, &mut app.ui.files);
    if entries.is_empty() {
        overlay_note(frame, area, "empty directory");
    }
}

fn draw_help(frame: &mut ratatui::Frame, area: Rect, block: Block<'_>) {
    let lines = vec![
        ratatui::text::Line::from("Global keys"),
        key_line("Tab / BackTab", "next / previous screen"),
        key_line("Esc", "back (quit from Dashboard)"),
        key_line("F1", "help"),
        key_line("C-q / F12", "quit"),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from("Screens"),
    ];
    let mut all = lines;
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

/// Renders a dim hint inside an empty screen without stealing focus.
fn overlay_note(frame: &mut ratatui::Frame, area: Rect, note: &str) {
    let [note_area] = Layout::vertical([Constraint::Length(1)]).areas(area);
    let centered = Rect {
        x: note_area.x + 2,
        width: note_area.width.saturating_sub(4),
        ..note_area
    };
    frame.render_widget(
        Paragraph::new(ratatui::text::Span::styled(
            note,
            Style::default().fg(Color::DarkGray),
        )),
        centered,
    );
}
