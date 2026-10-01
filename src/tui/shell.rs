//! Global application shell: header, navigation rail, content, footer.
//!
//! The shell keeps workspace/project/agent/branch/connection context
//! permanently visible (Prompt 30: "Workspace first"), exposes the
//! primary navigation sections, and routes contextual hints from the
//! central keymap.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use super::app::App;
use super::backend::{BackendMode, WorkspaceBackend};
use super::keymap;
use super::screens::ScreenId;
use super::theme::{LayoutProfile, Role, GLYPHS};

/// Primary navigation sections (Prompt 30 order). Secondary screens
/// (Projects/Editor/Remote/Help) stay reachable through the palette,
/// contextual jumps, and the rail's large-terminal extension.
pub const PRIMARY_SECTIONS: &[(ScreenId, &str)] = &[
    (ScreenId::Dashboard, "Dashboard"),
    (ScreenId::Agents, "Agents"),
    (ScreenId::Tasks, "Tasks"),
    (ScreenId::Files, "Files"),
    (ScreenId::Changes, "Changes"),
    (ScreenId::Git, "Git"),
    (ScreenId::Context, "Context"),
    (ScreenId::Memory, "Memory"),
    (ScreenId::Skills, "Skills"),
    (ScreenId::Mcp, "MCP"),
    (ScreenId::Terminal, "Terminal"),
    (ScreenId::Audit, "Audit"),
    (ScreenId::Settings, "Settings"),
];

pub const SECONDARY_SECTIONS: &[(ScreenId, &str)] = &[
    (ScreenId::Projects, "Projects"),
    (ScreenId::Editor, "Editor"),
    (ScreenId::Remote, "Remote"),
    (ScreenId::Help, "Help"),
];

/// Everything the shell needs for its chrome, owned strings only so no
/// borrow of the app outlives the render call.
#[derive(Debug, Clone, Default)]
pub struct ShellContext {
    pub workspace: String,
    pub project: Option<String>,
    pub branch: Option<String>,
    pub dirty: usize,
    pub connection: String,
    pub connection_role: Role,
}

impl<B: WorkspaceBackend> App<B> {
    /// Collects shell context from the cached dashboard snapshot — the
    /// TTL cache guarantees this performs no fresh IO per frame.
    pub fn shell_context(&mut self) -> ShellContext {
        let snap = self.dashboard_cached();
        let workspace = snap
            .root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| snap.root.display().to_string());
        let (connection, connection_role) = match self.backend.mode() {
            BackendMode::Local => ("local".to_string(), Role::Success),
            BackendMode::Remote => (
                self.backend
                    .remote_base()
                    .unwrap_or_else(|| "remote".into()),
                Role::Running,
            ),
        };
        ShellContext {
            workspace,
            project: snap.current_project.clone(),
            branch: snap.branch.clone(),
            dirty: snap.dirty_entries,
            connection,
            connection_role,
        }
    }
}

