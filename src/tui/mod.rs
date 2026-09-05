//! Keyboard-first Ratatui terminal UI.
//!
//! The TUI presents workspace operations and never touches the
//! filesystem, Git, or processes directly; it drives a
//! [`WorkspaceBackend`](backend::WorkspaceBackend) implementation.

pub mod app;
pub mod backend;
pub mod remote;
pub mod screens;

/// Runs the TUI against a local backend rooted at `root`.
pub fn run_local(root: impl Into<std::path::PathBuf>) -> anyhow::Result<()> {
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
    let mut terminal = ratatui::init();
    let result = app::run(&mut terminal, backend);
    ratatui::restore();
    result
}
