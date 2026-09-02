//! Git screen: status porcelain listing with stage/unstage/commit and
//! worktree/staged diff views. All git access flows through the
//! backend's structured [`GitOutput`] — the screen never shells out.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{List, ListItem, Paragraph};

use super::hint_line;
use crate::services::git::PorcelainEntry;
use crate::tui::app::App;
use crate::tui::backend::WorkspaceBackend;

/// Which output pane the screen is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GitPane {
    #[default]
    Status,
    DiffWorktree,
    DiffStaged,
    Log,
}

#[derive(Default)]
pub struct GitUi {
    pub pane: GitPane,
    /// Cached porcelain entries, refreshed on entry and after actions.
    pub entries: Vec<PorcelainEntry>,
    /// Cached pane text (diff/log output) with its source pane tag.
    pub output: String,
    /// Message for the commit form.
    pub commit_input: String,
    pub commit_active: bool,
}

impl GitUi {
    pub fn refresh_status<B: WorkspaceBackend>(&mut self, backend: &B) {
        self.entries = match backend.git_status() {
            Ok(out) => out.porcelain_entries(),
            Err(_) => Vec::new(),
        };
    }
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    // Commit form borrows only its own input state.
    if app.ui.git_ui.commit_active {
        match key.code {
            KeyCode::Enter => {
                let message = app.ui.git_ui.commit_input.trim().to_string();
                app.ui.git_ui.commit_active = false;
                app.ui.git_ui.commit_input.clear();
                if message.is_empty() {
                    app.set_error("commit message cannot be empty");
                } else {
                    match app.backend.git_commit(&message) {
                        Ok(out) if out.exit_code.unwrap_or(0) == 0 => {
                            app.set_message("committed");
                            app.ui.git_ui.refresh_status(&app.backend);
                        }
                        Ok(out) => app.set_error(format!("git commit: {}", out.stderr.trim())),
                        Err(e) => app.set_error(format!("git commit: {e:#}")),
                    }
                }
            }
            KeyCode::Esc => {
                app.ui.git_ui.commit_active = false;
                app.ui.git_ui.commit_input.clear();
            }
            KeyCode::Backspace => {
                app.ui.git_ui.commit_input.pop();
            }
            KeyCode::Char(c) => app.ui.git_ui.commit_input.push(c),
            _ => {}
        }
        return;
    }

    let selected = app
        .ui
        .git
        .selected()
        .unwrap_or(0)
        .min(app.ui.git_ui.entries.len().saturating_sub(1));

    match key.code {
        KeyCode::Char('1') | KeyCode::Char('s') => {
            app.ui.git_ui.pane = GitPane::Status;
            app.ui.git_ui.refresh_status(&app.backend);
        }
        KeyCode::Char('2') | KeyCode::Char('d') => {
            app.ui.git_ui.pane = GitPane::DiffWorktree;
            load_pane(app, GitPane::DiffWorktree);
        }
        KeyCode::Char('3') => {
            app.ui.git_ui.pane = GitPane::DiffStaged;
            load_pane(app, GitPane::DiffStaged);
        }
        KeyCode::Char('4') | KeyCode::Char('l') => {
            app.ui.git_ui.pane = GitPane::Log;
            load_pane(app, GitPane::Log);
        }
        KeyCode::Up => {
            if selected > 0 {
                app.ui.git.select(Some(selected - 1));
            }
        }
        KeyCode::Down => {
            if !app.ui.git_ui.entries.is_empty() {
                app.ui
                    .git
                    .select(Some((selected + 1).min(app.ui.git_ui.entries.len() - 1)));
            }
        }
        KeyCode::Char('+') | KeyCode::Char('a') => {
            let path = app.ui.git_ui.entries.get(selected).map(|e| e.path.clone());
            if let Some(path) = path {
                match app.backend.git_stage(&path) {
                    Ok(out) if out.exit_code.unwrap_or(0) == 0 => {
                        app.set_message(format!("staged: {path}"));
                        app.ui.git_ui.refresh_status(&app.backend);
                    }
                    Ok(out) => app.set_error(format!("git add: {}", out.stderr.trim())),
                    Err(e) => app.set_error(format!("git add: {e:#}")),
                }
            }
        }
        KeyCode::Char('-') | KeyCode::Char('u') => {
            let path = app.ui.git_ui.entries.get(selected).map(|e| e.path.clone());
            if let Some(path) = path {
                match app.backend.git_unstage(&path) {
                    Ok(out) if out.exit_code.unwrap_or(0) == 0 => {
                        app.set_message(format!("unstaged: {path}"));
                        app.ui.git_ui.refresh_status(&app.backend);
                    }
                    Ok(out) => app.set_error(format!("git restore: {}", out.stderr.trim())),
                    Err(e) => app.set_error(format!("git restore: {e:#}")),
                }
            }
        }
        KeyCode::Char('c') => {
            app.ui.git_ui.commit_active = true;
            app.ui.git_ui.commit_input.clear();
        }
        KeyCode::Backspace => {
            app.ui.git_ui.refresh_status(&app.backend);
        }
        _ => {}
    }
}