/// Renders the shell chrome and returns the content rect for the active
/// screen. `None` means the terminal cannot fit any content.
pub fn render_shell<B: WorkspaceBackend>(
    frame: &mut Frame,
    app: &mut App<B>,
    area: Rect,
) -> Option<Rect> {
    let profile = LayoutProfile::for_area(area);
    let ctx = app.shell_context();

    let [header, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    render_header(frame, header, &ctx, profile);

    match profile {
        LayoutProfile::Compact => {
            let [content, footer] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(rest);
            render_footer(frame, footer, app, profile);
            (content.width > 0 && content.height > 0).then_some(content)
        }
        LayoutProfile::Normal | LayoutProfile::Large => {
            let [nav, body] =
                Layout::horizontal([Constraint::Length(profile.nav_width()), Constraint::Min(1)])
                    .areas(rest);
            let [content, footer] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(body);
            render_nav_rail(frame, nav, app.screen(), profile);
            render_footer(frame, footer, app, profile);
            (content.width > 0 && content.height > 0).then_some(content)
        }
    }
}

fn render_header(frame: &mut Frame, area: Rect, ctx: &ShellContext, profile: LayoutProfile) {
    let mut spans = vec![
        Span::styled("AWH", Role::Active.style()),
        Span::styled(" ", Role::Muted.style()),
        Span::styled(ctx.workspace.clone(), Role::Normal.style()),
    ];
    if let Some(project) = &ctx.project {
        spans.push(Span::styled("/", Role::Muted.style()));
        spans.push(Span::styled(project.clone(), Role::Focused.style()));
    }
    if profile != LayoutProfile::Compact {
        spans.push(Span::styled("  ", Role::Muted.style()));
        match (&ctx.branch, ctx.dirty) {
            (Some(branch), dirty) => {
                let glyph = GLYPHS.branch;
                spans.push(Span::styled(
                    format!("{glyph} {branch}"),
                    Role::Normal.style(),
                ));
                if dirty > 0 {
                    spans.push(Span::styled(format!(" ●{dirty}"), Role::Warning.style()));
                }
            }
            (None, _) => spans.push(Span::styled("no git", Role::Muted.style())),
        }
    }
    spans.push(Span::styled("  ", Role::Muted.style()));
    spans.push(Span::styled(
        ctx.connection.clone(),
        ctx.connection_role.style(),
    ));
    if profile == LayoutProfile::Compact {
        // The rail is collapsed; keep palette access visible.
        spans.push(Span::styled("  ^K", Role::Muted.style()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_nav_rail(frame: &mut Frame, area: Rect, active: ScreenId, profile: LayoutProfile) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut lines: Vec<Line> = Vec::new();
    for (index, (id, title)) in PRIMARY_SECTIONS.iter().enumerate() {
        let selected = *id == active;
        let marker = if selected {
            Role::Selected.symbol()
        } else {
            " "
        };
        let style = if selected {
            Role::Selected.style()
        } else {
            Role::Normal.style()
        };
        let key = if index < 9 {
            (index + 1).to_string()
        } else {
            "·".to_string()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {key} "), style),
            Span::styled(*title, style),
        ]));
    }
    if profile == LayoutProfile::Large && area.height as usize > PRIMARY_SECTIONS.len() + 3 {
        lines.push(Line::from(Span::styled("─ secondary", Role::Muted.style())));
        for (id, title) in SECONDARY_SECTIONS {
            let selected = *id == active;
            let marker = if selected {
                Role::Selected.symbol()
            } else {
                " "
            };
            let style = if selected {
                Role::Selected.style()
            } else {
                Role::Muted.style()
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker}   "), style),
                Span::styled(*title, style),
            ]));
        }
    }
    let paragraph = Paragraph::new(lines)
        .block(Block::bordered().title(Span::styled(" nav ", Role::Muted.style())));
    frame.render_widget(paragraph, area);
}

/// Footer: notification/status line (contextual message, error, or
/// operation progress) plus the real keymap hint for the active screen.
fn render_footer<B: WorkspaceBackend>(
    frame: &mut Frame,
    area: Rect,
    app: &mut App<B>,
    profile: LayoutProfile,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let [status, hint] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(if area.height >= 2 && profile != LayoutProfile::Compact {
            1
        } else {
            0
        }),
    ])
    .areas(area);

    let status_line = if let Some(op) = app.pending_operation() {
        Line::from(op.status_line())
    } else if let Some(err) = app.error.clone() {
        Line::from(vec![
            Span::styled(Role::Error.symbol(), Role::Error.style()),
            Span::raw(" "),
            Span::raw(err),
        ])
    } else if let Some(msg) = app.message.clone() {
        Line::from(vec![
            Span::styled(Role::Success.symbol(), Role::Success.style()),
            Span::raw(" "),
            Span::raw(msg),
        ])
    } else {
        Line::from(Span::styled("", Role::Muted.style()))
    };
    frame.render_widget(Paragraph::new(status_line), status);

    if hint.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                keymap::footer_hint(app.screen()),
                Role::Muted.style(),
            )),
            hint,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_sections_cover_the_required_navigation() {
        // Prompt 30 requires exactly these 13 primary sections, in order.
        let titles: Vec<&str> = PRIMARY_SECTIONS.iter().map(|(_, t)| *t).collect();
        assert_eq!(
            titles,
            vec![
                "Dashboard",
                "Agents",
                "Tasks",
                "Files",
                "Changes",
                "Git",
                "Context",
                "Memory",
                "Skills",
                "MCP",
                "Terminal",
                "Audit",
                "Settings",
            ]
        );
    }

    #[test]
    fn secondary_screens_stay_reachable() {
        // Projects/Editor/Remote/Help are not primary sections but
        // must remain registered for palette and contextual jumps.
        let ids: Vec<ScreenId> = SECONDARY_SECTIONS.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&ScreenId::Projects));
        assert!(ids.contains(&ScreenId::Editor));
        assert!(ids.contains(&ScreenId::Help));
        assert!(ids.contains(&ScreenId::Remote));
    }
}
