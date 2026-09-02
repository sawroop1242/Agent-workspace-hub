//! Files screen: directory browser with create/rename/delete, path
//! breadcrumbs, and project search. Every filesystem operation goes
//! through the backend (which enforces workspace boundaries).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{List, ListItem, Paragraph};

use super::hint_line;
use crate::tui::app::{ActionKind, App};
use crate::tui::backend::WorkspaceBackend;

/// What the single-line input is currently collecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Creating a new file or directory (`name/` suffix means dir).
    Create,
    /// Renaming the selected entry.
    Rename,
    /// Searching file contents.
    Search,
}

#[derive(Default)]
pub struct FilesUi {
    /// Current directory relative to the workspace root ("" = root).
    pub cwd: String,
    pub input: String,
    pub input_mode: Option<InputMode>,
    /// Result lines from the last search.
    pub search_results: Vec<String>,
    pub show_search: bool,
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    let ui = &mut app.ui.files_ui;
    if let Some(mode) = ui.input_mode {
        match key.code {
            KeyCode::Esc => {
                ui.input_mode = None;
                ui.input.clear();
            }
            KeyCode::Backspace => {
                ui.input.pop();
            }
            KeyCode::Enter => {
                let value = ui.input.trim().to_string();
                ui.input.clear();
                ui.input_mode = None;
                if !value.is_empty() {
                    apply_input(app, mode, value);
                }
            }
            KeyCode::Char(c) => ui.input.push(c),
            _ => {}
        }
        return;
    }

    let entries = app.backend.list_dir(&ui.cwd).unwrap_or_default();
    let selected = app
        .ui
        .files
        .selected()
        .unwrap_or(0)
        .min(entries.len().saturating_sub(1));
    let rel = |name: &str| {
        if ui.cwd.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", ui.cwd, name)
        }
    };

    match key.code {
        KeyCode::Up => {
            if selected > 0 {
                app.ui.files.select(Some(selected - 1));
            }
        }
        KeyCode::Down => {
            if !entries.is_empty() {
                app.ui
                    .files
                    .select(Some((selected + 1).min(entries.len() - 1)));
            }
        }
        KeyCode::Enter => {
            if let Some(entry) = entries.get(selected) {
                if entry.is_dir {
                    ui.cwd = rel(&entry.name);
                    app.ui.files.select(Some(0));
                } else {
                    app.ui.editor_ui.path = Some(rel(&entry.name));
                    app.goto(crate::tui::screens::ScreenId::Editor);
                }
            }
        }
        KeyCode::Backspace => {
            // Go up one directory; stays inside the workspace root.
            ui.cwd = match ui.cwd.rsplit_once('/') {
                Some((parent, _)) => parent.to_string(),
                None => String::new(),
            };
            app.ui.files.select(Some(0));
        }
        KeyCode::Char('n') => {
            ui.input_mode = Some(InputMode::Create);
        }
        KeyCode::Char('r') => {
            if entries.is_empty() {
                app.set_error("nothing selected to rename");
            } else {
                ui.input = entries[selected].name.clone();
                ui.input_mode = Some(InputMode::Rename);
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(entry) = entries.get(selected) {
                let path = rel(&entry.name);
                app.request_action(ActionKind::DeletePath(path));
            }
        }
        KeyCode::Char('s') => {
            ui.input_mode = Some(InputMode::Search);
            ui.show_search = true;
        }
        KeyCode::Esc => {
            ui.show_search = false;
            ui.search_results.clear();
        }
        _ => {}
    }
}

fn apply_input<B: WorkspaceBackend>(app: &mut App<B>, mode: InputMode, value: String) {
    let ui = &app.ui.files_ui;
    let entries = app.backend.list_dir(&ui.cwd).unwrap_or_default();
    let selected = app
        .ui
        .files
        .selected()
        .unwrap_or(0)
        .min(entries.len().saturating_sub(1));
    let join = |name: &str| {
        if ui.cwd.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", ui.cwd, name)
        }
    };
    match mode {
        InputMode::Create => {
            let (name, is_dir) = match value.strip_suffix('/') {
                Some(stripped) => (stripped.to_string(), true),
                None => (value, false),
            };
            if is_dir {
                match app.backend.create_dir(&join(&name)) {
                    Ok(()) => app.set_message(format!("created dir: {name}")),
                    Err(e) => app.set_error(format!("mkdir {name}: {e:#}")),
                }
            } else {
                match app.backend.write_file(&join(&name), "") {
                    Ok(true) => app.set_message(format!("created file: {name}")),
                    Ok(false) => app.set_error("write refused"),
                    Err(e) => app.set_error(format!("create {name}: {e:#}")),
                }
            }
        }
        InputMode::Rename => {
            if entries.is_empty() {
                return;
            }
            let from = join(&entries[selected].name);
            let to = join(&value);
            match app.backend.rename_path(&from, &to) {
                Ok(()) => app.set_message(format!("renamed to: {value}")),
                Err(e) => app.set_error(format!("rename: {e:#}")),
            }
        }
        InputMode::Search => match app.backend.search_files(&value, 50) {
            Ok(hits) => {
                let ui = &mut app.ui.files_ui;
                ui.search_results = hits
                    .iter()
                    .map(|h| format!("{}:{}: {}", h.path, h.line_number, h.line))
                    .collect();
                if ui.search_results.is_empty() {
                    app.set_message(format!("no matches for {value:?}"));
                }
            }
            Err(e) => app.set_error(format!("search: {e:#}")),
        },
    }
}

