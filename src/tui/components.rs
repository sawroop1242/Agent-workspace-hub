//! Reusable TUI components built on the semantic theme.
//!
//! Every screen composes these primitives instead of hand-rolling
//! widgets: panels, tables, status tags, breadcrumbs, tabs, diff and
//! log viewers, dialog frames, placeholders, and activity feeds.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

use super::theme::{Role, GLYPHS};

/// A titled panel with a semantic status role for its title.
pub fn panel_block<'a>(title: &str, role: Role, focused: bool) -> Block<'a> {
    let marker = if focused { GLYPHS.bullet } else { " " };
    let mut block = Block::bordered().title(format!(" {marker}{title} "));
    if focused {
        block = block.border_style(Style::default().fg(ratatui::style::Color::Cyan));
    }
    let _ = role;
    block
}

/// Renders a simple `label: value` row with a muted label.
pub fn kv_row<'a>(label: &str, value: impl Into<String>) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label:<14}"), Role::Muted.style()),
        Span::raw(value.into()),
    ])
}

/// Status tag: symbol + label, styled by role (never color-only).
pub fn status_tag(role: Role, label: &str) -> Span<'static> {
    role.tag(label)
}

/// Breadcrumb line for path-like context (files, projects, scopes).
pub fn breadcrumbs<'a>(parts: &[&str]) -> Line<'a> {
    let mut spans = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" / ", Role::Muted.style()));
        }
        spans.push(Span::raw((*part).to_string()));
    }
    Line::from(spans)
}

/// Tab bar: highlights the active tab with the focus marker.
pub fn tabs_line<'a>(tabs: &[&str], active: usize) -> Line<'a> {
    let mut spans = Vec::new();
    for (i, tab) in tabs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Role::Muted.style()));
        }
        if i == active.min(tabs.len().saturating_sub(1)) {
            spans.push(Span::styled(format!("[{tab}]"), Role::Focused.style()));
        } else {
            spans.push(Span::styled(format!(" {tab} "), Role::Muted.style()));
        }
    }
    Line::from(spans)
}

/// State a bounded panel can render when it has nothing to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewState {
    Loading,
    Empty { hint: &'static str },
    Unavailable { reason: &'static str },
    Denied { reason: &'static str },
    Error { message: String },
}

impl ViewState {
    fn describe(&self) -> String {
        match self {
            ViewState::Loading => format!("{} loading…", Role::Pending.symbol()),
            ViewState::Empty { hint } => {
                format!("{} nothing here — {hint}", Role::Pending.symbol())
            }
            ViewState::Unavailable { reason } => {
                format!("{} unavailable — {reason}", Role::Disconnected.symbol())
            }
            ViewState::Denied { reason } => format!("{} denied — {reason}", Role::Blocked.symbol()),
            ViewState::Error { message } => format!("{} {message}", Role::Error.symbol()),
        }
    }

    pub fn line(&self) -> Line<'_> {
        let text = self.describe();
        Line::from(Span::raw(text))
    }
}

/// Renders a non-ready placeholder inside the given panel area.
pub fn render_state_placeholder(frame: &mut Frame, area: Rect, state: &ViewState) {
    frame.render_widget(Paragraph::new(state.line()).wrap(Wrap { trim: true }), area);
}

/// A two-column table row: left cell primary text, right cell a status
/// tag. Callers keep lists bounded; this component does no IO.
pub fn tagged_row<'a>(primary: String, role: Role, label: &str) -> Line<'a> {
    Line::from(vec![
        Span::raw(primary),
        Span::raw("  "),
        status_tag(role, label),
    ])
}

/// Activity feed row: `symbol action subject` with the kind's role.
pub fn feed_row<'a>(role: Role, action: &str, subject: &str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{} ", role.symbol()), role.style()),
        Span::styled(format!("{action:<22}"), Role::Normal.style()),
        Span::raw(subject.to_string()),
    ])
}

// ---------------------------------------------------------------------------
// Diff viewer
// ---------------------------------------------------------------------------

/// One parsed line of a unified diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    Hunk {
        header: String,
    },
    Add {
        text: String,
        old_no: u32,
        new_no: u32,
    },
    Remove {
        text: String,
        old_no: u32,
        new_no: u32,
    },
    Context {
        text: String,
        old_no: u32,
        new_no: u32,
    },
    Meta {
        text: String,
    },
}

