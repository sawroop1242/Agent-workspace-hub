//! Central keymap registry.
//!
//! One source of truth for every binding; the Help overlay, the footer
//! hint line, and the command palette all render FROM these tables, so
//! what the user sees is always the actually active keymap.

use super::screens::ScreenId;

/// One keyboard binding as exposed in help/palette.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub key: &'static str,
    pub action: &'static str,
}

pub const GLOBAL_BINDINGS: &[Binding] = &[
    Binding {
        key: "Ctrl+K",
        action: "command palette",
    },
    Binding {
        key: "?",
        action: "help",
    },
    Binding {
        key: "F1",
        action: "help",
    },
    Binding {
        key: "Tab",
        action: "next view",
    },
    Binding {
        key: "Shift+Tab",
        action: "previous view",
    },
    Binding {
        key: "Esc",
        action: "close / back",
    },
    Binding {
        key: "b",
        action: "back",
    },
    Binding {
        key: "r",
        action: "refresh view",
    },
    Binding {
        key: "/",
        action: "search or filter",
    },
    Binding {
        key: "Ctrl+Q",
        action: "quit",
    },
    Binding {
        key: "F12",
        action: "quit",
    },
];

/// The palette/search overlays list every binding they consume here so
/// help stays in sync with the actual dispatcher.
pub const OVERLAY_BINDINGS: &[Binding] = &[
    Binding {
        key: "chars",
        action: "filter (fuzzy)",
    },
    Binding {
        key: "Up/Down",
        action: "choose command",
    },
    Binding {
        key: "Enter",
        action: "run command",
    },
    Binding {
        key: "Esc",
        action: "close overlay",
    },
];

/// Per-screen bindings. Screens with no entry inherit only the global
/// set; screens add their contextual actions here.
pub fn screen_bindings(screen: ScreenId) -> Vec<Binding> {
    match screen {
        ScreenId::Dashboard => vec![Binding {
            key: "Enter",
            action: "open selected summary",
        }],
        ScreenId::Agents => vec![
            Binding {
                key: "Up/Down",
                action: "select agent",
            },
            Binding {
                key: "Enter",
                action: "agent detail",
            },
            Binding {
                key: "Space",
                action: "start/stop agent",
            },
        ],
        ScreenId::Tasks => vec![
            Binding {
                key: "Up/Down",
                action: "select task",
            },
            Binding {
                key: "Enter",
                action: "task detail",
            },
        ],
        ScreenId::Files => vec![
            Binding {
                key: "Enter",
                action: "open dir/file",
            },
            Binding {
                key: "Backspace",
                action: "up one dir",
            },
            Binding {
                key: "n",
                action: "new file/dir",
            },
            Binding {
                key: "m",
                action: "rename entry",
            },
            Binding {
                key: "d/Del",
                action: "delete (confirms)",
            },
            Binding {
                key: "s",
                action: "search contents",
            },
            Binding {
                key: "e",
                action: "edit selected file",
            },
        ],
        ScreenId::Editor => vec![
            Binding {
                key: "chars",
                action: "type",
            },
            Binding {
                key: "Ctrl+S",
                action: "save",
            },
            Binding {
                key: "Ctrl+D",
                action: "discard changes (confirms)",
            },
        ],
        ScreenId::Changes => vec![
            Binding {
                key: "1/2",
                action: "unstaged/staged list",
            },
            Binding {
                key: "Tab",
                action: "switch side",
            },
            Binding {
                key: "Space",
                action: "stage/unstage selected",
            },
            Binding {
                key: "c",
                action: "commit (input)",
            },
            Binding {
                key: "Left/Right",
                action: "scroll diff",
            },
        ],
        ScreenId::Git => vec![
            Binding {
                key: "1..5",
                action: "status/diffs/log/branches",
            },
            Binding {
                key: "+/-",
                action: "stage/unstage",
            },
            Binding {
                key: "c",
                action: "commit",
            },
            Binding {
                key: "p",
                action: "push",
            },
            Binding {
                key: "P",
                action: "pull",
            },
            Binding {
                key: "Backspace",
                action: "refresh",
            },
        ],
        ScreenId::Context => vec![Binding {
            key: "e",
            action: "edit context",
        }],
        ScreenId::Memory => vec![
            Binding {
                key: "a",
                action: "append entry",
            },
            Binding {
                key: "/",
                action: "filter entries",
            },
        ],
        ScreenId::Skills => vec![Binding {
            key: "Space",
            action: "toggle project skill",
        }],
        ScreenId::Mcp => vec![Binding {
            key: "Up/Down",
            action: "select server",
        }],
        ScreenId::Terminal => vec![
            Binding {
                key: "Enter",
                action: "run command",
            },
            Binding {
                key: "Up",
                action: "previous from history",
            },
        ],
        ScreenId::Audit => vec![
            Binding {
                key: "/",
                action: "filter events",
            },
            Binding {
                key: "Enter",
                action: "event detail",
            },
            Binding {
                key: "Space",
                action: "cycle kind filter",
            },
        ],
        ScreenId::Settings => vec![],
        ScreenId::Projects => vec![
            Binding {
                key: "Enter",
                action: "open project",
            },
            Binding {
                key: "n",
                action: "new project",
            },
            Binding {
                key: "d",
                action: "delete project (confirms)",
            },
        ],
        ScreenId::Remote => vec![],
        ScreenId::Help => vec![],
    }
}

/// Footer hint for the active screen: global essentials plus the
/// screen's own bindings, truncated to fit narrow terminals at render
/// time by the caller.
pub fn footer_hint(screen: ScreenId) -> String {
    let mut parts: Vec<String> = vec!["^K commands".into(), "? help".into()];
    for b in screen_bindings(screen) {
        parts.push(format!(
            "{} {}",
            b.key,
            b.action.split_whitespace().next().unwrap_or("")
        ));
    }
    parts.join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_screen_declares_bindings() {
        // The help overlay iterates screens and must never hit a
        // missing entry (falls back to empty vec — still valid).
        for &id in super::super::screens::ALL_SCREENS {
            let _ = screen_bindings(id);
        }
    }

    #[test]
    fn footer_hint_is_nonempty_for_every_screen() {
        for &id in super::super::screens::ALL_SCREENS {
            assert!(!footer_hint(id).is_empty());
        }
    }
}
