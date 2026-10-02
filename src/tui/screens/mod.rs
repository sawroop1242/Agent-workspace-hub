//! Screen registry and dispatch.
//!
//! Each screen lives in its own module with a `handle_key` + `draw`
//! pair operating on shared [`App`] state plus its own UI struct held
//! in [`ScreenState`]. The primary sections follow the Prompt 30
//! navigation order; Projects/Editor/Remote/Help stay reachable as
//! secondary screens via the palette, contextual jumps, and the rail.

pub mod agents;
pub mod audit;
pub mod changes;
pub mod context;
pub mod editor;
pub mod files;
pub mod git;
pub mod mcp;
pub mod memory;
pub mod projects;
pub mod remote;
pub mod settings;
pub mod skills;
pub mod tasks;
pub mod terminal;

use crossterm::event::KeyEvent;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use super::app::App;
use super::backend::WorkspaceBackend;
use super::theme::{LayoutProfile, Role};

/// All navigable screens: the 13 primary sections first (Prompt 30
/// navigation order), then the secondary screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenId {
    Dashboard,
    Agents,
    Tasks,
    Files,
    Changes,
    Git,
    Context,
    Memory,
    Skills,
    Mcp,
    Terminal,
    Audit,
    Settings,
    Projects,
    Editor,
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
        blurb: "Operational overview: workspace, git, health, activity",
    },
    ScreenMeta {
        id: ScreenId::Agents,
        title: "Agents",
        blurb: "Agent profiles, runtime sessions, lifecycle actions",
    },
    ScreenMeta {
        id: ScreenId::Tasks,
        title: "Tasks",
        blurb: "Task activity with status distinction",
    },
    ScreenMeta {
        id: ScreenId::Files,
        title: "Files",
        blurb: "Browse, read, and edit files within workspace boundaries",
    },
    ScreenMeta {
        id: ScreenId::Changes,
        title: "Changes",
        blurb: "Staged/unstaged files with diff, stage, unstage, commit",
    },
    ScreenMeta {
        id: ScreenId::Git,
        title: "Git",
        blurb: "Status, diff, log, stage, commit over structured Git",
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
        id: ScreenId::Mcp,
        title: "MCP",
        blurb: "MCP servers, tools, connectors",
    },
    ScreenMeta {
        id: ScreenId::Terminal,
        title: "Terminal",
        blurb: "Bounded argv command execution with output capture",
    },
    ScreenMeta {
        id: ScreenId::Audit,
        title: "Audit",
        blurb: "Chronological canonical audit events with filters",
    },
    ScreenMeta {
        id: ScreenId::Settings,
        title: "Settings",
        blurb: "Workspace configuration and preferences",
    },
    ScreenMeta {
        id: ScreenId::Projects,
        title: "Projects",
        blurb: "Create, open, delete projects (destructive actions confirm)",
    },
    ScreenMeta {
        id: ScreenId::Editor,
        title: "Editor",
        blurb: "Text editor with dirty state and safe large-file refusal",
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

/// Every screen id in ring order (drives keymap/palette completeness
/// tests).
pub const ALL_SCREENS: &[ScreenId] = &[
    ScreenId::Dashboard,
    ScreenId::Agents,
    ScreenId::Tasks,
    ScreenId::Files,
    ScreenId::Changes,
    ScreenId::Git,
    ScreenId::Context,
    ScreenId::Memory,
    ScreenId::Skills,
    ScreenId::Mcp,
    ScreenId::Terminal,
    ScreenId::Audit,
    ScreenId::Settings,
    ScreenId::Projects,
    ScreenId::Editor,
    ScreenId::Remote,
    ScreenId::Help,
];

/// Per-screen mutable UI state (selections, inputs, buffers).
#[derive(Default)]
pub struct ScreenState {
    pub projects: ratatui::widgets::ListState,
    pub files: ratatui::widgets::ListState,
    pub git: ratatui::widgets::ListState,
    pub agents: ratatui::widgets::ListState,
    pub tasks: ratatui::widgets::ListState,
    pub changes: ratatui::widgets::ListState,
    pub audit: ratatui::widgets::ListState,
    /// True while the active screen owns a text input: global keys
    /// (digits, palette, refresh) must not steal keystrokes.
    pub capture_input: bool,
    pub projects_ui: projects::ProjectsUi,
    pub files_ui: files::FilesUi,
    pub editor_ui: editor::EditorUi,
    pub git_ui: git::GitUi,
    pub terminal_ui: terminal::TerminalUi,
    pub context_ui: context::ContextUi,
    pub memory_ui: memory::MemoryUi,
    pub skills_ui: skills::SkillsUi,
    pub mcp_ui: mcp::McpUi,
    pub remote_ui: remote::RemoteUi,
    pub agents_ui: agents::AgentsUi,
    pub tasks_ui: tasks::TasksUi,
    pub changes_ui: changes::ChangesUi,
    pub audit_ui: audit::AuditUi,
    /// Content produced by a DiscardChanges action, adopted by the
    /// Editor screen on its next draw.
    pub reload_content: Option<(String, String)>,
}

/// Dispatches a key press to the active screen. Screens set
/// `capture_input` themselves so the global layer never steals keys
/// while a text input is active.
pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    match app.screen() {
        ScreenId::Dashboard => {}
        ScreenId::Agents => agents::handle_key(app, key),
        ScreenId::Tasks => tasks::handle_key(app, key),
        ScreenId::Files => files::handle_key(app, key),
        ScreenId::Changes => changes::handle_key(app, key),
        ScreenId::Editor => editor::handle_key(app, key),
        ScreenId::Git => git::handle_key(app, key),
        ScreenId::Terminal => terminal::handle_key(app, key),
        ScreenId::Audit => audit::handle_key(app, key),
        ScreenId::Context => context::handle_key(app, key),
        ScreenId::Memory => memory::handle_key(app, key),
        ScreenId::Skills => skills::handle_key(app, key),
        ScreenId::Mcp => mcp::handle_key(app, key),
        ScreenId::Settings => settings::handle_key(app, key),
        ScreenId::Remote => remote::handle_key(app, key),
        ScreenId::Projects => projects::handle_key(app, key),
        ScreenId::Help => {}
    }
}