/// Parses `git diff` unified output into structured lines with line
/// numbers. Bounded to `max_lines`; `truncated` reports the cut.
pub fn parse_diff(output: &str, max_lines: usize) -> (Vec<DiffLine>, bool) {
    let mut lines = Vec::new();
    let mut truncated = false;
    let mut old_no: u32 = 0;
    let mut new_no: u32 = 0;
    for raw in output.lines() {
        if lines.len() >= max_lines {
            truncated = true;
            break;
        }
        if let Some(header) = raw.strip_prefix("@@") {
            // Parse the ranges to keep line numbers honest.
            if let Some(ranges) = header.split("@@").next() {
                for part in ranges.split_whitespace() {
                    if let Some(n) = part.strip_prefix('-') {
                        old_no = n
                            .split(',')
                            .next()
                            .unwrap_or("0")
                            .trim_start_matches('-')
                            .parse()
                            .unwrap_or(1);
                    } else if let Some(n) = part.strip_prefix('+') {
                        new_no = n.split(',').next().unwrap_or("0").parse().unwrap_or(1);
                    }
                }
            }
            lines.push(DiffLine::Hunk {
                header: raw.to_string(),
            });
        } else if let Some(text) = raw.strip_prefix('+') {
            lines.push(DiffLine::Add {
                text: text.to_string(),
                old_no,
                new_no,
            });
            new_no = new_no.saturating_add(1);
        } else if let Some(text) = raw.strip_prefix('-') {
            lines.push(DiffLine::Remove {
                text: text.to_string(),
                old_no,
                new_no,
            });
            old_no = old_no.saturating_add(1);
        } else if let Some(text) = raw.strip_prefix(' ') {
            lines.push(DiffLine::Context {
                text: text.to_string(),
                old_no,
                new_no,
            });
            old_no = old_no.saturating_add(1);
            new_no = new_no.saturating_add(1);
        } else {
            lines.push(DiffLine::Meta {
                text: raw.to_string(),
            });
        }
    }
    (lines, truncated)
}

/// How a diff is displayed: which segment scrolled into view.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiffScroll {
    /// Horizontal offset into long lines.
    pub horizontal: usize,
    /// Vertical offset (top line index).
    pub vertical: usize,
}

/// Renders structured diff lines with line numbers and horizontal
/// scrolling. Indicates truncation and binary/large output via the
/// header caller-supplied note.
pub fn render_diff(
    frame: &mut Frame,
    area: Rect,
    lines: &[DiffLine],
    truncated: bool,
    scroll: DiffScroll,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let gutter = 11; // "12345 12345 "
    let width = area.width.saturating_sub(gutter + 1) as usize;
    let visible = area.height as usize;
    let rows: Vec<Line> = lines
        .iter()
        .skip(scroll.vertical)
        .take(visible)
        .map(|line| match line {
            DiffLine::Hunk { header } => Line::from(Span::styled(
                truncate_scroll(header, scroll.horizontal, width),
                Role::Focused.style(),
            )),
            DiffLine::Add {
                text,
                old_no,
                new_no,
            } => Line::from(vec![
                Span::styled(format!("{old_no:>5} "), Role::Muted.style()),
                Span::styled(format!("{new_no:>5} ",), Role::Muted.style()),
                Span::styled(
                    format!("+{}", truncate_scroll(text, scroll.horizontal, width)),
                    Role::Success.style(),
                ),
            ]),
            DiffLine::Remove {
                text,
                old_no,
                new_no,
            } => Line::from(vec![
                Span::styled(format!("{old_no:>5} "), Role::Muted.style()),
                Span::styled(format!("{new_no:>5} ",), Role::Muted.style()),
                Span::styled(
                    format!("-{}", truncate_scroll(text, scroll.horizontal, width)),
                    Role::Error.style(),
                ),
            ]),
            DiffLine::Context {
                text,
                old_no,
                new_no,
            } => Line::from(vec![
                Span::styled(format!("{old_no:>5} "), Role::Muted.style()),
                Span::styled(format!("{new_no:>5} ",), Role::Muted.style()),
                Span::styled(
                    truncate_scroll(text, scroll.horizontal, width),
                    Role::Normal.style(),
                ),
            ]),
            DiffLine::Meta { text } => Line::from(Span::styled(
                truncate_scroll(text, scroll.horizontal, width),
                Role::Muted.style(),
            )),
        })
        .collect();
    let mut display: Vec<Line> = rows;
    if truncated {
        display.insert(
            0,
            Line::from(Span::styled(
                format!("{} diff truncated at display bound", Role::Warning.symbol()),
                Role::Warning.style(),
            )),
        );
    }
    frame.render_widget(Paragraph::new(display), area);
}