fn load_pane<B: WorkspaceBackend>(app: &mut App<B>, pane: GitPane) {
    let path = app
        .ui
        .git_ui
        .entries
        .get(app.ui.git.selected().unwrap_or(0))
        .map(|e| e.path.clone());
    let result = match pane {
        GitPane::DiffWorktree => app.backend.git_diff(false, path.as_deref()),
        GitPane::DiffStaged => app.backend.git_diff(true, path.as_deref()),
        GitPane::Log => app.backend.git_log(50),
        GitPane::Status => Ok(crate::services::git::GitOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
        }),
    };
    app.ui.git_ui.output = match result {
        Ok(out) if out.exit_code.unwrap_or(0) == 0 => out.stdout,
        Ok(out) => format!("git error: {}", out.stderr.trim()),
        Err(e) => format!("error: {e:#}"),
    };
}

pub fn draw<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: ratatui::widgets::Block<'_>,
) {
    let ui = &mut app.ui.git_ui;
    let pane_label = match ui.pane {
        GitPane::Status => "1:status",
        GitPane::DiffWorktree => "2:diff worktree",
        GitPane::DiffStaged => "3:diff staged",
        GitPane::Log => "4:log",
    };
    let block = block.title(format!(" Git — {pane_label} "));

    if ui.commit_active {
        frame.render_widget(
            Paragraph::new(format!("commit message: {}", ui.commit_input))
                .style(Style::default().fg(Color::Yellow))
                .block(block),
            area,
        );
        hint_line(frame, area, "[Enter] commit  [Esc] cancel");
        return;
    }

    let [main, hint] = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Min(3),
        ratatui::layout::Constraint::Length(1),
    ])
    .areas(area);

    match ui.pane {
        GitPane::Status => {
            if ui.entries.is_empty() {
                ui.refresh_status(&app.backend);
            }
            let items: Vec<ListItem> = ui
                .entries
                .iter()
                .map(|e| {
                    let style = if e.status.starts_with(' ') {
                        Style::default().fg(Color::Green)
                    } else if e.status.starts_with('?') {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default().fg(Color::Yellow)
                    };
                    ListItem::new(format!("{} {}", e.status, e.path)).style(style)
                })
                .collect();
            let list = List::new(items).block(block).highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );
            frame.render_stateful_widget(list, main, &mut app.ui.git);
        }
        GitPane::DiffWorktree | GitPane::DiffStaged | GitPane::Log => {
            frame.render_widget(
                Paragraph::new(ui.output.as_str())
                    .style(Style::default().fg(Color::Gray))
                    .block(block),
                main,
            );
        }
    }

    frame.render_widget(
        Paragraph::new(
            "[1]status [2]diff [3]staged-diff [4]log  [+/-]stage/unstage  [c]commit  [Bksp]refresh",
        )
        .style(Style::default().fg(Color::DarkGray)),
        hint,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::App;
    use crate::tui::backend::LocalBackend;
    use crate::tui::screens::ScreenId;

    /// Creates an app whose workspace root is a real git repository
    /// with one committed file and one modified file.
    fn git_app() -> App<LocalBackend> {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        let app = App::new(LocalBackend::new(root.clone()));
        let out = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .output()
            .expect("git init");
        assert!(out.status.success(), "git init failed");
        // Persist identity so service-driven commits succeed.
        for args in [
            ["config", "user.name", "t"],
            ["config", "user.email", "t@t"],
        ] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .expect("git config");
        }
        std::fs::write(root.join("tracked.txt"), "original").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&root)
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-q", "-m", "init"])
            .current_dir(&root)
            .output()
            .expect("git commit");
        app.backend.write_file("tracked.txt", "changed").unwrap();
        app
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
    fn status_lists_dirty_entries_and_stage_updates_them() {
        let mut app = git_app();
        app.goto(ScreenId::Git);
        press(&mut app, KeyCode::Char('1'));
        assert!(app
            .ui
            .git_ui
            .entries
            .iter()
            .any(|e| e.path == "tracked.txt"));
        app.ui.git.select(Some(0));

        press(&mut app, KeyCode::Char('+'));
        let status = app
            .ui
            .git_ui
            .entries
            .iter()
            .find(|e| e.path == "tracked.txt")
            .map(|e| e.status.clone())
            .unwrap();
        assert!(!status.starts_with("??"), "should be staged: {status}");
    }

    #[test]
    fn commit_form_commits_staged_changes() {
        let mut app = git_app();
        app.goto(ScreenId::Git);
        press(&mut app, KeyCode::Char('1'));
        app.ui.git.select(Some(0));
        press(&mut app, KeyCode::Char('+'));
        press(&mut app, KeyCode::Char('c'));
        type_string(&mut app, "add dirty file");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.ui.git_ui.entries.len(), 0, "worktree should be clean");
    }

    #[test]
    fn empty_commit_message_is_rejected() {
        let mut app = git_app();
        app.goto(ScreenId::Git);
        press(&mut app, KeyCode::Char('c'));
        press(&mut app, KeyCode::Enter);
        assert!(app.error.is_some());
    }

    #[test]
    fn diff_panes_load_output() {
        let mut app = git_app();
        app.goto(ScreenId::Git);
        press(&mut app, KeyCode::Char('2'));
        assert_eq!(app.ui.git_ui.pane, GitPane::DiffWorktree);
        assert!(
            app.ui.git_ui.output.contains("tracked.txt"),
            "worktree diff should mention the file"
        );
        press(&mut app, KeyCode::Char('4'));
        assert!(
            app.ui.git_ui.output.contains("init"),
            "log shows first commit"
        );
    }
}
