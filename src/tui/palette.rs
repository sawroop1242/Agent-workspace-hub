//! Command palette (Ctrl+K).
//!
//! First-class fuzzy-searchable command surface. Commands expose name,
//! category, shortcut, enabled state, and a disabled reason; execution
//! routes through the same app handlers as normal UI actions — the
//! palette never bypasses the backend boundary or confirmation modals.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use super::app::{ActionKind, App};
use super::backend::WorkspaceBackend;
use super::screens::ScreenId;
use super::shell::{PRIMARY_SECTIONS, SECONDARY_SECTIONS};
use super::theme::Role;

/// What a palette command does when run. Variants carry exactly the
/// data the corresponding UI path needs — no behavior of their own.
#[derive(Debug, Clone)]
pub enum CommandAction {
    Navigate(ScreenId),
    OpenProject(String),
    Refresh,
    Help,
    Quit,
    /// Runs the same ActionKind the screens use (destructive kinds
    /// still pass through the confirmation modal).
    Request(ActionKind),
}

/// A palette entry. `enabled: false` renders the command with its
/// reason and refuses execution.
#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub category: &'static str,
    pub shortcut: &'static str,
    pub enabled: bool,
    pub disabled_reason: &'static str,
    pub action: CommandAction,
}

/// Builds the live command list from app state. Bounded: one command
/// per screen, per project, plus global actions.
pub fn commands<B: WorkspaceBackend>(app: &App<B>) -> Vec<Command> {
    let mut cmds: Vec<Command> = Vec::new();

    let section_shortcut = |id: ScreenId| -> &'static str {
        let index = PRIMARY_SECTIONS.iter().position(|(sid, _)| *sid == id);
        match index {
            Some(i) if i < 9 => NAV_SHORTCUTS[i],
            _ => "",
        }
    };

    for (id, title) in PRIMARY_SECTIONS {
        cmds.push(Command {
            name: format!("Go to {title}"),
            category: "navigation",
            shortcut: section_shortcut(*id),
            enabled: true,
            disabled_reason: "",
            action: CommandAction::Navigate(*id),
        });
    }
    for (id, title) in SECONDARY_SECTIONS {
        cmds.push(Command {
            name: format!("Go to {title}"),
            category: "navigation",
            shortcut: "",
            enabled: true,
            disabled_reason: "",
            action: CommandAction::Navigate(*id),
        });
    }

    cmds.push(Command {
        name: "Refresh current view".into(),
        category: "global",
        shortcut: "r",
        enabled: true,
        disabled_reason: "",
        action: CommandAction::Refresh,
    });
    cmds.push(Command {
        name: "Help".into(),
        category: "global",
        shortcut: "?",
        enabled: true,
        disabled_reason: "",
        action: CommandAction::Help,
    });
    cmds.push(Command {
        name: "Quit AWH".into(),
        category: "global",
        shortcut: "Ctrl+Q",
        enabled: true,
        disabled_reason: "",
        action: CommandAction::Quit,
    });

    // Project switching: offered from the live backend listing; a
    // backend that cannot list projects shows a disabled entry with
    // the reason instead of an invented list.
    match app.backend.list_projects() {
        Ok(projects) if projects.is_empty() => cmds.push(Command {
            name: "Open project…".into(),
            category: "workspace",
            shortcut: "",
            enabled: false,
            disabled_reason: "no projects yet",
            action: CommandAction::OpenProject(String::new()),
        }),
        Ok(projects) => {
            for name in projects {
                let enabled = app.backend.current_project_hint().as_deref() != Some(name.as_str());
                cmds.push(Command {
                    name: format!("Open project {name}"),
                    category: "workspace",
                    shortcut: "",
                    enabled,
                    disabled_reason: "already open",
                    action: CommandAction::OpenProject(name),
                });
            }
        }
        Err(_) => cmds.push(Command {
            name: "Open project…".into(),
            category: "workspace",
            shortcut: "",
            enabled: false,
            disabled_reason: "project list unavailable",
            action: CommandAction::OpenProject(String::new()),
        }),
    }

    cmds
}

const NAV_SHORTCUTS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// Case-insensitive subsequence fuzzy match with a simple score:
/// prefer consecutive runs and word-prefix hits. Returns None when the
/// query is not a subsequence of the candidate.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    let c: Vec<char> = candidate.chars().flat_map(|c| c.to_lowercase()).collect();
    let mut score = 0i32;
    let mut longest_run = 0i32;
    let mut current_run = 0i32;
    let mut gaps = 0i32;
    let mut qi = query.chars().flat_map(|c| c.to_lowercase()).peekable();
    let mut prev_match: Option<usize> = None;
    let mut ci = 0usize;
    for qc in qi.by_ref() {
        let mut matched = false;
        while ci < c.len() {
            if c[ci] == qc {
                current_run = match prev_match {
                    Some(p) if p + 1 == ci => current_run + 1,
                    _ => 1,
                };
                longest_run = longest_run.max(current_run);
                if ci == 0 || c[ci - 1] == ' ' {
                    score += 2; // word prefix
                }
                if let Some(p) = prev_match {
                    if p + 1 != ci {
                        gaps += 1;
                    }
                }
                prev_match = Some(ci);
                ci += 1;
                matched = true;
                break;
            }
            ci += 1;
        }
        if !matched {
            return None;
        }
    }
    // Coherent (consecutive/prefix) queries must outrank scattered
    // ones: longest run dominates, then fewest gaps.
    Some(4 * longest_run - 2 * gaps + score)
}

