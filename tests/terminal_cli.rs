//! End-to-end CLI tests for the TRM-001 terminal surface: `awh terminal
//! run|list|kill` against the real compiled binary. `run` is the
//! bounded, synchronous, argv-only execution the canonical
//! TerminalService owns (shared with MCP, the Control API, and the
//! TUI); `list`/`kill` pin the documented ephemeral lifecycle — an
//! always-empty live list and a deterministic unknown-id kill, never a
//! raw-PID signal.

use std::process::Command;
use tempfile::tempdir;

/// Runs `awh` in `dir`, returning (exit code, stdout, stderr).
fn run_code(dir: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn awh binary");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn terminal_list_reports_the_ephemeral_lifecycle() {
    let dir = tempdir().expect("tempdir");
    let (code, out, err) = run_code(dir.path(), &["terminal", "list"]);
    assert_eq!(code, 0, "list must succeed: {err}");
    assert_eq!(out.trim(), "[]", "machine-readable empty list: {out}");
    assert!(err.contains("ephemeral"), "must explain the model: {err}");
}

#[test]
fn terminal_kill_fails_deterministically_on_any_id() {
    let dir = tempdir().expect("tempdir");
    let (code, _, err) = run_code(dir.path(), &["terminal", "kill", "exec-1"]);
    assert_eq!(code, 1, "kill must fail: {err}");
    assert!(
        err.contains("unknown execution id: exec-1"),
        "must name the id: {err}"
    );
    assert!(err.contains("ephemeral"), "must explain the model: {err}");
}

#[test]
fn run_requires_a_program() {
    let dir = tempdir().expect("tempdir");
    let (code, _, err) = run_code(dir.path(), &["terminal", "run"]);
    assert_ne!(code, 0, "empty argv must not run anything");
    // clap's required-argument error names <ARGV>…; the usage line names
    // the program placeholder either way.
    assert!(
        err.to_lowercase().contains("argv"),
        "must explain the usage: {err}"
    );
}

#[test]
fn run_rejects_program_with_whitespace() {
    let dir = tempdir().expect("tempdir");
    // A program name containing spaces is a shell command line, which
    // the service refuses to interpret — pinned on every platform
    // because the check runs before any spawn.
    let (code, _, err) = run_code(dir.path(), &["terminal", "run", "--", "ls -la"]);
    assert_eq!(code, 1, "must fail: {err}");
    assert!(err.contains("whitespace"), "must name the rule: {err}");
}

#[test]
fn run_timeout_flag_is_bounded() {
    let dir = tempdir().expect("tempdir");
    let (code, _, err) = run_code(
        dir.path(),
        &["terminal", "run", "--timeout", "0", "--", "printf", "x"],
    );
    assert_eq!(code, 1, "zero timeout must be rejected: {err}");
    assert!(
        err.contains("between 1 and 600"),
        "must name the bound: {err}"
    );

    let (code, _, err) = run_code(
        dir.path(),
        &["terminal", "run", "--timeout", "601", "--", "printf", "x"],
    );
    assert_eq!(code, 1, "oversized timeout must be rejected: {err}");
    assert!(
        err.contains("between 1 and 600"),
        "must name the bound: {err}"
    );
}

#[test]
fn run_missing_program_fails_without_auditing_success() {
    let dir = tempdir().expect("tempdir");
    let (code, _, err) = run_code(
        dir.path(),
        &["terminal", "run", "--", "definitely-not-a-real-program-xyz"],
    );
    assert_eq!(code, 1, "spawn failure must fail: {err}");
    assert!(err.contains("failed to spawn"), "must name it: {err}");
}

#[cfg(unix)]
#[test]
fn run_executes_argv_without_a_shell_and_propagates_the_child_exit_code() {
    let dir = tempdir().expect("tempdir");

    // Metacharacters stay argv data: no shell ever interprets them.
    let (code, out, err) = run_code(
        dir.path(),
        &["terminal", "run", "--", "printf", "a;b|c`d$e"],
    );
    assert_eq!(code, 0, "printf must succeed: {err}");
    assert_eq!(out, "a;b|c`d$e", "argv must not be shell-interpreted");

    // Nonzero child exits propagate as the CLI's own exit status.
    let (code, _, _) = run_code(dir.path(), &["terminal", "run", "--", "false"]);
    assert_eq!(code, 1, "child exit 1 must propagate");
}

#[cfg(unix)]
#[test]
fn nonzero_exit_does_not_drop_buffered_child_stdout() {
    let dir = tempdir().expect("tempdir");
    // awk is argv (not a shell): it writes stdout without a trailing
    // newline and exits 3. The CLI must flush its pipes before it
    // replaces the exit status, or the buffered bytes die with it.
    let (code, out, err) = run_code(
        dir.path(),
        &[
            "terminal",
            "run",
            "--",
            "awk",
            "BEGIN{printf \"x\"; exit 3}",
        ],
    );
    assert_eq!(code, 3, "child exit 3 must propagate: {err}");
    assert_eq!(out, "x", "stdout must survive the exit-status swap: {out}");
}

#[cfg(unix)]
#[test]
fn run_timeout_exits_124() {
    let dir = tempdir().expect("tempdir");
    let (code, _, err) = run_code(
        dir.path(),
        &["terminal", "run", "--timeout", "1", "--", "sleep", "5"],
    );
    assert_eq!(code, 124, "timeout must exit 124: {err}");
    assert!(err.contains("timed out"), "must name the timeout: {err}");
}

#[cfg(unix)]
#[test]
fn run_is_audited_without_its_arguments() {
    let dir = tempdir().expect("tempdir");
    let secret = "SECRET-ARG-NEVER-AUDIT-ME-9f2c";
    let (code, _, err) = run_code(dir.path(), &["terminal", "run", "--", "printf", secret]);
    assert_eq!(code, 0, "printf must succeed: {err}");

    let audit = std::fs::read_to_string(dir.path().join(".agent/audit/audit.log"))
        .expect("durable audit log");
    assert!(audit.contains("cli_terminal_run"), "got: {audit}");
    assert!(
        !audit.contains(secret),
        "arguments can carry secrets and must never reach the audit log: {audit}"
    );
    assert!(audit.contains("printf"), "program name is the subject");
}

#[cfg(windows)]
#[test]
fn run_executes_argv_on_windows() {
    let dir = tempdir().expect("tempdir");
    let (code, out, err) = run_code(
        dir.path(),
        &["terminal", "run", "--", "cmd", "/C", "echo", "argv works"],
    );
    assert_eq!(code, 0, "cmd echo must succeed: {err}");
    assert!(out.trim_end().ends_with("argv works"), "got: {out}");
}
