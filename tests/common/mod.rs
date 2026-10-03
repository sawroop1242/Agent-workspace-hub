//! Shared helpers for the CLI-boundary integration suites.
//!
//! NOT a test crate of its own: included per-suite via
//! `#[path = "common/mod.rs"] mod common;` (the classic pattern --
//! cargo only compiles `tests/*.rs` as integration crates, so code
//! under `tests/common/` is never built standalone).

use std::path::Path;
use std::process::Command;

/// Every `AWH_*` environment variable the binary reads at runtime
/// (enumerated from `env::var` call sites in src/, excluding the
/// `AWH_TEST_*` fixtures that only exist inside unit tests). Tests
/// strip all of them from spawned children so machine-local
/// configuration (developer shells, CI runner envs) can never change
/// what a test observes. THE SINGLE SOURCE: when src/ grows a new
/// `AWH_*` read, add it here -- every suite inherits the fix.
pub const SANITIZED_AWH_VARS: &[&str] = &[
    // Control-API / SSE server plane
    "AWH_HOST",
    "AWH_PORT",
    "AWH_TLS_CERT",
    "AWH_TLS_KEY",
    "AWH_API_KEY",
    "AWH_ALLOWED_ORIGINS",
    // Dispatcher resource limits
    "AWH_MAX_MCP_LINE_BYTES",
    "AWH_MAX_HTTP_BODY_BYTES",
    "AWH_MCP_REQUEST_TIMEOUT_SECS",
    "AWH_HTTP_CLIENT_TIMEOUT_SECS",
    "AWH_CIRCUIT_FAILURE_THRESHOLD",
    "AWH_CIRCUIT_COOLDOWN_SECS",
    // Context-engine tuning
    "AWH_CONTEXT_ENABLED",
    "AWH_CONTEXT_MEMORY_ENABLED",
    "AWH_CONTEXT_AUTO_COMPRESS",
    "AWH_CONTEXT_AUTO_OFFLOAD",
    "AWH_CONTEXT_MAX_INPUT_TOKENS",
    "AWH_CONTEXT_RESERVED_OUTPUT_TOKENS",
    "AWH_CONTEXT_SAFETY_MARGIN_TOKENS",
    // Registry / trust roots
    "AWH_GLOBAL_SKILLS_ROOT",
    "AWH_TRUST_DIR",
    // Sandbox + tunnel helpers
    "AWH_BWRAP",
    "AWH_NGROK_AUTHTOKEN",
];

/// Non-`AWH_*` credentials and provider routing the binary reads.
/// Stripped alongside the AWH set for the same reason: a developer's
/// or runner's credentials must not enable provider behavior mid-test.
/// (Fixed names only -- dynamic reads like `--api-key-env` or
/// registry-driven `${secret:NAME}` expansion are user/registry-named
/// and cannot be enumerated statically; they are also fail-closed
/// behind explicit allow-lists in the product.)
pub const SANITIZED_PROVIDER_VARS: &[&str] = &[
    "GITHUB_TOKEN",
    "GITHUB_PERSONAL_ACCESS_TOKEN",
    "GITHUB_API_URL",
    "GITHUB_DEFAULT_OWNER",
    "GITHUB_DEFAULT_REPO",
    "COMPOSIO_API_KEY",
    "COMPOSIO_CONNECTED_ACCOUNT_ID",
    "COMPOSIO_TOOLKIT",
    "NGROK_AUTHTOKEN",
];

/// A hermetic `awh` child: the real binary, run in `dir`, with the
/// ambient `AWH_*` and provider-credential environment stripped.
/// Suites add their own args/extra env before spawning.
pub fn sanitized_command(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_awh"));
    command.args(args).current_dir(dir);
    for key in SANITIZED_AWH_VARS.iter().chain(SANITIZED_PROVIDER_VARS) {
        command.env_remove(key);
    }
    command
}
