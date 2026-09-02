//! Logs screen: newest-first view over the shared audit ring
//! (`services::audit::global()`), the same store that serves
//! `/api/v1/logs` and `/api/v1/audit`. Entries carry identifiers
//! only — never secret values (spec sections 16, 25).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Paragraph};

use super::hint_line;
use crate::tui::app::App;
use crate::tui::backend::WorkspaceBackend;

/// How many ring entries the screen keeps in view.
const VIEW_LIMIT: usize = 50;

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    match key.code {
        KeyCode::Char('r') | KeyCode::Enter => refresh(app),
        _ => {}
    }
}

/// Pulls the newest entries from the shared audit ring.
pub fn refresh<B: WorkspaceBackend>(app: &mut App<B>) {
    app.ui.logs = crate::services::audit::global().recent(VIEW_LIMIT);
}

pub fn draw<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: Block<'_>,
) {
    if app.ui.logs.is_empty() {
        refresh(app);
    }

    let mut lines = vec![ratatui::text::Line::from(ratatui::text::Span::styled(
        "kind       action              subject  detail",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))];
    if app.ui.logs.is_empty() {
        lines.push(ratatui::text::Line::from(
            "no audit events recorded this session",
        ));
        lines.push(ratatui::text::Line::from(
            "entries appear as auth, terminal, and git actions occur",
        ));
    }
    for entry in &app.ui.logs {
        let kind = ratatui::text::Span::styled(
            format!("{:<10} ", entry.kind),
            Style::default().fg(if entry.kind == "deny" {
                Color::Red
            } else {
                Color::Green
            }),
        );
        lines.push(ratatui::text::Line::from(vec![
            kind,
            ratatui::text::Span::raw(format!("{:<19} ", truncate(&entry.action, 19))),
            ratatui::text::Span::raw(format!("{:<8} ", truncate(&entry.subject, 8))),
            ratatui::text::Span::raw(truncate(&entry.detail, 40)),
        ]));
    }

    hint_line(frame, shrink(area), "[r] refresh  [Esc] back  newest first");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{cut}\u{2026}")
    }
}

/// Inner area one cell inside the border, for the hint line.
fn shrink(area: Rect) -> Rect {
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
    use crate::tui::app::App;
    use crate::tui::backend::LocalBackend;
    use crate::tui::screens::ScreenId;

    fn app() -> App<LocalBackend> {
        let tmp = tempfile::tempdir().unwrap();
        App::new(LocalBackend::new(tmp.path().to_path_buf()))
    }

    fn press(app: &mut App<LocalBackend>, code: KeyCode) {
        handle_key(app, KeyEvent::from(code));
    }

    #[test]
    fn refresh_key_pulls_ring_entries() {
        crate::services::audit::record_deny("probe_logs", "screen_test", "test");
        let mut app = app();
        app.goto(ScreenId::Logs);
        assert!(app.ui.logs.is_empty());
        press(&mut app, KeyCode::Char('r'));
        assert!(app.ui.logs.iter().any(|e| e.action == "probe_logs"));
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(truncate("abcdef", 3), "ab\u{2026}");
        assert_eq!(truncate("héllo", 5), "héllo");
    }
}