/// Palette state held in the app.
#[derive(Debug, Clone, Default)]
pub struct PaletteState {
    pub open: bool,
    pub query: String,
    pub selected: usize,
}

impl PaletteState {
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.selected = 0;
    }
}

/// Filtered command list for the current query, best score first.
pub fn matches<B: WorkspaceBackend>(app: &App<B>) -> Vec<Command> {
    let all = commands(app);
    if app.palette.query.is_empty() {
        return all;
    }
    let mut scored: Vec<(i32, Command)> = all
        .into_iter()
        .filter_map(|cmd| fuzzy_score(&app.palette.query, &cmd.name).map(|s| (s, cmd)))
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, cmd)| cmd).collect()
}

/// Renders the palette as a centered overlay on the content area.
pub fn render<B: WorkspaceBackend>(frame: &mut Frame, area: Rect, app: &App<B>) {
    let height = 12u16.min(area.height.saturating_sub(2).max(1));
    let width = (52u16).min(area.width.saturating_sub(2).max(1));
    let overlay = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + 1,
        width,
        height,
    };
    frame.render_widget(Clear, overlay);
    let [input, list] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(overlay);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("❯ ", Role::Focused.style()),
            Span::raw(app.palette.query.clone()),
            Span::styled("_", Role::Muted.style()),
        ]))
        .block(ratatui::widgets::Block::bordered().border_style(Role::Focused.style())),
        input,
    );

    let commands = matches(app);
    let visible = list.height.saturating_sub(2) as usize; // borders
    let lines: Vec<Line> = commands
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            *i < visible.saturating_add(app.palette.selected)
                && *i >= app.palette.selected.saturating_sub(visible)
        })
        .map(|(i, cmd)| {
            let selected = i == app.palette.selected;
            let style = if selected {
                Role::Selected.style()
            } else if cmd.enabled {
                Role::Normal.style()
            } else {
                Role::Muted.style()
            };
            let mut spans = vec![Span::styled(
                if selected { "▶ " } else { "  " },
                Role::Focused.style(),
            )];
            spans.push(Span::styled(cmd.name.clone(), style));
            if !cmd.enabled {
                spans.push(Span::styled(
                    format!("  (unavailable: {})", cmd.disabled_reason),
                    Role::Blocked.style(),
                ));
            } else if !cmd.shortcut.is_empty() {
                spans.push(Span::styled(
                    format!("  [{}]", cmd.shortcut),
                    Role::Muted.style(),
                ));
            }
            spans.push(Span::styled(
                format!("  · {}", cmd.category),
                Role::Muted.style(),
            ));
            Line::from(spans)
        })
        .collect();
    let count = Line::from(Span::styled(
        format!("{} commands — Enter run · Esc close", commands.len()),
        Role::Muted.style(),
    ));
    let mut display = lines.clone();
    display.push(count);
    frame.render_widget(
        Paragraph::new(display).block(
            ratatui::widgets::Block::bordered()
                .title(Span::styled(" commands ", Role::Focused.style())),
        ),
        list,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_subsequences_case_insensitively() {
        assert!(fuzzy_score("dash", "Go to Dashboard").is_some());
        assert!(fuzzy_score("GTD", "Go to Dashboard").is_some());
        assert!(fuzzy_score("xyz", "Go to Dashboard").is_none());
        assert!(fuzzy_score("gtdash", "Go to Dashboard").is_some());
    }

    #[test]
    fn fuzzy_prefers_consecutive_and_prefix_hits() {
        // "dash" hits a word prefix with consecutive runs; "dshbrd"
        // scatters. The scorer must rank the coherent query higher.
        let consecutive = fuzzy_score("dash", "Dashboard").unwrap();
        let scattered = fuzzy_score("dshbrd", "Dashboard").unwrap();
        assert!(consecutive > scattered, "{consecutive} vs {scattered}");
    }

    #[test]
    fn empty_query_returns_all_commands() {
        let app = crate::tui::app::App::new(crate::tui::backend::LocalBackend::new(
            tempfile::tempdir().unwrap().path().to_path_buf(),
        ));
        let all = commands(&app);
        assert!(all.iter().any(|c| c.name == "Go to Dashboard"));
        assert!(all.iter().any(|c| c.name == "Help"));
        assert!(all.iter().any(|c| c.name == "Quit AWH"));
        // Empty workspace: project switching is disabled with a reason.
        let project_cmd = all
            .iter()
            .find(|c| c.category == "workspace")
            .expect("project command exists");
        assert!(!project_cmd.enabled);
        assert_eq!(project_cmd.disabled_reason, "no projects yet");
    }

    #[test]
    fn project_commands_disable_the_already_open_one() {
        let mut backend = crate::tui::backend::LocalBackend::new(
            tempfile::tempdir().unwrap().path().to_path_buf(),
        );
        backend.create_project("alpha").unwrap();
        backend.open_project("alpha").unwrap();
        let app = crate::tui::app::App::new(backend);
        let all = commands(&app);
        let open = all
            .iter()
            .find(|c| c.name == "Open project alpha")
            .expect("project command");
        assert!(!open.enabled);
        assert_eq!(open.disabled_reason, "already open");
    }
}
