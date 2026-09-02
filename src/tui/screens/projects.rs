//! Projects screen: list, create, open (focus Files), delete with
//! modal confirmation.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{List, ListItem, Paragraph};

use super::hint_line;
use crate::tui::app::{ActionKind, App};
use crate::tui::backend::WorkspaceBackend;

/// Input mode for the new-project name form.
#[derive(Default)]
pub struct ProjectsUi {
    pub input: String,
    pub input_active: bool,
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    let ui = &mut app.ui.projects_ui;
    if ui.input_active {
        match key.code {
            KeyCode::Enter => {
                let name = ui.input.trim().to_string();
                ui.input_active = false;
                ui.input.clear();
                if name.is_empty() {
                    app.set_error("project name cannot be empty");
                } else {
                    match app.backend.create_project(&name) {
                        Ok(()) => app.set_message(format!("created project: {name}")),
                        Err(e) => app.set_error(format!("create {name}: {e:#}")),
                    }
                }
            }
            KeyCode::Esc => {
                ui.input_active = false;
                ui.input.clear();
            }
            KeyCode::Backspace => {
                ui.input.pop();
            }
            KeyCode::Char(c) => ui.input.push(c),
            _ => {}
        }
        return;
    }
    match key.code {
        KeyCode::Char('n') => {
            ui.input_active = true;
            ui.input.clear();
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            let Some(name) = selected_project(app) else {
                return;
            };
            app.request_action(ActionKind::DeleteProject(name));
        }
        KeyCode::Char('o') | KeyCode::Enter => {
            let Some(name) = selected_project(app) else {
                return;
            };
            if let Err(e) = app.backend.open_project(&name) {
                app.set_error(format!("open project: {e:#}"));
                return;
            }
            app.ui.files_ui.cwd = name;
            app.goto(crate::tui::screens::ScreenId::Files);
        }
        _ => {}
    }
}

fn selected_project<B: WorkspaceBackend>(app: &App<B>) -> Option<String> {
    let projects = app.backend.list_projects().ok()?;
    if projects.is_empty() {
        return None;
    }
    let index = app
        .ui
        .projects
        .selected()
        .unwrap_or(0)
        .min(projects.len() - 1);
    Some(projects[index].clone())
}

pub fn draw<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: ratatui::widgets::Block<'_>,
) {
    let ui = &app.ui.projects_ui;
    if ui.input_active {
        frame.render_widget(
            Paragraph::new(format!("new project name: {}", ui.input))
                .style(Style::default().fg(Color::Yellow))
                .block(block),
            area,
        );
    } else {
        let projects = app.backend.list_projects().unwrap_or_default();
        let items: Vec<ListItem> = projects.iter().map(|n| ListItem::new(n.clone())).collect();
        let list = List::new(items).block(block).highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
        frame.render_stateful_widget(list, area, &mut app.ui.projects);
        if projects.is_empty() {
            frame.render_widget(
                Paragraph::new("no projects — press n to create one")
                    .style(Style::default().fg(Color::DarkGray)),
                area.inner(ratatui::layout::Margin::new(2, 1)),
            );
        }
    }
    hint_line(
        frame,
        area,
        if ui.input_active {
            "[Enter] create  [Esc] cancel"
        } else {
            "[n] new  [o] open  [d/Del] delete (confirms)  [Up/Dn] select"
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{ActionKind, App};
    use crate::tui::backend::LocalBackend;
    use crate::tui::screens::ScreenId;

    fn app() -> App<LocalBackend> {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        App::new(LocalBackend::new(root))
    }

    fn press(app: &mut App<LocalBackend>, code: KeyCode) {
        super::handle_key(app, KeyEvent::from(code));
    }

    fn type_string(app: &mut App<LocalBackend>, s: &str) {
        for c in s.chars() {
            super::handle_key(app, KeyEvent::from(KeyCode::Char(c)));
        }
    }

    #[test]
    fn create_project_via_form() {
        let mut app = app();
        app.goto(ScreenId::Projects);
        press(&mut app, KeyCode::Char('n'));
        type_string(&mut app, "demo");
        press(&mut app, KeyCode::Enter);
        assert!(app
            .backend
            .list_projects()
            .unwrap()
            .iter()
            .any(|p| p == "demo"));
    }

    #[test]
    fn create_project_rejects_traversal_names() {
        let mut app = app();
        app.goto(ScreenId::Projects);
        press(&mut app, KeyCode::Char('n'));
        type_string(&mut app, "../evil");
        press(&mut app, KeyCode::Enter);
        assert!(app.error.is_some());
        assert!(app.backend.list_projects().unwrap().is_empty());
    }

    #[test]
    fn open_project_navigates_to_files_in_that_project() {
        let mut app = app();
        app.backend.create_project("alpha").unwrap();
        app.goto(ScreenId::Projects);
        app.ui.projects.select(Some(0));
        press(&mut app, KeyCode::Char('o'));
        assert_eq!(app.screen(), ScreenId::Files);
        assert_eq!(app.ui.files_ui.cwd, "alpha");
    }

    #[test]
    fn delete_confirms_then_removes_project() {
        let mut app = app();
        app.backend.create_project("alpha").unwrap();
        app.goto(ScreenId::Projects);
        app.ui.projects.select(Some(0));
        press(&mut app, KeyCode::Delete);
        match app.confirm.as_ref().map(|a| &a.kind) {
            Some(ActionKind::DeleteProject(name)) => assert_eq!(name, "alpha"),
            other => panic!("unexpected pending action: {other:?}"),
        }
        app.confirm_pending();
        assert!(app.backend.list_projects().unwrap().is_empty());
    }

    #[test]
    fn escape_cancels_the_new_project_form() {
        let mut app = app();
        app.goto(ScreenId::Projects);
        press(&mut app, KeyCode::Char('n'));
        type_string(&mut app, "half");
        press(&mut app, KeyCode::Esc);
        assert!(!app.ui.projects_ui.input_active);
        assert!(app.backend.list_projects().unwrap().is_empty());
    }
}
