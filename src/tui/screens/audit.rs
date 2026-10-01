//! Audit screen: newest-first chronological view over the CANONICAL
//! persistent audit store (the shared bounded ring serving both
//! `/api/v1/audit` and `/api/v1/logs`) — this screen deliberately has
//! NO separate TUI audit store. Filters (text + kind), a detail pane
//! with correlation ids, and bounded rendering.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};
use ratatui::Frame;

use super::hint_line;
use crate::tui::app::App;
use crate::tui::backend::{AuditRow, WorkspaceBackend};
use crate::tui::components::{render_state_placeholder, ViewState};
use crate::tui::theme::Role;

/// Rows rendered per draw (the ring holds 1000; the view shows a
/// bounded window).
const VIEW_LIMIT: usize = 200;

#[derive(Default)]
pub struct AuditUi {
    /// Free-text filter over action/subject/detail.
    pub filter: String,
    /// Kind filter (allow/deny) or None for all.
    pub kind_filter: Option<String>,
    pub filtering: bool,
}

pub fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    if app.ui.audit_ui.filtering {
        app.ui.capture_input = true;
        let ui = &mut app.ui.audit_ui;
        match key.code {
            KeyCode::Esc | KeyCode::Enter => ui.filtering = false,
            KeyCode::Backspace => {
                ui.filter.pop();
            }
            KeyCode::Char(c) => ui.filter.push(c),
            _ => {}
        }
        return;
    }
    app.ui.capture_input = false;

    // Snapshot the filter state so `cached_audit`'s app borrow doesn't
    // overlap the mutable one below.
    let filter_snapshot = AuditUi {
        filter: app.ui.audit_ui.filter.clone(),
        kind_filter: app.ui.audit_ui.kind_filter.clone(),
        filtering: false,
    };
    let count = app
        .cached_audit()
        .map(|rows| filtered_rows(rows, &filter_snapshot).len())
        .unwrap_or(0);

    match key.code {
        KeyCode::Char('/') => {
            app.ui.audit_ui.filtering = true;
            app.ui.capture_input = true;
        }
        KeyCode::Char('k') => {
            app.ui.audit_ui.kind_filter = match app.ui.audit_ui.kind_filter.as_deref() {
                Some("allow") => Some("deny".into()),
                Some("deny") => None,
                _ => Some("allow".into()),
            }
        }
        KeyCode::Up => {
            if let Some(sel) = app.ui.audit.selected() {
                if sel > 0 {
                    app.ui.audit.select(Some(sel - 1));
                }
            }
        }
        KeyCode::Down if count > 0 => {
            let sel = app.ui.audit.selected().unwrap_or(0);
            app.ui.audit.select(Some((sel + 1).min(count - 1)));
        }
        _ => {}
    }
}

fn filtered_rows(rows: &[AuditRow], ui: &AuditUi) -> Vec<AuditRow> {
    rows.iter()
        .filter(|r| ui.kind_filter.as_deref().is_none_or(|k| r.kind == k))
        .filter(|r| {
            ui.filter.is_empty()
                || r.action.to_lowercase().contains(&ui.filter.to_lowercase())
                || r.subject.to_lowercase().contains(&ui.filter.to_lowercase())
                || r.detail.to_lowercase().contains(&ui.filter.to_lowercase())
        })
        .cloned()
        .collect()
}

