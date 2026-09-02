//! Terminal screen: one-shot argv execution with bounded captured
//! output. There is no interactive shell — commands are tokenized
//! into argv and run through the backend's audited terminal service.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;

use super::hint_line;
use crate::tui::app::App;
use crate::tui::backend::WorkspaceBackend;

#[derive(Default)]
pub struct TerminalUi {
    /// Command line being composed (program + args, space separated).
    pub input: String,
    /// Bounded stdout/stderr from the last run.
    pub output: String,
    pub last_exit: Option<i32>,
    pub timed_out: bool,
    pub truncated: bool,
}

/// Splits an input line into argv tokens, honoring double quotes.
pub fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for c in line.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ' ' if !in_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    let ui = &mut app.ui.terminal_ui;
    match key.code {
        KeyCode::Enter => {
            let tokens = tokenize(&ui.input);
            ui.input.clear();
            let Some((program, args)) = tokens.split_first().map(|(p, a)| (p.clone(), a.to_vec()))
            else {
                return;
            };
            match app.backend.terminal_run(&program, &args) {
                Ok(outcome) => {
                    ui.timed_out = outcome.timed_out;
                    ui.truncated = outcome.truncated;
                    ui.last_exit = outcome.exit_code;
                    let mut text = String::new();
                    if !outcome.stdout.is_empty() {
                        text.push_str(&outcome.stdout);
                    }
                    if !outcome.stderr.is_empty() {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(&outcome.stderr);
                    }
                    if outcome.timed_out {
                        text.push_str("\n[command timed out]");
                    }
                    if outcome.truncated {
                        text.push_str("\n[output truncated]");
                    }
                    ui.output = text;
                }
                Err(e) => {
                    ui.output = format!("error: {e:#}");
                }
            }
        }
        KeyCode::Backspace => {
            ui.input.pop();
        }
        KeyCode::Char(c) => ui.input.push(c),
        _ => {}
    }
}

pub fn draw<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: ratatui::widgets::Block<'_>,
) {
    let ui = &app.ui.terminal_ui;
    let [main, input_line] = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Min(3),
        ratatui::layout::Constraint::Length(3),
    ])
    .areas(area);

    let mut text = ui.output.clone();
    if text.is_empty() {
        text = "no commands run yet".to_string();
    }
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(Color::Gray))
            .block(block),
        main,
    );

    let exit = match ui.last_exit {
        Some(0) => "ok".to_string(),
        Some(code) => format!("exit {code}"),
        None => "-".to_string(),
    };
    let input_block = ratatui::widgets::Block::bordered()
        .title(format!(" run — {exit} "))
        .title_style(Style::default().fg(Color::Cyan));
    frame.render_widget(
        Paragraph::new(format!("$ {}", ui.input)).block(input_block),
        input_line,
    );
    hint_line(
        frame,
        area,
        "[Enter] run  [Bksp] edit  no shell features (argv only)",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::App;
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
    fn tokenize_splits_on_spaces_and_honors_quotes() {
        assert_eq!(tokenize("echo hello world"), ["echo", "hello", "world"]);
        assert_eq!(tokenize("echo \"a b\" c"), ["echo", "a b", "c"]);
        assert!(tokenize("   ").is_empty());
        assert_eq!(tokenize("one"), ["one"]);
    }

    #[test]
    fn run_captures_stdout_and_exit_code() {
        let mut app = app();
        app.goto(ScreenId::Terminal);
        type_string(&mut app, "echo hi");
        press(&mut app, KeyCode::Enter);
        assert!(app.ui.terminal_ui.output.contains("hi"));
        assert_eq!(app.ui.terminal_ui.last_exit, Some(0));
    }

    #[test]
    fn run_reports_nonzero_exit_and_stderr() {
        let mut app = app();
        app.goto(ScreenId::Terminal);
        type_string(&mut app, "false");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.ui.terminal_ui.last_exit, Some(1));
    }

    #[test]
    fn missing_program_shows_error() {
        let mut app = app();
        app.goto(ScreenId::Terminal);
        type_string(&mut app, "definitely-not-a-real-program-xyz");
        press(&mut app, KeyCode::Enter);
        assert!(app.ui.terminal_ui.output.contains("error"));
    }

    #[test]
    fn empty_input_is_ignored() {
        let mut app = app();
        app.goto(ScreenId::Terminal);
        press(&mut app, KeyCode::Enter);
        assert!(app.ui.terminal_ui.output.is_empty());
        assert_eq!(app.ui.terminal_ui.last_exit, None);
    }
}