pub fn draw<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: ratatui::widgets::Block<'_>,
) {
    let ui = &mut app.ui.files_ui;
    let cwd_display = if ui.cwd.is_empty() { "/" } else { &ui.cwd };

    let [browser, bottom] = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Min(3),
        ratatui::layout::Constraint::Length(if ui.show_search { 6 } else { 1 }),
    ])
    .areas(area);

    let entries = app.backend.list_dir(&ui.cwd).unwrap_or_default();
    let mut items: Vec<ListItem> = Vec::new();
    if !ui.cwd.is_empty() {
        items.push(ListItem::new(".."));
    }
    items.extend(entries.iter().map(|e| {
        let marker = if e.is_dir { "dir " } else { "file" };
        ListItem::new(format!("{marker}  {}", e.name))
    }));
    let list = List::new(items)
        .block(block.title(format!(" Files — {cwd_display} ")))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, browser, &mut app.ui.files);

    if let Some(mode) = ui.input_mode {
        let label = match mode {
            InputMode::Create => "new name (name/ for dir)",
            InputMode::Rename => "rename to",
            InputMode::Search => "search for",
        };
        frame.render_widget(
            Paragraph::new(format!("{label}: {}", ui.input))
                .style(Style::default().fg(Color::Yellow)),
            bottom,
        );
    } else if ui.show_search {
        let results = if ui.search_results.is_empty() {
            vec!["(no results)".to_string()]
        } else {
            ui.search_results.clone()
        };
        frame.render_widget(
            Paragraph::new(results.join("\n")).style(Style::default().fg(Color::Gray)),
            bottom,
        );
        hint_line(
            frame,
            area,
            "[Enter] open  [Backspace] up  [n] new  [r] rename  [d/Del] delete (confirms)  [s] search  [Esc] hide results",
        );
    } else {
        hint_line(
            frame,
            area,
            "[Enter] open  [Backspace] up  [n] new  [r] rename  [d/Del] delete (confirms)  [s] search",
        );
    }
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
    fn enter_opens_text_file_in_editor() {
        let mut app = app();
        app.backend.write_file("note.md", "hi").unwrap();
        app.goto(ScreenId::Files);
        app.ui.files.select(Some(0));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), ScreenId::Editor);
        assert_eq!(app.ui.editor_ui.path.as_deref(), Some("note.md"));
    }

    #[test]
    fn enter_into_directory_updates_cwd_and_backspace_climbs() {
        let mut app = app();
        app.backend.create_dir("sub").unwrap();
        app.backend.write_file("sub/a.txt", "x").unwrap();
        app.goto(ScreenId::Files);
        app.ui.files.select(Some(0));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.ui.files_ui.cwd, "sub");
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.ui.files_ui.cwd, "");
        // Backspace at the root stays at the root.
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.ui.files_ui.cwd, "");
    }

    #[test]
    fn create_file_and_directory_via_input_modes() {
        let mut app = app();
        app.goto(ScreenId::Files);
        press(&mut app, KeyCode::Char('n'));
        type_string(&mut app, "file.txt");
        press(&mut app, KeyCode::Enter);
        assert!(app.backend.read_file("file.txt").is_ok());

        press(&mut app, KeyCode::Char('n'));
        type_string(&mut app, "folder/");
        press(&mut app, KeyCode::Enter);
        assert!(app.backend.list_dir("folder").is_ok());
    }

    #[test]
    fn rename_selected_entry() {
        let mut app = app();
        app.backend.write_file("old.txt", "1").unwrap();
        app.goto(ScreenId::Files);
        app.ui.files.select(Some(0));
        press(&mut app, KeyCode::Char('r'));
        // Clear the seeded name entirely, then type the new one.
        for _ in 0.."old.txt".len() {
            press(&mut app, KeyCode::Backspace);
        }
        type_string(&mut app, "new.txt");
        press(&mut app, KeyCode::Enter);
        assert!(app.backend.read_file("new.txt").is_ok());
        assert!(app.backend.read_file("old.txt").is_err());
    }

    #[test]
    fn delete_requests_confirmation_then_removes() {
        let mut app = app();
        app.backend.write_file("gone.txt", "x").unwrap();
        app.goto(ScreenId::Files);
        app.ui.files.select(Some(0));
        press(&mut app, KeyCode::Delete);
        match app.confirm.as_ref().map(|a| &a.kind) {
            Some(ActionKind::DeletePath(path)) => assert_eq!(path, "gone.txt"),
            other => panic!("unexpected pending action: {other:?}"),
        }
        app.confirm_pending();
        assert!(app.backend.read_file("gone.txt").is_err());
    }

    #[test]
    fn search_collects_hits() {
        let mut app = app();
        app.backend.write_file("a.md", "needle here").unwrap();
        app.backend.write_file("b.md", "nothing").unwrap();
        app.goto(ScreenId::Files);
        press(&mut app, KeyCode::Char('s'));
        type_string(&mut app, "needle");
        press(&mut app, KeyCode::Enter);
        let results = &app.ui.files_ui.search_results;
        assert_eq!(results.len(), 1);
        assert!(results[0].starts_with("a.md:1"));
    }
}