fn truncate_scroll(text: &str, offset: usize, width: usize) -> String {
    let mut s = text;
    let mut offset = offset;
    while offset > 0 {
        match s.char_indices().nth(1) {
            Some((i, _)) => {
                s = &s[i..];
                offset -= 1;
            }
            None => {
                s = "";
                break;
            }
        }
    }
    if s.chars().count() > width {
        s.chars().take(width).collect()
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Dialogs / overlays
// ---------------------------------------------------------------------------

/// Centers a rect for an overlay inside `outer`.
pub fn centered(outer: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(outer.width);
    let h = height.min(outer.height);
    Rect {
        x: outer.x + (outer.width.saturating_sub(w)) / 2,
        y: outer.y + (outer.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

/// Renders a dialog frame with title, explanation lines, and an action
/// hint row. The caller owns focus/key handling; this is pure chrome.
pub fn render_dialog(
    frame: &mut Frame,
    outer: Rect,
    title: &str,
    explanation: &[Line<'_>],
    actions: &str,
    role: Role,
) {
    let height = (explanation.len() as u16 + 4).clamp(5, outer.height);
    let width = (48u16).min(outer.width);
    let area = centered(outer, width, height);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(format!(" {} ", title))
        .title_style(role.style())
        .border_style(role.style());
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    let inner_body = Rect {
        x: body.x + 1,
        y: body.y + 1,
        width: body.width.saturating_sub(2),
        height: body.height.saturating_sub(2),
    };
    frame.render_widget(Paragraph::new(explanation.to_vec()).block(block), area);
    frame.render_widget(
        Paragraph::new(Span::styled(actions, Role::Muted.style())),
        Rect {
            x: footer.x + 1,
            y: footer.y,
            width: footer.width.saturating_sub(2),
            height: 1,
        },
    );
    let _ = inner_body;
}

/// Borderless underline row used for in-place inputs.
pub fn input_line<'a>(label: &str, value: &str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label}: "), Role::Focused.style()),
        Span::raw(value.to_string()),
        Span::styled("_", Role::Muted.style()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1,3 +1,4 @@
 context
-old
+new
+added
@@ -10,1 +10,2 @@
 more
";

    #[test]
    fn parse_diff_tracks_line_numbers_and_hunks() {
        let (lines, truncated) = parse_diff(SAMPLE, 100);
        assert!(!truncated);
        assert_eq!(lines.len(), 10);
        assert!(matches!(&lines[0], DiffLine::Meta { .. }));
        assert!(matches!(&lines[3], DiffLine::Hunk { .. }));
        match &lines[5] {
            DiffLine::Remove { old_no, new_no, .. } => {
                assert_eq!(*old_no, 2);
                assert_eq!(*new_no, 2);
            }
            other => panic!("expected remove line, got {other:?}"),
        }
        match &lines[7] {
            DiffLine::Add { new_no, .. } => assert_eq!(*new_no, 3),
            other => panic!("expected add line, got {other:?}"),
        }
        // The second hunk resets the counters from its header.
        match &lines[8] {
            DiffLine::Hunk { header } => assert!(header.contains("@@ -10,1 +10,2 @@")),
            other => panic!("expected second hunk, got {other:?}"),
        }
    }

    #[test]
    fn parse_diff_bounds_output() {
        let (lines, truncated) = parse_diff(SAMPLE, 4);
        assert!(truncated);
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn parse_diff_detects_binary_hint_only_via_meta() {
        // Binary diffs show "Binary files ... differ" as a meta line —
        // the viewer never renders bytes.
        let (lines, _) = parse_diff("Binary files a.bin and b.bin differ", 10);
        assert!(matches!(&lines[0], DiffLine::Meta { text } if text.contains("Binary")));
    }

    #[test]
    fn truncate_scroll_slices_by_chars_not_bytes() {
        let s = "héllo wörld";
        assert_eq!(truncate_scroll(s, 0, 11), s);
        assert_eq!(truncate_scroll(s, 1, 10), "éllo wörld");
        assert_eq!(truncate_scroll(s, 0, 5), "héllo");
        assert_eq!(truncate_scroll(s, 99, 5), "");
    }

    #[test]
    fn centered_clamps_to_outer() {
        let outer = Rect::new(0, 0, 20, 10);
        let r = centered(outer, 100, 100);
        assert_eq!(r, outer);
        let r = centered(outer, 10, 4);
        assert_eq!((r.x, r.y, r.width, r.height), (5, 3, 10, 4));
    }

    #[test]
    fn view_state_labels_carry_symbols() {
        // Every non-ready state renders its own symbol so color is
        // never the only channel.
        for state in [
            ViewState::Loading,
            ViewState::Empty {
                hint: "no projects",
            },
            ViewState::Unavailable {
                reason: "remote has no route",
            },
            ViewState::Denied { reason: "policy" },
            ViewState::Error {
                message: "boom".into(),
            },
        ] {
            let text = state.describe();
            assert!(text
                .chars()
                .next()
                .map(|c| !c.is_whitespace())
                .unwrap_or(false));
        }
    }
}
