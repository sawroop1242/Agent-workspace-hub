//! Changes screen: the review surface. Staged and unstaged file lists
//! parsed from `git status --porcelain`, a diff viewer with line
//! numbers and horizontal scrolling, and stage/unstage/commit actions
//! routed through the tracked-operation path. No git subprocess runs
//! outside the backend.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};
use ratatui::Frame;

use super::hint_line;
use crate::tui::app::App;
use crate::tui::backend::WorkspaceBackend;
use crate::tui::components::render_state_placeholder;
use crate::tui::components::{parse_diff, render_diff, DiffScroll, ViewState};
use crate::tui::theme::Role;

/// Which side of the staged/unstaged split is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangesPane {
    Files,
    Diff,
}

#[derive(Default)]
pub struct ChangesUi {
    pub pane: ChangesPaneDefault,
    /// 0 = unstaged, 1 = staged list.
    pub side: usize,
    pub commit_input: Option<String>,
    pub diff_scroll: DiffScrollDefault,
}

// Small wrappers so `Default` works without trait imports in
// ScreenState.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChangesPaneDefault {
    pub files: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DiffScrollDefault {
    pub horizontal: usize,
    pub vertical: usize,
}

impl ChangesUi {
    fn files_focused(&self) -> bool {
        !self.pane.files
    }
}

