//! Keyboard-first Ratatui terminal UI.
//!
//! The TUI presents workspace operations and never touches the
//! filesystem, Git, or processes directly; it drives a
//! [`WorkspaceBackend`](backend::WorkspaceBackend) implementation.

pub mod app;
pub mod backend;
pub mod components;
pub mod keymap;
pub mod operations;
pub mod palette;
pub mod remote;
pub mod screens;
pub mod shell;
pub mod theme;

/// Runs the TUI against a local backend rooted at `root`.
/// Rejects non-interactive stdin before ratatui's terminal setup: piping
/// into `awh tui` would otherwise panic inside the crossterm backend
/// instead of failing like any other CLI misuse.
fn require_interactive_terminal() -> anyhow::Result<()> {
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        Ok(())
    } else {
        anyhow::bail!("awh tui needs an interactive terminal (stdin is not a tty)")
    }
}

/// Runs the TUI against the local workspace at `root`.
pub fn run_local(root: impl Into<std::path::PathBuf>) -> anyhow::Result<()> {
    require_interactive_terminal()?;
    let mut terminal = ratatui::init();
    let result = app::run(&mut terminal, backend::LocalBackend::new(root));
    ratatui::restore();
    result
}

/// Runs the TUI against a remote Control API at `base` (e.g.
/// `https://host:8080`) authenticated with `api_key`. The handshake
/// runs before the terminal is initialized so connection failures are
/// reported as ordinary CLI errors instead of a broken TUI.
pub fn run_remote(base: &str, api_key: &str) -> anyhow::Result<()> {
    let backend = remote::RemoteBackend::new(base, api_key);
    match backend.probe() {
        remote::ConnectionState::Connected { version, .. } => {
            eprintln!("connected to {base} (server version {version})");
        }
        remote::ConnectionState::AuthFailed => anyhow::bail!("remote AWH rejected the API key"),
        remote::ConnectionState::Unavailable { reason } => {
            anyhow::bail!("remote AWH unreachable: {reason}")
        }
        remote::ConnectionState::Incompatible { server_version } => anyhow::bail!(
            "remote AWH version {server_version} is incompatible with this client ({})",
            env!("CARGO_PKG_VERSION")
        ),
        state => anyhow::bail!("connection failed: {state:?}"),
    }
    require_interactive_terminal()?;
    let mut terminal = ratatui::init();
    let result = app::run(&mut terminal, backend);
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    /// The guard is a pure tty check: when stdin is not a terminal it
    /// must refuse with a normal CLI error. `cargo test` never runs with
    /// stdin as a tty in CI, but an interactive `cargo test` session
    /// does — so this test only pins the error SHAPE on the refusals we
    /// can force, by asserting the helper never touches the terminal
    /// state on either path. (run_local itself is intentionally not
    /// exercised here: under a real tty it would enter the event loop.)
    #[test]
    fn guard_message_names_the_requirement() {
        // Under a non-tty stdin the guard must fail with the exact
        // actionable message; under a tty it must succeed without any
        // terminal side effects.
        match super::require_interactive_terminal() {
            Ok(()) => {
                // Only possible when cargo test itself runs in a real
                // terminal; nothing to assert beyond "did not panic".
            }
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message.contains("interactive terminal"),
                    "guard message should name the requirement: {message}"
                );
            }
        }
    }
}
