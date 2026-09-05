//! Editor screen: plain-text editing with dirty-state tracking, save,
//! discard-with-confirmation, and safe handling of large or binary
//! files (they are never loaded into memory).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Paragraph, Wrap};

use super::hint_line;
use crate::services::files::MAX_FILE_BYTES;
use crate::tui::app::{ActionKind, App};
use crate::tui::backend::WorkspaceBackend;

#[derive(Default)]
pub struct EditorUi {
    /// Path being edited, relative to the workspace root.
    pub path: Option<String>,
    /// Loaded (or last saved) content.
    pub saved: Option<String>,
    /// Current buffer contents.
    pub buffer: String,
    /// Cursor: byte offset into `buffer` for inserts; line/col derived.
    pub cursor: usize,
    pub dirty: bool,
    /// Path typed into the open-file prompt.
    pub open_input: String,
    pub open_active: bool,
    /// Incremental search: query, prompt state, and last match offset.
    pub search_input: String,
    pub search_active: bool,
    pub last_match: Option<usize>,
    /// Message explaining why a file was refused (too large / binary).
    pub refusal: Option<String>,
}

impl EditorUi {
    /// Opens a file through the backend, refusing large/binary files
    /// before reading their bytes.
    pub fn load<B: WorkspaceBackend>(&mut self, backend: &B, path: &str) -> Result<(), String> {
        let meta = backend
            .meta(path)
            .map_err(|e| format!("stat {path}: {e:#}"))?;
        match meta.kind {
            crate::services::files::PathKind::Directory => {
                return Err(format!("{path} is a directory"));
            }
            crate::services::files::PathKind::BinaryFile => {
                self.refusal = Some(format!("{path} looks binary — editing refused"));
                return Err(self.refusal.clone().unwrap());
            }
            crate::services::files::PathKind::TextFile => {}
        }
        if meta.size > MAX_FILE_BYTES {
            self.refusal = Some(format!(
                "{path} is {} bytes; the editor refuses files over {} bytes",
                meta.size, MAX_FILE_BYTES
            ));
            return Err(self.refusal.clone().unwrap());
        }
        let content = backend
            .read_file(path)
            .map_err(|e| format!("read {path}: {e:#}"))?;
        self.path = Some(path.to_string());
        self.saved = Some(content.clone());
        self.buffer = content;
        self.cursor = self.buffer.len();
        self.dirty = false;
        // Offsets from the previously open buffer are meaningless here.
        self.search_active = false;
        self.search_input.clear();
        self.last_match = None;
        self.refusal = None;
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
        // Step back one character (UTF-8 safe).
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

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    // Adopt content reloaded by a completed DiscardChanges action.
    if let Some((path, content)) = app.ui.reload_content.take() {
        if app.ui.editor_ui.path.as_deref() == Some(&path) {
            let ui = &mut app.ui.editor_ui;
            ui.buffer = content;
            ui.saved = Some(ui.buffer.clone());
            ui.dirty = false;
            ui.cursor = ui.cursor.min(ui.buffer.len());
        }
    }

    let ui = &mut app.ui.editor_ui;
    if ui.open_active {
        match key.code {
            KeyCode::Enter => {
                ui.open_active = false;
                let path = ui.open_input.trim().to_string();
                ui.open_input.clear();
                if path.is_empty() {
                    app.set_error("file path cannot be empty");
                } else if let Err(msg) = ui.load(&app.backend, &path) {
                    app.set_error(msg);
                }
            }
            KeyCode::Esc => {
                ui.open_active = false;
                ui.open_input.clear();
            }
            KeyCode::Backspace => {
                ui.open_input.pop();
            }
            KeyCode::Char(c) => ui.open_input.push(c),
            _ => {}
        }
        return;
    }

    // No file open: any key other than 'o' does nothing.
    if ui.path.is_none() {
        if key.code == KeyCode::Char('o') {
            ui.open_active = true;
        }
        return;
    }

    // Search prompt: '/' opens it, Enter jumps to the next match,
    // Esc cancels, typing edits the query. Live-updating the match
    // keeps the found offset in sync while the query evolves.
    if ui.search_active {
        match key.code {
            KeyCode::Enter => {
                jump_to_next_match(app);
            }
            KeyCode::Esc => {
                ui.search_active = false;
                ui.search_input.clear();
                ui.last_match = None;
            }
            KeyCode::Backspace => {
                ui.search_input.pop();
                ui.last_match = None;
            }
            KeyCode::Char(c) => {
                ui.search_input.push(c);
                ui.last_match = None;
            }
            _ => {}
        }
        return;
    }
    if key.code == KeyCode::Char('/') {
        ui.search_active = true;
        ui.search_input.clear();
        ui.last_match = None;
        return;
    }

    match key.code {
        KeyCode::Char('o')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            ui.open_active = true;
        }
        KeyCode::Left => ui.cursor = ui.cursor.saturating_sub(1),
        KeyCode::Right => ui.cursor = (ui.cursor + 1).min(ui.buffer.len()),
        KeyCode::Up => {
            let (line, _) = ui.line_col();
            if line > 1 {
                let target = line - 1;
                ui.cursor = line_start(&ui.buffer, target);
            }
        }
        KeyCode::Down => {
            let (line, _) = ui.line_col();
            let total = ui.buffer.matches('\n').count() + 1;
            if line < total {
                ui.cursor = line_start(&ui.buffer, line + 1);
            }
        }
        KeyCode::Home => ui.cursor = line_start(&ui.buffer, ui.line_col().0),
        KeyCode::End => {
            let line_end = ui.buffer[ui.cursor..]
                .find('\n')
                .map(|i| ui.cursor + i)
                .unwrap_or(ui.buffer.len());
            ui.cursor = line_end;
        }
        KeyCode::Backspace => ui.backspace(),
        KeyCode::Enter => ui.insert_char('\n'),
        KeyCode::Char('s')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            save(app);
        }
        KeyCode::Char('w')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            // Close buffer; confirmation is required when dirty.
            if ui.dirty {
                let path = ui.path.clone().unwrap_or_default();
                app.request_action(ActionKind::DiscardChanges(path));
            } else {
                ui.path = None;
                ui.saved = None;
                ui.buffer.clear();
                ui.cursor = 0;
                ui.search_active = false;
                ui.search_input.clear();
                ui.last_match = None;
            }
        }
        KeyCode::Char(c) => ui.insert_char(c),
        KeyCode::Tab => {
            for _ in 0..4 {
                ui.insert_char(' ');
            }
        }
        _ => {}
    }
}