/// Max diff lines rendered per load (bounded view; the backend's
/// output cap already bounds the source).
const MAX_DIFF_LINES: usize = 2000;

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    // Commit-message input captures everything.
    if app.ui.changes_ui.commit_input.is_some() {
        app.ui.capture_input = true;
        match key.code {
            KeyCode::Esc => app.ui.changes_ui.commit_input = None,
            KeyCode::Backspace => {
                if let Some(input) = app.ui.changes_ui.commit_input.as_mut() {
                    input.pop();
                }
            }
            KeyCode::Enter => {
                let message = app
                    .ui
                    .changes_ui
                    .commit_input
                    .take()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if !message.is_empty() {
                    app.ops.track(format!("commit: {message}"));
                    app.ops.to_executing();
                    match app.backend.git_commit(&message) {
                        Ok(_) => {
                            app.ops.to_confirmed();
                            app.invalidate_views();
                            app.set_message(format!("committed: {message}"));
                        }
                        Err(e) => {
                            app.ops.to_failed(format!("{e:#}"));
                            app.set_error(format!("commit: {e:#}"));
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                if let Some(input) = app.ui.changes_ui.commit_input.as_mut() {
                    input.push(c);
                }
            }
            _ => {}
        }
        return;
    }
    app.ui.capture_input = false;

    let files_focused = app.ui.changes_ui.files_focused();
    // Snapshot the rows/side before mutating selection state.
    let changes_snapshot = app.cached_changes().cloned().unwrap_or_default();
    let side = app.ui.changes_ui.side;

    match key.code {
        KeyCode::Tab => {
            // Toggle between file list and diff pane.
            app.ui.changes_ui.pane.files = files_focused;
        }
        KeyCode::Char('1') => app.ui.changes_ui.side = 0,
        KeyCode::Char('2') => app.ui.changes_ui.side = 1,
        KeyCode::Left if !files_focused => {
            app.ui.changes_ui.diff_scroll.horizontal =
                app.ui.changes_ui.diff_scroll.horizontal.saturating_sub(4);
        }
        KeyCode::Right if !files_focused => {
            app.ui.changes_ui.diff_scroll.horizontal =
                app.ui.changes_ui.diff_scroll.horizontal.saturating_add(4);
        }
        KeyCode::Up if files_focused => {
            let len = side_len(&changes_snapshot, side);
            if len > 0 {
                let sel = app.ui.changes.selected().unwrap_or(0);
                app.ui
                    .changes
                    .select(Some(sel.saturating_sub(1).min(len - 1)));
            }
        }
        KeyCode::Down if files_focused => {
            let len = side_len(&changes_snapshot, side);
            if len > 0 {
                let sel = app.ui.changes.selected().unwrap_or(0);
                app.ui.changes.select(Some((sel + 1).min(len - 1)));
            }
        }
        KeyCode::Char(' ') if files_focused => {
            let rows = side_rows(&changes_snapshot, side);
            if let Some(path) = rows.get(app.ui.changes.selected().unwrap_or(0)) {
                let path = path.clone();
                let (label, result) = if side == 0 {
                    ("stage", app.backend.git_stage(&path))
                } else {
                    ("unstage", app.backend.git_unstage(&path))
                };
                app.ops.track(format!("{label} {path}"));
                app.ops.to_executing();
                match result {
                    Ok(_) => {
                        app.ops.to_confirmed();
                        app.invalidate_views();
                        app.set_message(format!("done: {label} {path}"));
                    }
                    Err(e) => {
                        app.ops.to_failed(format!("{e:#}"));
                        app.set_error(format!("{label} {path}: {e:#}"));
                    }
                }
            }
        }
        KeyCode::Char('c') => {
            // Only meaningful with something staged.
            app.ui.changes_ui.commit_input = Some(String::new());
            app.ui.capture_input = true;
        }
        _ => {}
    }
}

fn side_len(changes: &[crate::tui::backend::ChangedFile], side: usize) -> usize {
    side_rows(changes, side).len()
}

fn side_rows(changes: &[crate::tui::backend::ChangedFile], side: usize) -> Vec<String> {
    changes
        .iter()
        .filter(|c| if side == 0 { c.unstaged } else { c.staged })
        .map(|c| c.path.clone())
        .collect()
}

#[allow(dead_code)]
fn _side_len_alias(changes: &[crate::tui::backend::ChangedFile], side: usize) -> usize {
    side_len(changes, side)
}

pub fn draw<B: WorkspaceBackend>(frame: &mut Frame, app: &mut App<B>, area: Rect) {
    // Commit input overlay takes the full screen area as a dialog.
    if let Some(input) = &app.ui.changes_ui.commit_input {
        let line = Line::from(vec![
            Span::styled("commit message: ", Role::Focused.style()),
            Span::raw(input.clone()),
            Span::styled("_", Role::Muted.style()),
        ]);
        frame.render_widget(
            Paragraph::new(line).block(Block::bordered().title(" commit ")),
            area,
        );
        return;
    }

    let [content, hint] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let [files_pane, diff_pane] =
        Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)]).areas(content);

    let changes_opt = app.cached_changes().cloned();
    let side = app.ui.changes_ui.side;
    match changes_opt {
        Some(changes) => {
            let rows = side_rows(&changes, side);
            let title = match side {
                0 => " unstaged ",
                _ => " staged ",
            };
            if rows.is_empty() {
                render_state_placeholder(
                    frame,
                    inner(files_pane),
                    &ViewState::Empty {
                        hint: if side == 0 {
                            "working tree clean"
                        } else {
                            "nothing staged"
                        },
                    },
                );
                frame.render_widget(Block::bordered().title(title), files_pane);
            } else {
                let items: Vec<ListItem> = rows
                    .iter()
                    .map(|p| ListItem::new(Line::from(Span::raw(p.clone()))))
                    .collect();
                let list = List::new(items)
                    .block(Block::bordered().title(title))
                    .highlight_style(Role::Selected.style());
                frame.render_stateful_widget(list, files_pane, &mut app.ui.changes);
            }

            // Diff pane: show the selected path's diff for the focused
            // side.
            let selected_path = rows.get(app.ui.changes.selected().unwrap_or(0)).cloned();
            match selected_path {
                Some(path) => {
                    let staged = side == 1;
                    let output = app
                        .backend
                        .git_diff(staged, Some(&path))
                        .map(|o| o.stdout)
                        .unwrap_or_default();
                    let (lines, truncated) = parse_diff(&output, MAX_DIFF_LINES);
                    let scroll = DiffScroll {
                        horizontal: app.ui.changes_ui.diff_scroll.horizontal,
                        vertical: app.ui.changes_ui.diff_scroll.vertical,
                    };
                    let inner_diff = inner(diff_pane);
                    frame.render_widget(
                        Block::bordered().title(format!(" diff · {path} ")),
                        diff_pane,
                    );
                    render_diff(frame, inner_diff, &lines, truncated, scroll);
                }
                None => {
                    render_state_placeholder(
                        frame,
                        inner(diff_pane),
                        &ViewState::Empty {
                            hint: "select a file to view its diff",
                        },
                    );
                    frame.render_widget(Block::bordered().title(" diff "), diff_pane);
                }
            }
        }
        None => {
            render_state_placeholder(
                frame,
                inner(files_pane),
                &ViewState::Unavailable {
                    reason: "git status not available on this backend",
                },
            );
            frame.render_widget(Block::bordered().title(" unstaged "), files_pane);
            frame.render_widget(Block::bordered().title(" diff "), diff_pane);
        }
    }

    hint_line(
        frame,
        hint,
        "[1] unstaged [2] staged [Tab] files/diff [Space] stage/unstage [c] commit [←/→] scroll diff",
    );
}

