//! Memory screen (spec section 13): a read-mostly view of the focused
//! project's append-only memory log (`.agent/memory.jsonl`), with a
//! simple append prompt. Entries are listed newest first; memory is
//! never editable in place (append-only by design).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Paragraph, Wrap};

use super::hint_line;
use crate::models::MemoryEntry;
use crate::tui::app::App;
use crate::tui::backend::WorkspaceBackend;

#[derive(Default)]
pub struct MemoryUi {
    /// Loaded entries, newest first.
    pub entries: Vec<MemoryEntry>,
    /// Input buffer for the append prompt.
    pub input: String,
    pub input_active: bool,
}

impl MemoryUi {
    pub fn load<B: WorkspaceBackend>(&mut self, backend: &B) -> Result<(), String> {
        let entries = backend
            .list_memory(backend.current_project_hint().as_deref())
            .map_err(|e| format!("read memory: {e:#}"))?;
        self.entries = entries.into_iter().rev().collect();
        Ok(())
    }
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    let ui = &mut app.ui.memory_ui;
    let project = app.backend.current_project_hint();

    if ui.input_active {
        match key.code {
            KeyCode::Enter => {
                ui.input_active = false;
                let content = ui.input.trim().to_string();
                ui.input.clear();
                if content.is_empty() {
                    app.set_error("memory content cannot be empty");
                } else {
                    match app.backend.append_memory(project.as_deref(), &content) {
                        Ok(()) => {
                            if let Err(msg) = ui.load(&app.backend) {
                                app.set_error(msg);
                            } else {
                                app.set_message("memory recorded");
                            }
                        }
                        Err(e) => app.set_error(format!("append memory: {e:#}")),
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
        KeyCode::Char('a') => ui.input_active = true,
        KeyCode::Char('r') => {
            if let Err(msg) = ui.load(&app.backend) {
                app.set_error(msg);
            } else {
                app.set_message("memory reloaded");
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
    let ui = &app.ui.memory_ui;
    let project = app.backend.current_project_hint();
    let target = project.as_deref().unwrap_or("(workspace root)");

    if ui.input_active {
        frame.render_widget(
            Paragraph::new(format!("record memory: {}", ui.input))
                .style(Style::default().fg(Color::Yellow))
                .block(block),
            area,
        );
        hint_line(frame, area, "[Enter] append  [Esc] cancel");
        return;
    }

    let mut lines: Vec<ratatui::text::Line> = Vec::new();
    if ui.entries.is_empty() {
        lines.push(ratatui::text::Line::from(
            "no memory recorded for this scope",
        ));
        lines.push(ratatui::text::Line::from(
            "press a to append a memory entry",
        ));
    }
    for entry in &ui.entries {
        lines.push(ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(
                format!("{}  ", short_ts(&entry.timestamp)),
                Style::default().fg(Color::Cyan),
            ),
            ratatui::text::Span::raw(&entry.content),
        ]));
    }
    let block = block.title(format!(
        " memory — {target} ({} entries, newest first) ",
        ui.entries.len()
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
    hint_line(frame, area, "[a] append  [r] reload  [Esc] back");
}

/// Compact timestamp: `2026-09-02T14:37:34Z` -> `09-02 14:37`.
fn short_ts(ts: &str) -> String {
    // RFC 3339 prefix is fixed-width; slice defensively on failure.
    let Some((date, rest)) = ts.split_once('T') else {
        return ts.chars().take(16).collect();
    };
    let month_day: String = date.chars().skip(5).collect();
    let time: String = rest.chars().take(5).collect();
    format!("{month_day} {time}")
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
    fn append_then_load_lists_newest_first() {
        let mut app = app_with_project("alpha");
        app.backend.append_memory(Some("alpha"), "first").unwrap();
        app.backend.append_memory(Some("alpha"), "second").unwrap();
        let ui = &mut app.ui.memory_ui;
        ui.load(&app.backend).unwrap();
        assert_eq!(ui.entries.len(), 2);
        assert_eq!(ui.entries[0].content, "second");
        assert_eq!(ui.entries[1].content, "first");
    }

    #[test]
    fn memory_is_scoped_to_focused_project() {
        let app = app_with_project("beta");
        app.backend.create_project("other").unwrap();
        app.backend
            .append_memory(Some("beta"), "beta fact")
            .unwrap();
        app.backend
            .append_memory(Some("other"), "other fact")
            .unwrap();
        assert_eq!(app.backend.list_memory(Some("beta")).unwrap().len(), 1);
        assert_eq!(app.backend.list_memory(Some("other")).unwrap().len(), 1);
    }

    #[test]
    fn short_ts_compacts_rfc3339() {
        assert_eq!(short_ts("2026-09-02T14:37:34.123456789Z"), "09-02 14:37");
        assert_eq!(short_ts("not-a-timestamp"), "not-a-timestamp");
    }
}