/// Jumps the cursor to the next occurrence of the search query,
/// starting after the current match and wrapping around once. An empty
/// query or no-match is reported through the message bar, not a panic.
fn jump_to_next_match<B: WorkspaceBackend>(app: &mut App<B>) {
    let ui = &mut app.ui.editor_ui;
    let query = ui.search_input.clone();
    if query.is_empty() {
        app.set_error("search: empty query");
        return;
    }
    let from = ui.last_match.map_or(0, |m| m + 1);
    let buffer = ui.buffer.clone();
    let wrapped = buffer[from..].find(&query).map(|i| from + i).or_else(|| {
        if from > 0 {
            buffer[..from].find(&query)
        } else {
            None
        }
    });
    match wrapped {
        Some(offset) => {
            ui.cursor = offset;
            ui.last_match = Some(offset);
            app.set_message(format!("match at byte {offset}"));
        }
        None => {
            app.set_error(format!("search: '{query}' not found"));
        }
    }
}

fn save<B: WorkspaceBackend>(app: &mut App<B>) {
    let ui = &app.ui.editor_ui;
    let Some(path) = ui.path.clone() else {
        return;
    };
    let content = ui.buffer.clone();
    match app.backend.write_file(&path, &content) {
        Ok(true) => {
            let ui = &mut app.ui.editor_ui;
            ui.saved = Some(content);
            ui.dirty = false;
            app.set_message(format!("saved: {path}"));
        }
        Ok(false) => app.set_error("write refused by backend"),
        Err(e) => app.set_error(format!("save {path}: {e:#}")),
    }
}

/// Byte offset where the given 1-based line starts.
fn line_start(buffer: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut offset = 0;
    for _ in 1..line {
        match buffer[offset..].find('\n') {
            Some(i) => offset += i + 1,
            None => return offset,
        }
    }
    offset
}