fn inner(area: Rect) -> Rect {
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

    fn git_app() -> App<LocalBackend> {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        let app = App::new(LocalBackend::new(root));
        // A real git repo with one modified + one staged file.
        let backend_root = app.backend.root().to_path_buf();
        let _ = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&backend_root)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "tui@test"])
            .current_dir(&backend_root)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "TUI Test"])
            .current_dir(&backend_root)
            .status();
        std::fs::write(backend_root.join("a.txt"), "one\ntwo\n").unwrap();
        let _ = std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&backend_root)
            .status();
        let _ = std::process::Command::new("git")
            .args(["commit", "-q", "-m", "init"])
            .current_dir(&backend_root)
            .status();
        std::fs::write(backend_root.join("a.txt"), "one\nTWO\nthree\n").unwrap();
        std::fs::write(backend_root.join("b.txt"), "new file\n").unwrap();
        let _ = std::process::Command::new("git")
            .args(["add", "b.txt"])
            .current_dir(&backend_root)
            .status();
        app
    }

    fn press(app: &mut App<LocalBackend>, code: KeyCode) {
        handle_key(app, KeyEvent::from(code));
    }

    #[test]
    fn changes_split_staged_from_unstaged() {
        let mut app = git_app();
        app.goto(ScreenId::Changes);
        let changes = app.cached_changes().cloned().unwrap();
        assert_eq!(changes.len(), 2);
        let unstaged = side_rows(&changes, 0);
        let staged = side_rows(&changes, 1);
        assert!(unstaged.contains(&"a.txt".to_string()));
        assert!(staged.contains(&"b.txt".to_string()));
    }

    #[test]
    fn space_stages_the_selected_unstaged_file() {
        let mut app = git_app();
        app.goto(ScreenId::Changes);
        app.ui.changes.select(Some(0));
        press(&mut app, KeyCode::Char(' '));
        let changes = app.cached_changes().cloned().unwrap();
        let unstaged = side_rows(&changes, 0);
        assert!(
            !unstaged.contains(&"a.txt".to_string()),
            "a.txt should be staged after Space"
        );
    }

    #[test]
    fn commit_input_collects_and_commits() {
        let mut app = git_app();
        app.goto(ScreenId::Changes);
        // Stage everything first (a.txt stays unstaged).
        app.ui.changes.select(Some(0));
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Char('c'));
        assert!(app.ui.changes_ui.commit_input.is_some());
        for c in "test commit".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        assert!(app.ui.capture_input);
        press(&mut app, KeyCode::Enter);
        assert!(app.ui.changes_ui.commit_input.is_none());
        let commits = app.backend.recent_commits(5).unwrap();
        assert!(commits.iter().any(|c| c.subject == "test commit"));
    }

    #[test]
    fn tab_toggles_between_files_and_diff_panes() {
        let mut app = git_app();
        app.goto(ScreenId::Changes);
        assert!(app.ui.changes_ui.files_focused());
        press(&mut app, KeyCode::Tab);
        assert!(!app.ui.changes_ui.files_focused());
        press(&mut app, KeyCode::Tab);
        assert!(app.ui.changes_ui.files_focused());
    }
}
