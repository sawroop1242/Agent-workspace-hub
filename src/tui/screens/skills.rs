//! Skills screen (spec section 14): global skills installed under
//! `~/.agent-workspace-hub/skills` and the ones the focused project
//! references from that registry. Toggle (`t`) adds/removes a project
//! reference; skills are referenced by name, never copied.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use super::hint_line;
use crate::skills::Skill;
use crate::tui::app::App;
use crate::tui::backend::WorkspaceBackend;

#[derive(Default)]
pub struct SkillsUi {
    /// Global registry skills.
    pub global: Vec<Skill>,
    /// Skills referenced by the focused project.
    pub project: Vec<Skill>,
    /// 0 = global list, 1 = project list.
    pub pane: u8,
    pub global_state: ListState,
    pub project_state: ListState,
}

impl SkillsUi {
    pub fn load<B: WorkspaceBackend>(&mut self, backend: &B) -> Result<(), String> {
        self.global = backend
            .list_global_skills()
            .map_err(|e| format!("list global skills: {e:#}"))?;
        self.project = backend
            .list_project_skills(backend.current_project_hint().as_deref())
            .map_err(|e| format!("list project skills: {e:#}"))?;
        self.global_state.select(if self.global.is_empty() {
            None
        } else {
            Some(0)
        });
        self.project_state.select(if self.project.is_empty() {
            None
        } else {
            Some(0)
        });
        Ok(())
    }

    fn selected_global(&self) -> Option<&Skill> {
        self.global_state
            .selected()
            .and_then(|i| self.global.get(i))
    }

    fn selected_project(&self) -> Option<&Skill> {
        self.project_state
            .selected()
            .and_then(|i| self.project.get(i))
    }

    fn move_selection(&mut self, delta: i32) {
        let state = if self.pane == 0 {
            &mut self.global_state
        } else {
            &mut self.project_state
        };
        let len = if self.pane == 0 {
            self.global.len()
        } else {
            self.project.len()
        };
        if len == 0 {
            state.select(None);
            return;
        }
        let current = state.selected().unwrap_or(0);
        let next = (current as i32 + delta).rem_euclid(len as i32) as usize;
        state.select(Some(next));
    }
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    let ui = &mut app.ui.skills_ui;
    let project = app.backend.current_project_hint();

    match key.code {
        KeyCode::Tab => {
            ui.pane = if ui.pane == 0 { 1 } else { 0 };
        }
        KeyCode::Up => ui.move_selection(-1),
        KeyCode::Down => ui.move_selection(1),
        KeyCode::Char('r') => {
            if let Err(msg) = ui.load(&app.backend) {
                app.set_error(msg);
            } else {
                app.set_message("skills reloaded");
            }
        }
        KeyCode::Char('t') | KeyCode::Enter => {
            let Some(skill) = (if ui.pane == 0 {
                ui.selected_global()
            } else {
                ui.selected_project()
            }) else {
                return;
            };
            let name = skill.name.clone();
            let pane = ui.pane;
            let outcome = app.backend.toggle_project_skill(project.as_deref(), &name);
            match outcome {
                Ok(true) => {
                    let action = if pane == 0 { "added" } else { "removed" };
                    if let Err(msg) = ui.load(&app.backend) {
                        app.set_error(msg);
                    } else {
                        app.set_message(format!("project reference {action} for {name}"));
                    }
                }
                Ok(false) => app.set_message("no change"),
                Err(e) => app.set_error(format!("toggle skill: {e:#}")),
            }
        }
        _ => {}
    }
}

pub fn draw<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: ratatui::widgets::Block<'_>,
) {
    let ui = &app.ui.skills_ui;
    let project = app.backend.current_project_hint();
    let target = project.as_deref().unwrap_or("(workspace root)");

    if ui.global.is_empty() && ui.project.is_empty() {
        let lines = vec![
            ratatui::text::Line::from("no skills installed"),
            ratatui::text::Line::from(""),
            ratatui::text::Line::from("install one with: awh skills install <name>"),
        ];
        frame.render_widget(Paragraph::new(lines).block(block), area);
        hint_line(frame, area, "[r] reload  [Esc] back");
        return;
    }

    let cols =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    let (left, right) = (cols[0], cols[1]);

    let global_items: Vec<ListItem> = ui
        .global
        .iter()
        .map(|s| ListItem::new(format!("{}\n  {}", s.name, first_line(&s.description))))
        .collect();
    let project_items: Vec<ListItem> = ui
        .project
        .iter()
        .map(|s| ListItem::new(format!("{}\n  {}", s.name, first_line(&s.description))))
        .collect();

    let global_block = ratatui::widgets::Block::bordered()
        .title(if ui.pane == 0 {
            format!(" global registry * ({}) ", ui.global.len())
        } else {
            format!(" global registry ({}) ", ui.global.len())
        })
        .border_style(if ui.pane == 0 {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    let project_block = ratatui::widgets::Block::bordered()
        .title(if ui.pane == 1 {
            format!(" project * {target} ({}) ", ui.project.len())
        } else {
            format!(" project {target} ({}) ", ui.project.len())
        })
        .border_style(if ui.pane == 1 {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let mut global_list = List::new(global_items).block(global_block);
    let mut project_list = List::new(project_items).block(project_block);
    if ui.pane == 0 {
        global_list = global_list.highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    } else {
        project_list = project_list.highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    }
    frame.render_stateful_widget(global_list, left, &mut app.ui.skills_ui.global_state);
    frame.render_stateful_widget(project_list, right, &mut app.ui.skills_ui.project_state);

    hint_line(
        frame,
        area,
        "[Tab] switch pane  [t] toggle project reference  [r] reload",
    );
}

/// First line of a description, truncated for one-row list items.
fn first_line(desc: &str) -> String {
    let line: String = desc
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .chars()
        .take(60)
        .collect();
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_selection_wraps_both_directions() {
        let mut ui = SkillsUi {
            global: vec![skill_named("a"), skill_named("b"), skill_named("c")],
            ..Default::default()
        };
        ui.global_state.select(Some(0));
        ui.move_selection(-1);
        assert_eq!(ui.global_state.selected(), Some(2));
        ui.move_selection(1);
        assert_eq!(ui.global_state.selected(), Some(0));
        ui.move_selection(5);
        assert_eq!(ui.global_state.selected(), Some(2));
    }

    #[test]
    fn empty_pane_keeps_selection_none() {
        let mut ui = SkillsUi::default();
        ui.move_selection(1);
        assert!(ui.global_state.selected().is_none());
    }

    #[test]
    fn first_line_takes_trimmed_first_nonempty() {
        assert_eq!(first_line("\n  hello world\nrest"), "hello world");
        assert_eq!(first_line(""), "");
    }

    fn skill_named(name: &str) -> Skill {
        Skill {
            name: name.to_owned(),
            description: "test skill\nsecond line".to_owned(),
            version: None,
            path: std::path::PathBuf::from("/tmp"),
        }
    }
}