pub fn draw<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: ratatui::widgets::Block<'_>,
) {
    let ui = &app.ui.editor_ui;
    if ui.open_active {
        frame.render_widget(
            Paragraph::new(format!("open file: {}", ui.open_input))
                .style(Style::default().fg(Color::Yellow))
                .block(block),
            area,
        );
        hint_line(frame, area, "[Enter] open  [Esc] cancel");
        return;
    }

    if ui.search_active {
        let match_note = ui
            .last_match
            .map(|m| format!("  at byte {m}"))
            .unwrap_or_default();
        frame.render_widget(
            Paragraph::new(format!("search: {}{match_note}", ui.search_input))
                .style(Style::default().fg(Color::Cyan))
                .block(block),
            area,
        );
        hint_line(frame, area, "[Enter] next match  [Esc] cancel");
        return;
    }

    let Some(path) = &ui.path else {
        let mut lines = vec![
            ratatui::text::Line::from("no file open"),
            ratatui::text::Line::from(""),
        ];
        if let Some(refusal) = &ui.refusal {
            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                refusal.clone(),
                Style::default().fg(Color::Red),
            )));
            lines.push(ratatui::text::Line::from(""));
        }
        lines.push(ratatui::text::Line::from(
            "press o to open a file by relative path",
        ));
        frame.render_widget(Paragraph::new(lines).block(block), area);
        hint_line(frame, area, "[o] open  [Esc] back");
        return;
    };

    let (line, col) = ui.line_col();
    let dirty = if ui.dirty { " *dirty*" } else { "" };
    let block = block.title(format!(" {path}{dirty} — {line}:{col} "));
    let mut para = Paragraph::new(ui.buffer.as_str())
        .block(block)
        .wrap(Wrap { trim: false });
    if ui.dirty {
        para = para.style(Style::default().fg(Color::LightYellow));
    } else {
        para = para.style(Style::default().fg(Color::Gray).add_modifier(Modifier::DIM));
    }
    frame.render_widget(para, area);
    hint_line(
        frame,
        area,
        "[C-s] save  [C-w] close  [C-o] open  / search  [chars] type  [arrows/Home/End] move",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::App;
    use crate::tui::backend::LocalBackend;

    fn app_with_file(content: &str) -> App<LocalBackend> {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        let app = App::new(LocalBackend::new(root));
        app.backend.write_file("doc.md", content).unwrap();
        app
    }

    fn press(app: &mut App<LocalBackend>, code: KeyCode) {
        super::handle_key(app, KeyEvent::from(code));
    }

    fn type_into(app: &mut App<LocalBackend>, s: &str) {
        for c in s.chars() {
            super::handle_key(app, KeyEvent::from(KeyCode::Char(c)));
        }
    }

    #[test]
    fn load_refuses_binary_file() {
        let mut app = app_with_file("text");
        // Write raw bytes so the file is genuinely non-UTF-8.
        let bin = app.backend.root().join("blob.bin");
        std::fs::write(&bin, b"a\x00b").unwrap();
        let ui = &mut app.ui.editor_ui;
        assert!(ui.load(&app.backend, "blob.bin").is_err());
        assert!(ui.refusal.as_deref().unwrap().contains("binary"));
        assert!(ui.path.is_none());
    }

    #[test]
    fn load_refuses_directories() {
        let mut app = app_with_file("text");
        app.backend.create_dir("folder").unwrap();
        let ui = &mut app.ui.editor_ui;
        assert!(ui.load(&app.backend, "folder").is_err());
    }

    #[test]
    fn editing_marks_dirty_and_save_clears_it() {
        let mut app = app_with_file("hello");
        app.ui.editor_ui.load(&app.backend, "doc.md").unwrap();
        app.ui.editor_ui.insert_char('!');
        assert!(app.ui.editor_ui.dirty);
        assert_eq!(app.ui.editor_ui.buffer, "hello!");

        let mut key = crossterm::event::KeyEvent::from(KeyCode::Char('s'));
        key.modifiers = crossterm::event::KeyModifiers::CONTROL;
        handle_key(&mut app, key);
        assert!(!app.ui.editor_ui.dirty);
        assert_eq!(app.backend.read_file("doc.md").unwrap(), "hello!");
    }

    #[test]
    fn close_dirty_buffer_requires_confirmation_then_reloads() {
        let mut app = app_with_file("base");
        app.ui.editor_ui.load(&app.backend, "doc.md").unwrap();
        app.ui.editor_ui.insert_char('X');

        let mut key = crossterm::event::KeyEvent::from(KeyCode::Char('w'));
        key.modifiers = crossterm::event::KeyModifiers::CONTROL;
        handle_key(&mut app, key);
        assert!(app.confirm.is_some());

        app.confirm_pending();
        handle_key(&mut app, crossterm::event::KeyEvent::from(KeyCode::Null));
        assert!(!app.ui.editor_ui.dirty);
        assert_eq!(app.ui.editor_ui.buffer, "base");
    }

    #[test]
    fn backspace_is_utf8_safe() {
        let mut app = app_with_file("");
        app.backend.write_file("u.txt", "").unwrap();
        app.ui.editor_ui.load(&app.backend, "u.txt").unwrap();
        let ui = &mut app.ui.editor_ui;
        for c in "h\u{e9}llo".chars() {
            ui.insert_char(c);
        }
        assert_eq!(ui.buffer, "h\u{e9}llo");
        // Cursor sits between the multibyte 'é' and the first 'l';
        // backspace must remove the whole character, not one byte.
        ui.cursor = 3;
        ui.backspace();
        assert_eq!(ui.buffer, "hllo");
    }

    #[test]
    fn line_col_and_line_start_agree() {
        let mut app = app_with_file("");
        app.backend
            .write_file("lines.txt", "one\ntwo\nthree")
            .unwrap();
        app.ui.editor_ui.load(&app.backend, "lines.txt").unwrap();
        let ui = &mut app.ui.editor_ui;
        // Load leaves the cursor at the end: line 3, col 6.
        assert_eq!(ui.line_col(), (3, 6));
        // Jump to the start of line 2 and verify the slice matches.
        ui.cursor = line_start(&ui.buffer, 2);
        assert_eq!(ui.line_col(), (2, 1));
        assert_eq!(&ui.buffer[ui.cursor..ui.cursor + 3], "two");
    }

    #[test]
    fn search_finds_next_match_and_wraps() {
        let mut app = app_with_file("alpha beta alpha gamma\nalpha end");
        app.ui.editor_ui.load(&app.backend, "doc.md").unwrap();
        press(&mut app, KeyCode::Char('/'));
        assert!(app.ui.editor_ui.search_active);
        type_into(&mut app, "alpha");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.ui.editor_ui.cursor, 0, "first match at byte 0");
        assert_eq!(app.ui.editor_ui.last_match, Some(0));

        press(&mut app, KeyCode::Enter);
        assert_eq!(app.ui.editor_ui.cursor, 11, "second match after 'beta '");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.ui.editor_ui.cursor, 23, "third match on line 2");
        // Wraps to the start on the next Enter.
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.ui.editor_ui.cursor, 0, "search wraps around");
    }

    #[test]
    fn search_missing_query_reports_error_and_esc_exits() {
        let mut app = app_with_file("hello world");
        app.ui.editor_ui.load(&app.backend, "doc.md").unwrap();
        press(&mut app, KeyCode::Char('/'));
        type_into(&mut app, "zzz");
        press(&mut app, KeyCode::Enter);
        assert!(app.error.is_some(), "no match must surface an error");

        press(&mut app, KeyCode::Esc);
        assert!(!app.ui.editor_ui.search_active);
        assert!(app.ui.editor_ui.search_input.is_empty());
        assert_eq!(app.ui.editor_ui.last_match, None);
        // Esc'd search must not capture the next '/'-less keypress —
        // typing goes back into the buffer.
        let before = app.ui.editor_ui.buffer.clone();
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.ui.editor_ui.buffer, format!("{before}x"));
    }

    #[test]
    fn search_empty_query_is_rejected() {
        let mut app = app_with_file("content");
        app.ui.editor_ui.load(&app.backend, "doc.md").unwrap();
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Enter);
        assert!(app.error.is_some(), "empty query must be refused");
    }

    #[test]
    fn opening_another_file_clears_search_state() {
        let mut app = app_with_file("alpha alpha");
        app.ui.editor_ui.load(&app.backend, "doc.md").unwrap();
        press(&mut app, KeyCode::Char('/'));
        type_into(&mut app, "alpha");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.ui.editor_ui.last_match, Some(0));

        app.backend.write_file("other.txt", "beta").unwrap();
        app.ui.editor_ui.load(&app.backend, "other.txt").unwrap();
        assert!(!app.ui.editor_ui.search_active);
        assert_eq!(app.ui.editor_ui.last_match, None);
    }
}
