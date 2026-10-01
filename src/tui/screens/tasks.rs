//! Tasks screen: task activity with status distinction (Prompt 30:
//! pending / running / completed / failed / cancelled / blocked — v1
//! maps the canonical `TaskStatus` vocabulary). Read-only view over
//! the task store; creation/edits happen through the CLI/services.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem};
use ratatui::Frame;

use super::{hint_line, task_role};
use crate::tui::app::App;
use crate::tui::backend::WorkspaceBackend;
use crate::tui::components::{render_state_placeholder, ViewState};
use crate::tui::theme::Role;

#[derive(Default)]
pub struct TasksUi {
    /// Simple status filter: None = all.
    pub filter: Option<String>,
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    app.ui.capture_input = false;
    let filter = app.ui.tasks_ui.filter.clone();
    let count = app
        .cached_tasks()
        .map(|tasks| {
            tasks
                .iter()
                .filter(|t| filter.as_deref().is_none_or(|f| t.status == f))
                .count()
        })
        .unwrap_or(0);

    match key.code {
        KeyCode::Up => {
            if let Some(sel) = app.ui.tasks.selected() {
                if sel > 0 {
                    app.ui.tasks.select(Some(sel - 1));
                }
            }
        }
        KeyCode::Down => {
            if count > 0 {
                let sel = app.ui.tasks.selected().unwrap_or(0);
                app.ui.tasks.select(Some((sel + 1).min(count - 1)));
            }
        }
        KeyCode::Char('a') => app.ui.tasks_ui.filter = None,
        KeyCode::Char('1') => app.ui.tasks_ui.filter = Some("Todo".into()),
        KeyCode::Char('2') => app.ui.tasks_ui.filter = Some("InProgress".into()),
        KeyCode::Char('3') => app.ui.tasks_ui.filter = Some("Blocked".into()),
        KeyCode::Char('4') => app.ui.tasks_ui.filter = Some("Done".into()),
        _ => {}
    }
}

pub fn draw<B: WorkspaceBackend>(frame: &mut Frame, app: &mut App<B>, area: Rect) {
    let [content, hint] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let filter = app.ui.tasks_ui.filter.clone();

    match app.cached_tasks() {
        Some(tasks) => {
            let filtered: Vec<_> = tasks
                .iter()
                .filter(|t| filter.as_deref().is_none_or(|f| t.status == f))
                .collect();
            if filtered.is_empty() {
                let state = if tasks.is_empty() {
                    ViewState::Empty {
                        hint: "create tasks with `awh task create`",
                    }
                } else {
                    ViewState::Empty {
                        hint: "no tasks match the active filter",
                    }
                };
                render_state_placeholder(
                    frame,
                    Rect {
                        x: content.x + 1,
                        y: content.y + 1,
                        width: content.width.saturating_sub(2),
                        height: content.height.saturating_sub(2),
                    },
                    &state,
                );
                frame.render_widget(Block::bordered().title(" tasks "), content);
            } else {
                let items: Vec<ListItem> = filtered
                    .iter()
                    .map(|t| {
                        let role = task_role(&t.status);
                        let assignee = t.assignee.clone().unwrap_or_else(|| "—".into());
                        ListItem::new(Line::from(vec![
                            Span::styled(format!("{} ", role.symbol()), role.style()),
                            Span::styled(format!("{:<12} ", t.status), role.style()),
                            Span::styled(format!("{:<14} ", assignee), Role::Muted.style()),
                            Span::raw(t.title.clone()),
                        ]))
                    })
                    .collect();
                let list = List::new(items)
                    .block(Block::bordered().title(match &filter {
                        Some(f) => format!(" tasks · {f} "),
                        None => " tasks ".to_string(),
                    }))
                    .highlight_style(Role::Selected.style());
                frame.render_stateful_widget(list, content, &mut app.ui.tasks);
            }
        }
        None => {
            render_state_placeholder(
                frame,
                Rect {
                    x: content.x + 1,
                    y: content.y + 1,
                    width: content.width.saturating_sub(2),
                    height: content.height.saturating_sub(2),
                },
                &ViewState::Unavailable {
                    reason: "task store not available on this backend",
                },
            );
            frame.render_widget(Block::bordered().title(" tasks "), content);
        }
    }

    hint_line(
        frame,
        hint,
        "[a] all  [1] todo  [2] in-progress  [3] blocked  [4] done  [↑/↓] select",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::backend::LocalBackend;
    use crate::tui::screens::ScreenId;

    fn app_with_project_task() -> App<LocalBackend> {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        let mut app = App::new(LocalBackend::new(root));
        app.backend.create_project("alpha").unwrap();
        app.backend.open_project("alpha").unwrap();
        let project_root =
            crate::core::workspace::Workspace::new(app_root(&app)).project_path("alpha");
        let store = crate::core::tasks::TaskStore::new(project_root).unwrap();
        store
            .create(
                "t-1".into(),
                "ship the release".into(),
                "v1 of everything".into(),
                crate::core::tasks::TaskPriority::High,
                vec![],
            )
            .unwrap();
        app
    }

    fn app_root(app: &App<LocalBackend>) -> std::path::PathBuf {
        app.backend.root().to_path_buf()
    }

    #[test]
    fn tasks_view_lists_project_tasks_with_status_symbols() {
        let mut app = app_with_project_task();
        app.goto(ScreenId::Tasks);
        let tasks = app.cached_tasks().cloned().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, "Todo");
        let role = task_role(&tasks[0].status);
        assert_eq!(role, Role::Pending);
    }

    #[test]
    fn status_filter_hides_non_matching_tasks() {
        let mut app = app_with_project_task();
        app.goto(ScreenId::Tasks);
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('4')));
        assert_eq!(app.ui.tasks_ui.filter.as_deref(), Some("Done"));
        let count = app
            .cached_tasks()
            .map(|t| t.iter().filter(|t| t.status == "Done").count())
            .unwrap_or(0);
        assert_eq!(count, 0);
    }
}
