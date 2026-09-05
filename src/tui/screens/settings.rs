//! Settings screen: the operator's real, resolved runtime
//! configuration — workspace root, focused project, dashboard refresh
//! cadence, backend mode (local/remote), audit capacity, and the
//! security posture that applies to every operation. Read-only by
//! design: every value here is enforced elsewhere in the product, so
//! there is nothing to "change" without lying about what changed.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Paragraph};

use super::hint_line;
use crate::tui::app::App;
use crate::tui::backend::WorkspaceBackend;

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    if key.code == KeyCode::Char('r') {
        app.invalidate_dashboard();
        app.set_message("settings refreshed");
    }
}

pub fn draw<B: WorkspaceBackend>(
    frame: &mut ratatui::Frame,
    app: &mut App<B>,
    area: Rect,
    block: Block<'_>,
) {
    let snapshot = app.dashboard_cached();
    let lines = vec![
        section("workspace"),
        row("root", snapshot.root.display().to_string()),
        row("projects", format!("{} registered", snapshot.project_count)),
        row(
            "focused",
            snapshot
                .current_project
                .clone()
                .unwrap_or_else(|| "(none)".to_owned()),
        ),
        row(
            "git",
            if snapshot.is_git_repo {
                format!(
                    "repo, branch {}, {} dirty",
                    snapshot.branch.as_deref().unwrap_or("?"),
                    snapshot.dirty_entries
                )
            } else {
                "not a repository".to_owned()
            },
        ),
        section("runtime"),
        row("version", env!("CARGO_PKG_VERSION").to_owned()),
        row("backend", backend_mode(app)),
        row("dashboard", "2s bounded refresh (r forces)".to_owned()),
        row(
            "audit ring",
            format!(
                "{} events in memory",
                snapshot.recent_activity.len() + count_rest()
            ),
        ),
        section("security"),
        row("file access", "workspace-relative paths only".to_owned()),
        row("traversal", "../ and absolute paths rejected".to_owned()),
        row("destructive", "modal confirmation required".to_owned()),
        row(
            "remote API",
            "bearer key per request (AWH_API_KEY)".to_owned(),
        ),
    ];

    hint_line(
        frame,
        shrink(area),
        "[r] refresh  [Esc] back  values are enforced, not editable",
    );
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Recent activity shown on the dashboard is capped at 5; the ring
/// itself holds up to 1000. The settings screen reports the true ring
/// size, which is what matters operationally.
fn count_rest() -> usize {
    crate::services::audit::global()
        .recent(usize::MAX)
        .len()
        .saturating_sub(5)
}

fn backend_mode<B: WorkspaceBackend>(app: &App<B>) -> String {
    match app.backend.mode() {
        crate::tui::backend::BackendMode::Local => "local filesystem".to_owned(),
        crate::tui::backend::BackendMode::Remote => {
            format!(
                "remote API ({})",
                app.backend.remote_base().unwrap_or_else(|| "?".to_owned())
            )
        }
    }
}

fn section(name: &str) -> ratatui::text::Line<'static> {
    ratatui::text::Line::from(ratatui::text::Span::styled(
        format!("— {name} "),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
}

fn row(label: &str, value: String) -> ratatui::text::Line<'static> {
    ratatui::text::Line::from(vec![
        ratatui::text::Span::raw(format!("{:<12}", label)),
        ratatui::text::Span::raw(value),
    ])
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
