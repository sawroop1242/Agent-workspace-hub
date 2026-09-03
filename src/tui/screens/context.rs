//! Context screen (spec sections 7, 12): view and edit the focused
//! project's persistent context file (`.agent/context.md`). The screen
//! targets the project opened via the Projects screen (`o`); with no
//! project focused it operates on the workspace root. Writes are
//! bounded to a byte cap so a runaway buffer cannot produce a giant
//! file, and the engine summary shows how the context budget is set.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Paragraph, Wrap};

use super::hint_line;
use crate::tui::app::App;
use crate::tui::backend::WorkspaceBackend;

#[derive(Default)]
pub struct ContextUi {
    /// Loaded (or last saved) context content.
    pub saved: Option<String>,
    pub buffer: String,
    pub cursor: usize,
    pub dirty: bool,
    /// True while editing is active (edit mode).
    pub editing: bool,
}

impl ContextUi {
    pub fn load<B: WorkspaceBackend>(&mut self, backend: &B) -> Result<(), String> {
        let content = backend
            .read_context(backend_current_project(backend).as_deref())
            .map_err(|e| format!("read context: {e:#}"))?;
        self.saved = Some(content.clone());
        self.buffer = content;
        self.cursor = self.buffer.len();
        self.dirty = false;
        Ok(())
    }

    fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.dirty = true;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut step = 1;
        while self.cursor > step && !self.buffer.is_char_boundary(self.cursor - step) {
            step += 1;
        }
        let start = self.cursor - step;
        self.buffer.replace_range(start..self.cursor, "");
        self.cursor = start;
        self.dirty = true;
    }

    /// Line/column derived from the byte cursor.
    fn line_col(&self) -> (usize, usize) {
        let before = &self.buffer[..self.cursor.min(self.buffer.len())];
        let line = before.matches('\n').count() + 1;
        let col = before
            .rsplit_once('\n')
            .map_or(before.len(), |(_, last)| last.len())
            + 1;
        (line, col)
    }
}

/// The focused project name, if any. The App holds the authoritative
/// focus; this keeps the load path symmetric with the key handler.
fn backend_current_project<B: WorkspaceBackend>(backend: &B) -> Option<String> {
    backend.current_project_hint()
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    let ui = &mut app.ui.context_ui;
    let project = app.backend.current_project_hint();

    if !ui.editing {
        match key.code {
            KeyCode::Char('e') => {
                ui.editing = true;
                if ui.saved.is_none() {
                    if let Err(msg) = ui.load(&app.backend) {
                        app.set_error(msg);
                    }
                }
            }
            KeyCode::Char('r') => {
                if let Err(msg) = ui.load(&app.backend) {
                    app.set_error(msg);
                } else {
                    ui.editing = false;
                    app.set_message("context reloaded");
                }
            }
            KeyCode::Char('s') if ui.dirty => {
                match app.backend.write_context(project.as_deref(), &ui.buffer) {
                    Ok(()) => {
                        ui.saved = Some(ui.buffer.clone());
                        ui.dirty = false;
                        app.set_message("context saved");
                    }
                    Err(e) => app.set_error(format!("save context: {e:#}")),
                }
            }
            KeyCode::Esc => {
                if ui.dirty {
                    // Leave edit mode but keep the buffer; the operator
                    // can save or reload deliberately.
                    ui.editing = false;
                    app.set_error("unsaved changes — [s] to save, [r] to reload");
                } else {
                    app.back();
                }
            }
            _ => {}
        }
        return;
    }

    // Editing mode.
    match key.code {
        KeyCode::Esc => {
            ui.editing = false;
        }
        KeyCode::Backspace => ui.backspace(),
        KeyCode::Left => ui.cursor = ui.cursor.saturating_sub(1),
        KeyCode::Right => ui.cursor = (ui.cursor + 1).min(ui.buffer.len()),
        KeyCode::Home => {
            let line_start = ui.buffer[..ui.cursor.min(ui.buffer.len())]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0);
            ui.cursor = line_start;
        }
        KeyCode::End => {
            let rest = &ui.buffer[ui.cursor.min(ui.buffer.len())..];
            let line_end = rest.find('\n').unwrap_or(rest.len());
            ui.cursor += line_end;
        }
        KeyCode::Char('s')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            match app.backend.write_context(project.as_deref(), &ui.buffer) {
                Ok(()) => {
                    ui.saved = Some(ui.buffer.clone());
                    ui.dirty = false;
                    ui.editing = false;
                    app.set_message("context saved");
                }
                Err(e) => app.set_error(format!("save context: {e:#}")),
            }
        }
        KeyCode::Char(c) => ui.insert_char(c),
        _ => {}
    }
}

pub fn draw<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: ratatui::widgets::Block<'_>,
) {
    let ui = &app.ui.context_ui;
    let project = app.backend.current_project_hint();
    let target = project.as_deref().unwrap_or("(workspace root)");

    let (line, col) = ui.line_col();
    let dirty = if ui.dirty { " *dirty*" } else { "" };
    let block = block.title(format!(" context — {target}{dirty} — {line}:{col} "));
    let mut para = Paragraph::new(ui.buffer.as_str())
        .block(block)
        .wrap(Wrap { trim: false });
    if ui.dirty {
        para = para.style(Style::default().fg(Color::LightYellow));
    } else if ui.editing {
        para = para.style(Style::default().fg(Color::Gray));
    } else {
        para = para.style(Style::default().fg(Color::Gray).add_modifier(Modifier::DIM));
    }
    frame.render_widget(para, area);
    if ui.editing {
        hint_line(
            frame,
            area,
            "[C-s] save  [Esc] stop editing  [chars] type  [arrows/Home/End] move",
        );
    } else {
        hint_line(frame, area, "[e] edit  [s] save  [r] reload  [Esc] back");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::backend::LocalBackend;

    fn app_with_project(name: &str) -> App<LocalBackend> {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        let mut app = App::new(LocalBackend::new(root));
        app.backend.create_project(name).unwrap();
        app.backend.open_project(name).unwrap();
        app
    }

    #[test]
    fn context_roundtrip_through_screen() {
        let mut app = app_with_project("alpha");
        app.backend
            .write_context(Some("alpha"), "project conventions")
            .unwrap();
        let ui = &mut app.ui.context_ui;
        assert!(ui.load(&app.backend).is_ok());
        assert_eq!(ui.buffer, "project conventions");
        assert!(!ui.dirty);
    }

    #[test]
    fn empty_context_when_file_missing() {
        let mut app = app_with_project("beta");
        let ui = &mut app.ui.context_ui;
        assert!(ui.load(&app.backend).is_ok());
        assert_eq!(ui.buffer, "");
    }

    #[test]
    fn writes_are_scoped_to_focused_project() {
        let app = app_with_project("gamma");
        app.backend
            .write_context(Some("gamma"), "gamma notes")
            .unwrap();
        // Writing via a different focus must not touch gamma's context.
        app.backend.write_context(None, "root notes").unwrap();
        assert_eq!(
            app.backend.read_context(Some("gamma")).unwrap(),
            "gamma notes"
        );
        assert_eq!(app.backend.read_context(None).unwrap(), "root notes");
    }
}