pub fn draw<B: WorkspaceBackend>(frame: &mut Frame, app: &mut App<B>, area: Rect) {
    let [list_area, detail_area, hint] = Layout::vertical([
        Constraint::Min(6),
        Constraint::Length(6),
        Constraint::Length(1),
    ])
    .areas(area);

    // Snapshot the small filter state + cached rows up front so the
    // mutable `app.ui.audit` below never conflicts.
    let ui_filter = app.ui.audit_ui.filter.clone();
    let ui_kind = app.ui.audit_ui.kind_filter.clone();
    let filter_ref = AuditUi {
        filter: ui_filter,
        kind_filter: ui_kind,
        filtering: false,
    };
    let rows = app.cached_audit().cloned();
    let selected_snapshot = app.ui.audit.selected().unwrap_or(0);

    match rows {
        Some(rows) => {
            let filtered = filtered_rows(&rows, &filter_ref);
            if filtered.is_empty() {
                let state = if rows.is_empty() {
                    ViewState::Empty {
                        hint: "no audit events recorded yet",
                    }
                } else {
                    ViewState::Empty {
                        hint: "no events match the active filter",
                    }
                };
                render_state_placeholder(frame, inner(list_area), &state);
                frame.render_widget(
                    Block::bordered().title(" audit · canonical ring "),
                    list_area,
                );
            } else {
                let selected = selected_snapshot.min(filtered.len() - 1);
                app.ui.audit.select(Some(selected));
                let sel_marker = selected;
                let items: Vec<ListItem> = filtered
                    .iter()
                    .enumerate()
                    .take(VIEW_LIMIT)
                    .map(|(i, r)| {
                        let role = match r.kind.as_str() {
                            "allow" => Role::Success,
                            "deny" => Role::Error,
                            _ => Role::Pending,
                        };
                        let marker = if i == sel_marker { "▶" } else { " " };
                        ListItem::new(Line::from(vec![
                            Span::styled(marker, Role::Focused.style()),
                            Span::styled(format!("{} ", role.symbol()), role.style()),
                            Span::styled(format!("{:<22}", r.action), Role::Normal.style()),
                            Span::styled(r.subject.clone(), Role::Muted.style()),
                        ]))
                    })
                    .collect();
                let title = match &filter_ref.kind_filter {
                    Some(k) => format!(" audit · {k} "),
                    None => " audit ".to_string(),
                };
                let list = List::new(items)
                    .block(Block::bordered().title(title))
                    .highlight_style(Role::Selected.style());
                frame.render_stateful_widget(list, list_area, &mut app.ui.audit);
            }

            // Detail pane: the selected event with correlation ids.
            let detail = filtered_rows(&rows, &filter_ref)
                .get(selected_snapshot)
                .cloned();
            let detail_lines: Vec<Line> = match detail {
                Some(r) => {
                    let event_id = r.event_id.clone().unwrap_or_else(|| "—".into());
                    let correlation = correlation_summary(&r);
                    vec![
                        Line::from(vec![
                            Span::styled("action    ", Role::Muted.style()),
                            Span::raw(r.action),
                        ]),
                        Line::from(vec![
                            Span::styled("subject   ", Role::Muted.style()),
                            Span::raw(r.subject),
                        ]),
                        Line::from(vec![
                            Span::styled("detail    ", Role::Muted.style()),
                            Span::raw(r.detail),
                        ]),
                        Line::from(vec![
                            Span::styled("event_id  ", Role::Muted.style()),
                            Span::raw(event_id),
                        ]),
                        Line::from(vec![
                            Span::styled("correlates", Role::Muted.style()),
                            Span::raw(correlation),
                        ]),
                    ]
                }
                None => vec![Line::from(Span::styled(
                    "select an event",
                    Role::Muted.style(),
                ))],
            };
            frame.render_widget(
                Paragraph::new(detail_lines).block(Block::bordered().title(" detail ")),
                detail_area,
            );
        }
        None => {
            render_state_placeholder(
                frame,
                inner(list_area),
                &ViewState::Unavailable {
                    reason: "audit store not reachable on this backend",
                },
            );
            frame.render_widget(Block::bordered().title(" audit "), list_area);
        }
    }

    if app.ui.audit_ui.filtering {
        hint_line(
            frame,
            hint,
            &format!("filter: {}_  [Enter/Esc] done", app.ui.audit_ui.filter),
        );
    } else {
        hint_line(
            frame,
            hint,
            "[/] filter  [k] cycle allow/deny/all  [↑/↓] select  [r] refresh",
        );
    }
}

fn correlation_summary(r: &AuditRow) -> String {
    let mut parts = Vec::new();
    if let Some(ws) = &r.workspace_id {
        parts.push(format!("ws={ws}"));
    }
    if let Some(agent) = &r.agent_id {
        parts.push(format!("agent={agent}"));
    }
    if let Some(session) = &r.session_id {
        parts.push(format!("sess={session}"));
    }
    if parts.is_empty() {
        "—".into()
    } else {
        parts.join(" ")
    }
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

    fn app() -> App<LocalBackend> {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        App::new(LocalBackend::new(root))
    }

    fn press(app: &mut App<LocalBackend>, code: KeyCode) {
        handle_key(app, KeyEvent::from(code));
    }

    #[test]
    fn audit_reads_the_canonical_ring_not_a_tui_store() {
        let mut app = app();
        // Record through the canonical service…
        crate::services::audit::record_allow("probe_audit_screen", "subject-a", "detail-a");
        crate::services::audit::record_deny("probe_audit_deny", "subject-b", "denied!");
        app.goto(ScreenId::Audit);
        let rows = app.cached_audit().cloned().unwrap();
        assert!(rows.iter().any(|r| r.action == "probe_audit_screen"));
        assert!(rows
            .iter()
            .any(|r| r.action == "probe_audit_deny" && r.kind == "deny"));
    }

    #[test]
    fn kind_filter_cycles_allow_deny_all() {
        let mut app = app();
        crate::services::audit::record_allow("f_allow", "s", "d");
        crate::services::audit::record_deny("f_deny", "s", "d");
        app.goto(ScreenId::Audit);
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.ui.audit_ui.kind_filter.as_deref(), Some("allow"));
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.ui.audit_ui.kind_filter.as_deref(), Some("deny"));
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.ui.audit_ui.kind_filter, None);
    }

    #[test]
    fn text_filter_narrows_to_matching_actions() {
        let mut app = app();
        crate::services::audit::record_allow("needle_action", "s", "d");
        crate::services::audit::record_allow("haystack_action", "s", "d");
        app.goto(ScreenId::Audit);
        press(&mut app, KeyCode::Char('/'));
        for c in "needle".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        let rows = app.cached_audit().cloned().unwrap();
        let filtered = filtered_rows(&rows, &app.ui.audit_ui);
        assert!(filtered.iter().all(|r| r.action.contains("needle")));
        assert!(filtered.len() < rows.len());
    }
}
