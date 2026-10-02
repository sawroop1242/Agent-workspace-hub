//! Agents screen: registered agent profiles, their runtime sessions,
//! and lifecycle actions. Everything flows through the backend
//! (`AgentRuntimeService` behind it); destructive stop/start go via
//! the operation tracker, and lifecycle rules live in the service.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};
use ratatui::Frame;

use super::{agent_role, hint_line};
use crate::tui::app::{ActionKind, App};
use crate::tui::backend::WorkspaceBackend;
use crate::tui::components::{render_state_placeholder, ViewState};
use crate::tui::theme::Role;

#[derive(Default)]
pub struct AgentsUi {
    /// 0 = profile list, 1 = sessions pane.
    pub pane: usize,
    pub session_selected: usize,
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    app.ui.capture_input = false;
    let pane = app.ui.agents_ui.pane;
    // Clone the cached rows first: the handlers below mutate `app`
    // (selections + tracked actions) while these are read.
    let agents = app.cached_agents().cloned().unwrap_or_default();
    let sessions = app.cached_sessions().cloned().unwrap_or_default();

    match key.code {
        KeyCode::Tab => app.ui.agents_ui.pane = 1 - app.ui.agents_ui.pane,
        KeyCode::Char('1') => app.ui.agents_ui.pane = 0,
        KeyCode::Char('2') => app.ui.agents_ui.pane = 1,
        KeyCode::Up => {
            if pane == 0 {
                if let Some(sel) = app.ui.agents.selected() {
                    if sel > 0 {
                        app.ui.agents.select(Some(sel - 1));
                    }
                }
            } else {
                app.ui.agents_ui.session_selected =
                    app.ui.agents_ui.session_selected.saturating_sub(1);
            }
        }
        KeyCode::Down => {
            if pane == 0 {
                if !agents.is_empty() {
                    let sel = app.ui.agents.selected().unwrap_or(0);
                    app.ui.agents.select(Some((sel + 1).min(agents.len() - 1)));
                }
            } else if !sessions.is_empty() {
                app.ui.agents_ui.session_selected =
                    (app.ui.agents_ui.session_selected + 1).min(sessions.len() - 1);
            }
        }
        KeyCode::Char(' ') => {
            // Space toggles the selected agent's runtime lifecycle
            // through the same tracked path as the palette.
            if let Some(agent) = agents.get(app.ui.agents.selected().unwrap_or(0)) {
                let id = agent.id.clone();
                let running = agent.status == "Active";
                let kind = if running {
                    ActionKind::AgentStop(id)
                } else {
                    ActionKind::AgentStart(id)
                };
                app.request_action(kind);
            }
        }
        _ => {}
    }
}

pub fn draw<B: WorkspaceBackend>(frame: &mut Frame, app: &mut App<B>, area: Rect) {
    let [content, hint] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    let [list_pane, session_pane] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(content);

    match app.cached_agents() {
        Some(agents) if agents.is_empty() => {
            render_state_placeholder(
                frame,
                panel_inner(list_pane),
                &ViewState::Empty {
                    hint: "register agents with `awh agent create`",
                },
            );
        }
        Some(agents) => {
            let items: Vec<ListItem> = agents
                .iter()
                .map(|a| {
                    let role = agent_role(&a.status, a.enabled);
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{} ", role.symbol()), role.style()),
                        Span::styled(format!("{:<16} ", a.id), Role::Normal.style()),
                        Span::styled(format!("{:<10} ", a.status), role.style()),
                        Span::styled(a.name.clone(), Role::Muted.style()),
                    ]))
                })
                .collect();
            let list = List::new(items)
                .block(Block::bordered().title(" agents "))
                .highlight_style(Role::Selected.style());
            frame.render_stateful_widget(list, panel_inner(list_pane), &mut app.ui.agents);
        }
        None => {
            render_state_placeholder(
                frame,
                panel_inner(list_pane),
                &ViewState::Unavailable {
                    reason: "agent registry not available on this backend",
                },
            );
        }
    }

    let session_rows = app.cached_sessions().cloned();
    let session_sel = app.ui.agents_ui.session_selected;
    match session_rows {
        Some(sessions) if sessions.is_empty() => {
            render_state_placeholder(
                frame,
                panel_inner(session_pane),
                &ViewState::Empty {
                    hint: "open sessions with `awh agent session open`",
                },
            );
        }
        Some(sessions) => {
            let visible = panel_inner(session_pane).height as usize;
            let start = session_sel.saturating_sub(visible.saturating_sub(1));
            let rows: Vec<Line> = sessions
                .iter()
                .enumerate()
                .skip(start)
                .take(visible)
                .map(|(i, s)| {
                    let role = match s.status.as_str() {
                        "Active" => Role::Running,
                        "Paused" => Role::Pending,
                        "Failed" => Role::Error,
                        _ => Role::Muted,
                    };
                    let selected = i == session_sel;
                    let marker = if selected { "▶" } else { " " };
                    Line::from(vec![
                        Span::styled(format!("{marker}{} ", role.symbol()), role.style()),
                        Span::styled(s.session_id.clone(), Role::Normal.style()),
                        Span::styled(" ", Role::Muted.style()),
                        Span::styled(s.status.clone(), role.style()),
                    ])
                })
                .collect();
            frame.render_widget(
                Paragraph::new(rows).block(Block::bordered().title(" sessions ")),
                session_pane,
            );
        }
        None => {
            render_state_placeholder(
                frame,
                panel_inner(session_pane),
                &ViewState::Unavailable {
                    reason: "sessions not available on this backend",
                },
            );
        }
    }

    hint_line(
        frame,
        hint,
        "[Tab/1/2] panes  [↑/↓] select  [Space] start/stop agent  [r] refresh",
    );
}

fn panel_inner(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::backend::LocalBackend;
    use crate::tui::screens::ScreenId;

    fn app() -> App<LocalBackend> {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        App::new(LocalBackend::new(root))
    }

    fn press(app: &mut App<LocalBackend>, code: KeyCode) {
        handle_key(app, KeyEvent::from(code));
    }

    #[test]
    fn space_starts_a_registered_agent_through_the_service() {
        let mut app = app();
        app.backend
            .register_profile("agent-coder", "Coder", "coder")
            .unwrap();
        app.goto(ScreenId::Agents);
        app.ui.agents.select(Some(0));
        press(&mut app, KeyCode::Char(' '));
        let agents = app.backend.list_agents().unwrap();
        assert_eq!(agents[0].status, "Active");
        // Stop flips back through the same tracked path.
        press(&mut app, KeyCode::Char(' '));
        let agents = app.backend.list_agents().unwrap();
        assert_eq!(agents[0].status, "Stopped");
    }

    #[test]
    fn unknown_status_is_distinct_from_running_in_the_role_map() {
        assert_ne!(agent_role("Active", true), agent_role("Stopped", true));
        assert_ne!(agent_role("Active", true), agent_role("Active", false));
    }
}