/// Renders the active screen into `area`. Screens own their panels;
/// the shell already rendered header/nav/footer.
pub fn draw<B: WorkspaceBackend>(frame: &mut Frame, app: &mut App<B>, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    // Screens that retitle their own block get a bare one; the rest
    // get the shell-titled block with the screen name.
    let bare = || Block::bordered();
    let titled = |title: &str| {
        Block::bordered()
            .title(format!(" {title} "))
            .title_style(Role::Focused.style())
    };
    match app.screen() {
        ScreenId::Dashboard => draw_dashboard(frame, app, area),
        ScreenId::Agents => agents::draw(frame, app, area),
        ScreenId::Tasks => tasks::draw(frame, app, area),
        ScreenId::Files => files::draw(frame, app, area, bare()),
        ScreenId::Changes => changes::draw(frame, app, area),
        ScreenId::Editor => editor::draw(frame, app, area, bare()),
        ScreenId::Git => git::draw(frame, app, area, bare()),
        ScreenId::Terminal => terminal::draw(frame, app, area, titled("Terminal")),
        ScreenId::Audit => audit::draw(frame, app, area),
        ScreenId::Context => context::draw(frame, app, area, bare()),
        ScreenId::Memory => memory::draw(frame, app, area, bare()),
        ScreenId::Skills => skills::draw(frame, app, area, titled("Skills")),
        ScreenId::Mcp => mcp::draw(frame, app, area, titled("MCP")),
        ScreenId::Settings => settings::draw(frame, app, area, titled("Settings")),
        ScreenId::Remote => remote::draw(frame, app, area, titled("Remote")),
        ScreenId::Projects => projects::draw(frame, app, area, titled("Projects")),
        ScreenId::Help => draw_help(frame, app, area),
    }
}

