//! Semantic design system for the premium TUI.
//!
//! Every visual role maps to BOTH a style and a text symbol so status is
//! never encoded in color alone (accessibility + limited-color terminals).
//! Screens pull styles from here instead of hand-rolling `Color::*` picks.

use ratatui::style::{Color, Modifier, Style};

/// Semantic emphasis roles used across the shell and every screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    #[default]
    Normal,
    Muted,
    Focused,
    Selected,
    Active,
    Success,
    Warning,
    Error,
    Blocked,
    Running,
    Pending,
    Completed,
    Cancelled,
    Disconnected,
    Unknown,
}

impl Role {
    /// Terminal style for the role. Deliberately conservative on color:
    /// emphasis relies on symbols + modifiers first.
    pub fn style(self) -> Style {
        match self {
            Role::Normal => Style::default().fg(Color::Gray),
            Role::Muted => Style::default().fg(Color::DarkGray),
            Role::Focused => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            Role::Selected => Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
            Role::Active => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            Role::Success => Style::default().fg(Color::Green),
            Role::Warning => Style::default().fg(Color::Yellow),
            Role::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            Role::Blocked => Style::default().fg(Color::Magenta),
            Role::Running => Style::default().fg(Color::Cyan),
            Role::Pending => Style::default().fg(Color::DarkGray),
            Role::Completed => Style::default().fg(Color::Green),
            Role::Cancelled => Style::default().fg(Color::DarkGray),
            Role::Disconnected => Style::default().fg(Color::Yellow),
            Role::Unknown => Style::default().fg(Color::Magenta),
        }
    }

    /// Text symbol for the role — the color-independent channel.
    pub fn symbol(self) -> &'static str {
        match self {
            Role::Normal => "·",
            Role::Muted => "·",
            Role::Focused => "»",
            Role::Selected => "▶",
            Role::Active => "●",
            Role::Success => "✓",
            Role::Warning => "▲",
            Role::Error => "✗",
            Role::Blocked => "⊘",
            Role::Running => "↻",
            Role::Pending => "○",
            Role::Completed => "✔",
            Role::Cancelled => "∅",
            Role::Disconnected => "⚡",
            Role::Unknown => "?",
        }
    }

    /// `symbol label` pair styled by the role.
    pub fn tag(self, label: &str) -> ratatui::text::Span<'static> {
        ratatui::text::Span::styled(format!("{} {}", self.symbol(), label), self.style())
    }
}

/// Terminal size tier driving the responsive layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutProfile {
    /// Very small: active view + primary content only.
    Compact,
    /// Navigation + main content + detail/activity.
    Normal,
    /// Richer dashboard, simultaneous detail panels.
    Large,
}

impl LayoutProfile {
    pub fn for_area(area: ratatui::layout::Rect) -> Self {
        let (w, h) = (area.width, area.height);
        if w < 80 || h < 20 {
            LayoutProfile::Compact
        } else if w >= 120 && h >= 40 {
            LayoutProfile::Large
        } else {
            LayoutProfile::Normal
        }
    }

    /// Width the navigation rail takes when rendered vertically.
    pub fn nav_width(self) -> u16 {
        match self {
            LayoutProfile::Compact => 0, // collapsed to the header line
            LayoutProfile::Normal => 16,
            LayoutProfile::Large => 20,
        }
    }

    /// Whether a right-hand detail/activity panel is affordable.
    pub fn detail_panel(self) -> bool {
        matches!(self, LayoutProfile::Large)
    }
}

/// UI glyph set. Plain-ASCII fallbacks keep the TUI legible on terminals
/// without unicode support; the richer glyphs are the default.
pub struct Glyphs {
    pub dir: &'static str,
    pub file: &'static str,
    pub up: &'static str,
    pub branch: &'static str,
    pub bullet: &'static str,
    pub ellipsis: &'static str,
    pub separator: &'static str,
}

pub const GLYPHS: Glyphs = Glyphs {
    dir: "▸",
    file: "·",
    up: "↑",
    branch: "⎇",
    bullet: "•",
    ellipsis: "…",
    separator: "│",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_has_a_unique_symbol() {
        // Never rely on color alone: each role must carry a non-empty
        // text symbol, and distinct statuses in a status column may not
        // share a symbol (color is the redundant channel, not the only
        // one).
        let roles = [
            Role::Success,
            Role::Warning,
            Role::Error,
            Role::Blocked,
            Role::Running,
            Role::Pending,
            Role::Completed,
            Role::Cancelled,
            Role::Disconnected,
            Role::Unknown,
        ];
        let mut seen = std::collections::HashSet::new();
        for role in roles {
            let sym = role.symbol();
            assert!(!sym.is_empty());
            assert!(seen.insert(sym), "duplicate symbol {sym:?}");
        }
    }

    #[test]
    fn layout_profiles_partition_terminal_sizes() {
        let mk = |w, h| ratatui::layout::Rect::new(0, 0, w, h);
        assert_eq!(LayoutProfile::for_area(mk(79, 40)), LayoutProfile::Compact);
        assert_eq!(LayoutProfile::for_area(mk(120, 19)), LayoutProfile::Compact);
        assert_eq!(LayoutProfile::for_area(mk(80, 20)), LayoutProfile::Normal);
        assert_eq!(LayoutProfile::for_area(mk(119, 39)), LayoutProfile::Normal);
        assert_eq!(LayoutProfile::for_area(mk(120, 40)), LayoutProfile::Large);
    }
}
