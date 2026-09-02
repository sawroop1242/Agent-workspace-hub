//! Keyboard-first Ratatui terminal UI.
//!
//! The TUI presents workspace operations and never touches the
//! filesystem, Git, or processes directly; it drives a
//! [`WorkspaceBackend`](backend::WorkspaceBackend) implementation.

pub mod app;
pub mod backend;
pub mod screens;

/// Runs the TUI against a local backend rooted at `root`.
pub fn run_local(root: impl Into<std::path::PathBuf>) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let result = app::run(&mut terminal, backend::LocalBackend::new(root));
    ratatui::restore();
    result
}