fn draw_dashboard<B: WorkspaceBackend>(frame: &mut Frame, app: &mut App<B>, area: Rect) {
    let profile = LayoutProfile::for_area(area);
    let snapshot = app.dashboard_cached();

    if profile == LayoutProfile::Compact {
        // Compact: identity + git only, one column.
        let mut lines = vec![
            Line::from(vec![
                Span::styled("workspace ", Role::Muted.style()),
                Span::raw(snapshot.root.display().to_string()),
            ]),
            Line::from(vec![
                Span::styled("projects  ", Role::Muted.style()),
                Span::raw(snapshot.project_count.to_string()),
            ]),
            Line::from(vec![
                Span::styled("project   ", Role::Muted.style()),
                Span::raw(
                    snapshot
                        .current_project
                        .clone()
                        .unwrap_or_else(|| "none selected".into()),
                ),
            ]),
            Line::from(vec![
                Span::styled("git       ", Role::Muted.style()),
                Span::raw(if snapshot.is_git_repo {
                    format!(
                        "{} ({})",
                        snapshot.branch.as_deref().unwrap_or("detached"),
                        snapshot.dirty_entries
                    )
                } else {
                    "no repository".into()
                }),
            ]),
        ];
        if !snapshot.warnings.is_empty() {
            for w in &snapshot.warnings {
                lines.push(Line::from(Span::styled(
                    format!("{} {w}", Role::Warning.symbol()),
                    Role::Warning.style(),
                )));
            }
        }
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Dashboard ")),
            area,
        );
        return;
    }

    let large = profile == LayoutProfile::Large;
    let [top, bottom] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(if large { 8 } else { 0 }),
    ])
    .areas(area);

    let columns = Layout::horizontal(if large {
        vec![
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ]
    } else {
        vec![Constraint::Percentage(50), Constraint::Percentage(50)]
    })
    .split(top);

    // Panel 1: workspace identity.
    let identity = vec![
        Line::from(vec![
            Span::styled("workspace ", Role::Muted.style()),
            Span::raw(snapshot.root.display().to_string()),
        ]),
        Line::from(vec![
            Span::styled("projects  ", Role::Muted.style()),
            Span::raw(snapshot.project_count.to_string()),
        ]),
        Line::from(vec![
            Span::styled("project   ", Role::Muted.style()),
            Span::raw(
                snapshot
                    .current_project
                    .clone()
                    .unwrap_or_else(|| "none selected".into()),
            ),
        ]),
        Line::from(vec![
            Span::styled("sessions  ", Role::Muted.style()),
            Span::raw(snapshot.running_sessions.to_string()),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(identity).block(Block::bordered().title(" workspace ")),
        columns[0],
    );

    // Panel 2: health surfaces.
    let git_line = if snapshot.is_git_repo {
        format!(
            "{} · {}",
            snapshot.branch.as_deref().unwrap_or("detached"),
            snapshot.dirty_entries
        )
    } else {
        "no repository".into()
    };
    let health = vec![
        Line::from(vec![
            Span::styled("git       ", Role::Muted.style()),
            Span::raw(git_line),
        ]),
        Line::from(vec![
            Span::styled("mcp       ", Role::Muted.style()),
            Span::raw(snapshot.mcp_status.clone()),
        ]),
        Line::from(vec![
            Span::styled("api       ", Role::Muted.style()),
            Span::raw(snapshot.api_status.clone()),
        ]),
        Line::from(vec![
            Span::styled("terminal  ", Role::Muted.style()),
            Span::raw("bounded argv (30s cap)"),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(health).block(Block::bordered().title(" health ")),
        columns[1],
    );

    if large {
        // Panel 3: recent canonical audit activity.
        let mut activity: Vec<Line> = snapshot
            .recent_activity
            .iter()
            .take(5)
            .map(|a| Line::from(Span::raw(format!("· {a}"))))
            .collect();
        if activity.is_empty() {
            activity.push(Line::from(Span::styled(
                "no security-relevant events recorded this session",
                Role::Muted.style(),
            )));
        }
        frame.render_widget(
            Paragraph::new(activity).block(Block::bordered().title(" activity ")),
            columns[2],
        );

        // Bottom row: task / changes / commits summary panels.
        let [tasks_p, changes_p, commits_p] = Layout::horizontal([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .areas(bottom);

        let tasks = app.cached_tasks().cloned().unwrap_or_default();
        let task_lines = task_summary_lines(&tasks);
        frame.render_widget(
            Paragraph::new(task_lines).block(Block::bordered().title(" tasks ")),
            tasks_p,
        );

        let changes = app.cached_changes().cloned().unwrap_or_default();
        let change_lines = change_summary_lines(&changes);
        frame.render_widget(
            Paragraph::new(change_lines).block(Block::bordered().title(" changes ")),
            changes_p,
        );

        let commits = app.cached_commits().cloned().unwrap_or_default();
        let commit_lines = commit_summary_lines(&commits);
        frame.render_widget(
            Paragraph::new(commit_lines).block(Block::bordered().title(" commits ")),
            commits_p,
        );
    }
}

fn task_summary_lines(tasks: &[super::backend::TaskView]) -> Vec<Line<'static>> {
    if tasks.is_empty() {
        return vec![Line::from(Span::styled(
            "no tasks recorded",
            Role::Muted.style(),
        ))];
    }
    let mut lines: Vec<Line> = tasks
        .iter()
        .take(3)
        .map(|t| {
            let role = task_role(&t.status);
            Line::from(vec![
                Span::styled(format!("{} ", role.symbol()), role.style()),
                Span::raw(t.title.clone()),
            ])
        })
        .collect();
    if tasks.len() > 3 {
        lines.push(Line::from(Span::styled(
            format!("{} tasks total", tasks.len()),
            Role::Muted.style(),
        )));
    }
    lines
}

fn change_summary_lines(changes: &[super::backend::ChangedFile]) -> Vec<Line<'static>> {
    if changes.is_empty() {
        return vec![Line::from(Span::styled(
            "working tree clean",
            Role::Success.style(),
        ))];
    }
    changes
        .iter()
        .take(3)
        .map(|c| {
            Line::from(vec![
                Span::styled(
                    format!("{} ", c.status_code.trim_end()),
                    Role::Warning.style(),
                ),
                Span::raw(c.path.clone()),
            ])
        })
        .collect()
}

fn commit_summary_lines(commits: &[super::backend::CommitView]) -> Vec<Line<'static>> {
    if commits.is_empty() {
        return vec![Line::from(Span::styled("no commits", Role::Muted.style()))];
    }
    commits
        .iter()
        .take(3)
        .map(|c| {
            Line::from(vec![
                Span::styled(c.hash.clone(), Role::Muted.style()),
                Span::raw(" "),
                Span::raw(c.subject.clone()),
            ])
        })
        .collect()
}

/// Task status → semantic role (shared by Dashboard and Tasks views).
pub(crate) fn task_role(status: &str) -> Role {
    match status {
        "InProgress" => Role::Running,
        "Blocked" => Role::Blocked,
        "Done" => Role::Completed,
        "Cancelled" => Role::Cancelled,
        _ => Role::Pending,
    }
}

/// Agent status → semantic role (shared by Dashboard and Agents views).
pub(crate) fn agent_role(status: &str, enabled: bool) -> Role {
    if !enabled {
        return Role::Cancelled;
    }
    match status {
        "Active" => Role::Running,
        "Paused" => Role::Pending,
        "Stopped" => Role::Muted,
        _ => Role::Pending,
    }
}

/// Help renders the ACTUALLY ACTIVE keymap: the global bindings plus
/// every screen's registered bindings, all sourced from `keymap`.
fn draw_help<B: WorkspaceBackend>(frame: &mut Frame, _app: &mut App<B>, area: Rect) {
    let mut all = vec![Line::from(Span::styled(
        "Global keys",
        Role::Focused.style(),
    ))];
    for binding in super::keymap::GLOBAL_BINDINGS {
        all.push(key_line(binding.key, binding.action));
    }
    all.push(Line::from(Span::styled("Palette", Role::Focused.style())));
    for binding in super::keymap::OVERLAY_BINDINGS {
        all.push(key_line(binding.key, binding.action));
    }
    all.push(Line::from(Span::styled(
        "Screens (Tab cycles · digits 1-9 jump to primary sections)",
        Role::Focused.style(),
    )));
    for screen in SCREENS {
        let bindings = super::keymap::screen_bindings(screen.id);
        let mut spans = vec![
            Span::styled(format!("{:<10} ", screen.title), Role::Normal.style()),
            Span::styled(screen.blurb, Role::Muted.style()),
        ];
        if !bindings.is_empty() {
            spans.push(Span::styled(
                format!(
                    "  [{}]",
                    bindings
                        .iter()
                        .map(|b| b.key.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                Role::Muted.style(),
            ));
        }
        all.push(Line::from(spans));
    }
    frame.render_widget(
        Paragraph::new(all).block(Block::bordered().title(" Help ")),
        area,
    );
}

fn key_line(key: &str, description: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key:<14}"), Role::Success.style()),
        Span::raw(description.to_string()),
    ])
}

/// Renders a dim key-hint line at the bottom of a screen area.
pub(crate) fn hint_line(frame: &mut Frame, area: Rect, hint: &str) {
    let [_, hint_area] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    frame.render_widget(
        Paragraph::new(Span::styled(hint, Role::Muted.style())),
        hint_area,
    );
}
