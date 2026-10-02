//! Master Test Prompt 01 — Foundation & Distribution verification.
//!
//! Human-behavior black-box tests against the real compiled `awh` binary:
//! `--version`/`status`/`init` semantics, configuration precedence
//! (defaults < `AWH_*` env < CLI flags), fail-closed configuration errors,
//! API-key absence/emptiness, TLS half-configuration, secret-safety of all
//! outputs, persistence across fresh processes, concurrent independent
//! workspaces, and unicode/space path handling.
//!
//! Every test drives the actual binary via `CARGO_BIN_EXE_awh` with an
//! explicit child environment; process-global env is never mutated.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

const BIN: &str = env!("CARGO_BIN_EXE_awh");

/// A synthetic token-shaped secret used for leakage checks. Never a real
/// credential.
const SYNTHETIC_SECRET: &str = "ghp_AWHTEST synthetic-LEAKCHECK-0123456789abcdef";

/// Runs `awh <args>` in `dir` with an explicit environment.
fn run_env(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.args(args)
        .current_dir(dir)
        .env_remove("AWH_HOST")
        .env_remove("AWH_PORT")
        .env_remove("AWH_TLS_CERT")
        .env_remove("AWH_TLS_KEY")
        .env_remove("AWH_API_KEY")
        .env_remove("AWH_ALLOWED_ORIGINS");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("spawn awh binary")
}

fn run(dir: &Path, args: &[&str]) -> Output {
    run_env(dir, args, &[])
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn manifest_path(root: &Path) -> PathBuf {
    root.join(".agent").join("workspace.json")
}

/// Reads one line from the child's stdout, blocking until it arrives.
fn read_stdout_line(child: &mut std::process::Child) -> String {
    use std::io::BufRead;
    let stdout = child.stdout.take().expect("stdout piped");
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read stdout line");
    line
}

// ---------------------------------------------------------------------------
// version / status
// ---------------------------------------------------------------------------

#[test]
fn version_flag_reports_crate_version_without_workspace_or_secret_leak() {
    let dir = tempdir().expect("tempdir");
    // No init: version must not require an initialized workspace.
    assert!(!manifest_path(dir.path()).exists());
    let out = run_env(
        dir.path(),
        &["--version"],
        &[("AWH_API_KEY", SYNTHETIC_SECRET)],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.trim().starts_with("awh "),
        "version output must start with the binary name, got: {text:?}"
    );
    assert!(
        text.contains(env!("CARGO_PKG_VERSION")),
        "version must contain the crate version {}, got: {text:?}",
        env!("CARGO_PKG_VERSION")
    );
    // Deterministic: identical on repeated invocation.
    let out2 = run_env(
        dir.path(),
        &["--version"],
        &[("AWH_API_KEY", SYNTHETIC_SECRET)],
    );
    assert_eq!(stdout(&out2), text);
    // No secret leakage in either stream.
    assert!(!stdout(&out).contains(SYNTHETIC_SECRET));
    assert!(!stderr(&out).contains(SYNTHETIC_SECRET));
    // Works from an initialized workspace too.
    let init = run(dir.path(), &["init"]);
    assert!(init.status.success());
    let out3 = run(dir.path(), &["--version"]);
    assert!(out3.status.success());
}

#[test]
fn status_succeeds_on_uninitialized_and_initialized_directories() {
    let dir = tempdir().expect("tempdir");
    // Uninitialized directory: status must not fail or create state.
    let out = run(dir.path(), &["status"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(!stdout(&out).trim().is_empty(), "status prints a report");
    assert!(
        !manifest_path(dir.path()).exists(),
        "status must not initialize a workspace"
    );
    // Fresh process after init must keep reporting successfully; status is
    // not pinned byte-for-byte across the init boundary so the report can
    // legitimately grow workspace facts later.
    let init = run(dir.path(), &["init"]);
    assert!(init.status.success());
    let out2 = run(dir.path(), &["status"]);
    assert!(out2.status.success());
    assert!(!stdout(&out2).trim().is_empty(), "status prints a report");
    // Idempotent repeated invocation (same state, deterministic bytes).
    let out3 = run(dir.path(), &["status"]);
    assert!(out3.status.success());
    assert_eq!(stdout(&out3), stdout(&out2));
}

// ---------------------------------------------------------------------------
// init: path handling (spaces, unicode, nested, relative)
// ---------------------------------------------------------------------------

#[test]
fn init_handles_directory_names_with_spaces() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("my awh project");
    let out = run(dir.path(), &["init", "--path", "my awh project"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(manifest_path(&root).exists(), "manifest under spaced path");
    assert!(stdout(&out).contains("initialized workspace"));
    let out2 = run(&root, &["init"]);
    assert!(out2.status.success());
    assert!(stdout(&out2).contains("already initialized"));
}

#[test]
fn init_handles_unicode_directory_names() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("проект-π-日本語");
    let out = run(dir.path(), &["init", "--path", "проект-π-日本語"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(manifest_path(&root).exists(), "manifest under unicode path");
    // Re-init from inside the unicode dir.
    let out2 = run(&root, &["init"]);
    assert!(out2.status.success());
    assert!(stdout(&out2).contains("already initialized"));
}

#[test]
fn init_creates_deeply_nested_targets_and_preserves_unrelated_files() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "keep me").unwrap();
    let nested = "a/b/c/d/e/f/g/h/workspace";
    let out = run(dir.path(), &["init", "--path", nested]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let root = dir.path().join(nested);
    assert!(manifest_path(&root).exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("README.md")).unwrap(),
        "keep me",
        "unrelated files must be preserved"
    );
    // Lexical vs canonical: `.` still resolves to the same identity.
    let out2 = run(&root, &["init", "--path", "."]);
    assert!(out2.status.success());
    assert!(stdout(&out2).contains("already initialized"));
}

#[test]
fn init_resolves_traversal_paths_exactly_where_the_user_asked() {
    let dir = tempdir().expect("tempdir");
    // `--path <name>/../escape` run from <dir> resolves lexically to
    // <dir>/escape. Init must create the manifest exactly there (the
    // resolved target), never in an implicit location.
    let dir_name = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let rel = format!("{dir_name}/../escape");
    let out = run(dir.path(), &["init", "--path", &rel]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let expected = dir.path().join("escape");
    assert!(
        manifest_path(&expected).exists(),
        "init must target the resolved path, not an implicit one"
    );
}

#[test]
fn init_fails_cleanly_on_unusable_root() {
    // A plain FILE as a path component makes the requested root impossible
    // to create (ENOTDIR) on every platform — no reliance on /proc or
    // runner privilege, so the failure is deterministic everywhere.
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("blocker"), "a plain file").unwrap();
    let target = if cfg!(windows) {
        "blocker\\not-writable-awh"
    } else {
        "blocker/not-writable-awh"
    };
    let out = run(dir.path(), &["init", "--path", target]);
    assert!(
        !out.status.success(),
        "init must fail on an unusable root, stdout: {}",
        stdout(&out)
    );
    let err = stderr(&out);
    assert!(!err.contains("panicked"), "got panic: {err}");
    assert!(
        err.to_lowercase().contains("failed to create workspace"),
        "error must name the failure, got: {err}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn init_fails_cleanly_on_unwritable_proc_root() {
    // /proc rejects mkdir even for root, giving a deterministic
    // unwritable-location failure that does not depend on dropping
    // privileges (tests may run as root in containers). Linux-only: /proc
    // does not exist on macOS and resolves to a writable drive path on
    // Windows, where this contract is covered by
    // init_fails_cleanly_on_unusable_root instead.
    let dir = tempdir().expect("tempdir");
    let out = run(dir.path(), &["init", "--path", "/proc/not-writable-awh"]);
    assert!(
        !out.status.success(),
        "init must fail on an unwritable root, stdout: {}",
        stdout(&out)
    );
    let err = stderr(&out);
    assert!(!err.contains("panicked"), "got panic: {err}");
    assert!(
        err.to_lowercase().contains("failed to create workspace"),
        "error must name the failure, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Configuration precedence: defaults < AWH_* env < CLI flags
// ---------------------------------------------------------------------------

/// Holds a TCP port occupied for the lifetime of the returned listener, so a
/// child's bind on that port deterministically fails with EADDRINUSE (the
/// OS picks a free port via port 0; the listener stays alive until dropped).
fn hold_port() -> (u16, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a port to hold");
    let port = listener.local_addr().expect("addr").port();
    (port, listener)
}

#[test]
fn sse_invalid_awh_port_fails_closed_with_clear_error() {
    // Regression: AWH_PORT=not-a-number previously fell back silently to
    // the 8443 default. The documented contract (docs/configuration.md)
    // says invalid values are never silently ignored.
    let dir = tempdir().expect("tempdir");
    let out = run_env(
        dir.path(),
        &["mcp", "serve", "--transport", "sse"],
        &[
            ("AWH_PORT", "not-a-number"),
            ("AWH_API_KEY", SYNTHETIC_SECRET),
        ],
    );
    assert!(
        !out.status.success(),
        "invalid AWH_PORT must fail, stdout: {}",
        stdout(&out)
    );
    let err = stderr(&out);
    assert!(
        err.contains("AWH_PORT"),
        "error must name the offending variable, got: {err}"
    );
    // No server output claiming it started, and no secret in the error.
    assert!(!err.contains(SYNTHETIC_SECRET));
    assert!(!stdout(&out).contains(SYNTHETIC_SECRET));
}

#[test]
fn sse_awh_port_override_is_applied_and_bind_fails_when_occupied() {
    let dir = tempdir().expect("tempdir");
    let (held, listener) = hold_port();
    let out = run_env(
        dir.path(),
        &["mcp", "serve", "--transport", "sse", "--host", "127.0.0.1"],
        &[
            ("AWH_PORT", &held.to_string()),
            ("AWH_API_KEY", SYNTHETIC_SECRET),
        ],
    );
    assert!(
        !out.status.success(),
        "bind on occupied port must fail, stdout: {}",
        stdout(&out)
    );
    let err = stderr(&out);
    assert!(
        err.contains(&held.to_string()),
        "error must reference the AWH_PORT override {held}, got: {err}"
    );
    drop(listener);
}

#[test]
fn sse_cli_flag_overrides_awh_env_for_port() {
    // Precedence proof: AWH_PORT points at a free port, but --port points at
    // an occupied one. The CLI flag must win — the error must mention the
    // CLI port, proving the env value did not silently take precedence.
    let dir = tempdir().expect("tempdir");
    let (occupied, listener) = hold_port();
    // A second port that is definitely free right now: bind it to pick, then
    // release it for the child to (attempt to) use.
    let (free, held_free) = hold_port();
    drop(held_free);
    let out = run_env(
        dir.path(),
        &[
            "mcp",
            "serve",
            "--transport",
            "sse",
            "--host",
            "127.0.0.1",
            "--port",
            &occupied.to_string(),
        ],
        &[
            ("AWH_PORT", &free.to_string()),
            ("AWH_API_KEY", SYNTHETIC_SECRET),
        ],
    );
    assert!(!out.status.success(), "stdout: {}", stdout(&out));
    let err = stderr(&out);
    assert!(
        err.contains(&occupied.to_string()),
        "CLI --port must win over AWH_PORT; error should mention {occupied}, got: {err}"
    );
    assert!(
        !err.contains(&format!(":{free}")),
        "env port {free} must not be used when --port is given; got: {err}"
    );
    drop(listener);
}

#[test]
fn sse_invalid_awh_host_fails_deterministically_at_bind() {
    let dir = tempdir().expect("tempdir");
    let out = run_env(
        dir.path(),
        &["mcp", "serve", "--transport", "sse", "--port", "9"],
        &[
            ("AWH_HOST", "not a valid host"),
            ("AWH_API_KEY", SYNTHETIC_SECRET),
        ],
    );
    assert!(
        !out.status.success(),
        "invalid host must fail (bind error), stdout: {}",
        stdout(&out)
    );
    assert!(!stderr(&out).contains(SYNTHETIC_SECRET));
}

#[test]
fn sse_missing_or_empty_api_key_fails_closed_before_serving() {
    let dir = tempdir().expect("tempdir");
    // Missing key.
    let out = run_env(
        dir.path(),
        &["mcp", "serve", "--transport", "sse", "--port", "9"],
        &[],
    );
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains("AWH_API_KEY"),
        "error must name the required variable, got: {err}"
    );
    // Empty key.
    let out = run_env(
        dir.path(),
        &["mcp", "serve", "--transport", "sse", "--port", "9"],
        &[("AWH_API_KEY", "   ")],
    );
    assert!(!out.status.success());
    assert!(stderr(&out).contains("AWH_API_KEY"));
    // No listening banner was printed in either case.
    assert!(!stdout(&out).contains("listening"));
}

#[test]
fn sse_awh_port_boundary_values_fail_or_parse_deterministically() {
    let dir = tempdir().expect("tempdir");
    // Values outside u16 (max+1, negative, huge, non-numeric) must fail
    // closed with an error naming AWH_PORT — never a silent fallback.
    for bad in ["65536", "-1", "99999999999999999999", "not-a-number"] {
        let out = run_env(
            dir.path(),
            &["mcp", "serve", "--transport", "sse"],
            &[("AWH_PORT", bad), ("AWH_API_KEY", SYNTHETIC_SECRET)],
        );
        assert!(
            !out.status.success(),
            "AWH_PORT={bad:?} must fail closed, stdout: {}",
            stdout(&out)
        );
        assert!(
            stderr(&out).contains("AWH_PORT"),
            "AWH_PORT={bad:?}: error must name the variable, got: {}",
            stderr(&out)
        );
    }
    // Whitespace around a valid number is accepted (trimmed). The proof must
    // never start a real server: with the API key absent, startup fails right
    // AFTER config parsing — so "no AWH_PORT error, only the API-key error"
    // proves the trimmed value parsed, and the process cannot reach bind.
    let out = run_env(
        dir.path(),
        &["mcp", "serve", "--transport", "sse"],
        &[("AWH_PORT", " 65535 ")],
    );
    assert!(
        !out.status.success(),
        "missing API key must fail the run, stdout: {}",
        stdout(&out)
    );
    let err = stderr(&out);
    assert!(
        !err.contains("AWH_PORT"),
        "trimmed valid AWH_PORT must parse; got: {err}"
    );
    assert!(
        err.contains("AWH_API_KEY"),
        "the failure must be the API key, proving config parsing passed; got: {err}"
    );
    // A set-but-empty (or whitespace-only) AWH_PORT is treated as unset:
    // startup proceeds on the default, so the first failure is the API key —
    // the common `AWH_PORT="${SOME_PORT:-}"` / env_file pattern must not
    // abort the server (docs/configuration.md).
    for empty in ["", "  "] {
        let out = run_env(
            dir.path(),
            &["mcp", "serve", "--transport", "sse"],
            &[("AWH_PORT", empty)],
        );
        assert!(
            !out.status.success(),
            "missing API key must fail the run (AWH_PORT={empty:?} is unset-like), stdout: {}",
            stdout(&out)
        );
        let err = stderr(&out);
        assert!(
            !err.contains("AWH_PORT"),
            "empty AWH_PORT must behave as unset, not fail closed; got: {err}"
        );
        assert!(
            err.contains("AWH_API_KEY"),
            "the failure must be the API key, proving empty AWH_PORT did not abort config; got: {err}"
        );
    }
}

// ---------------------------------------------------------------------------
// TLS half-configuration fails closed
// ---------------------------------------------------------------------------

#[test]
fn sse_tls_half_configured_fails_closed_both_ways() {
    let dir = tempdir().expect("tempdir");
    let cert = dir.path().join("cert.pem");
    fs::write(&cert, "fake cert").unwrap();
    let key = dir.path().join("key.pem");
    fs::write(&key, "fake key").unwrap();

    // Cert without key.
    let out = run_env(
        dir.path(),
        &["mcp", "serve", "--transport", "sse", "--port", "9"],
        &[
            ("AWH_TLS_CERT", cert.to_str().unwrap()),
            ("AWH_API_KEY", SYNTHETIC_SECRET),
        ],
    );
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains("private key") || err.contains("tls_cert_and_key_required"),
        "TLS half-config error must be explicit, got: {err}"
    );
    assert!(
        err.contains("AWH_TLS_KEY"),
        "error names the missing var: {err}"
    );

    // Key without cert.
    let out = run_env(
        dir.path(),
        &["mcp", "serve", "--transport", "sse", "--port", "9"],
        &[
            ("AWH_TLS_KEY", key.to_str().unwrap()),
            ("AWH_API_KEY", SYNTHETIC_SECRET),
        ],
    );
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains("certificate") || err.contains("tls_cert_and_key_required"),
        "TLS half-config error must be explicit, got: {err}"
    );
    assert!(
        err.contains("AWH_TLS_CERT"),
        "error names the missing var: {err}"
    );

    // Both set but pointing at invalid PEM material: startup must fail
    // rather than silently serving plain HTTP.
    let out = run_env(
        dir.path(),
        &[
            "mcp",
            "serve",
            "--transport",
            "sse",
            "--host",
            "127.0.0.1",
            "--port",
            "9",
        ],
        &[
            ("AWH_TLS_CERT", cert.to_str().unwrap()),
            ("AWH_TLS_KEY", key.to_str().unwrap()),
            ("AWH_API_KEY", SYNTHETIC_SECRET),
        ],
    );
    assert!(
        !out.status.success(),
        "invalid TLS material must not silently downgrade to plain HTTP, stdout: {}",
        stdout(&out)
    );
}

// ---------------------------------------------------------------------------
// Resource-limit env vars: rejected loudly, fail-safe default
// ---------------------------------------------------------------------------

#[test]
fn stdio_server_reports_invalid_resource_limit_and_keeps_serving() {
    // AWH_MAX_MCP_LINE_BYTES=abc must be rejected with a config_invalid
    // tracing event (stderr) while the server still runs on conservative
    // defaults — "never silently ignored" per docs/configuration.md.
    let dir = tempdir().expect("tempdir");
    assert!(run(dir.path(), &["init"]).status.success());
    let mut cmd = Command::new(BIN);
    cmd.args(["mcp", "serve", "--transport", "stdio"])
        .current_dir(dir.path())
        // Sanitize the environment the way run_env does, plus the provider
        // credentials, so no machine-local state (GITHUB_TOKEN, COMPOSIO_API_KEY,
        // leftover AWH_* values) can change what this test observes.
        .env_remove("AWH_HOST")
        .env_remove("AWH_PORT")
        .env_remove("AWH_TLS_CERT")
        .env_remove("AWH_TLS_KEY")
        .env_remove("AWH_API_KEY")
        .env_remove("AWH_ALLOWED_ORIGINS")
        .env_remove("GITHUB_TOKEN")
        .env_remove("COMPOSIO_API_KEY")
        .env("AWH_MAX_MCP_LINE_BYTES", "not-a-number")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().expect("spawn stdio server");

    // Drain stderr on a dedicated thread BEFORE waiting: a chatty child
    // could otherwise fill the ~64 KiB pipe buffer, block on write, and
    // deadlock the test in wait().
    let stderr_pipe = child.stderr.take().expect("stderr piped");
    let stderr_handle = std::thread::spawn(move || {
        let mut s = stderr_pipe;
        let mut buf = String::new();
        use std::io::Read;
        let _ = s.read_to_string(&mut buf);
        buf
    });

    // A valid initialize request must still complete on stdout.
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18","capabilities":{{}},"clientInfo":{{"name":"tp01","version":"0"}}}}}}"#
        )
        .expect("write init");
    }
    let first = read_stdout_line(&mut child);
    assert!(
        first.contains(r#""id":1"#) && first.contains("result"),
        "initialize must succeed on fail-safe defaults, got: {first}"
    );
    // Cleanup: close stdin (EOF shutdown) and reap.
    drop(child.stdin.take());
    let _ = child.wait();
    let err = stderr_handle.join().expect("stderr drain thread");
    assert!(
        err.contains("config_invalid") && err.contains("AWH_MAX_MCP_LINE_BYTES"),
        "invalid limit must be reported as config_invalid naming the variable, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Persistence across fresh processes / isolation / concurrency
// ---------------------------------------------------------------------------

#[test]
fn foundation_state_persists_across_fresh_processes() {
    let dir = tempdir().expect("tempdir");
    let out = run(dir.path(), &["init"]);
    assert!(out.status.success());
    let id: String = stdout(&out)
        .lines()
        .find(|l| l.contains("workspace id:"))
        .map(|l| l.split("workspace id:").nth(1).unwrap().trim().to_string())
        .expect("workspace id line");

    // Three fresh processes must all observe the same durable identity.
    for _ in 0..3 {
        let again = run(dir.path(), &["init"]);
        assert!(again.status.success());
        assert!(stdout(&again).contains("already initialized"));
        assert!(
            stdout(&again).contains(&id),
            "identity must be stable across fresh processes"
        );
    }
    let manifest = fs::read_to_string(manifest_path(dir.path())).expect("manifest readable");
    assert!(manifest.contains(&id), "durable manifest carries the id");
}

#[test]
fn two_workspaces_initialize_concurrently_and_stay_isolated() {
    let dir = tempdir().expect("tempdir");
    let a = dir.path().join("ws-a");
    let b = dir.path().join("ws-b");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();

    let a2 = a.clone();
    let b2 = b.clone();
    let handle_a = std::thread::spawn(move || run(&a2, &["init"]));
    let handle_b = std::thread::spawn(move || run(&b2, &["init"]));
    let out_a = handle_a.join().unwrap();
    let out_b = handle_b.join().unwrap();
    assert!(out_a.status.success(), "stderr: {}", stderr(&out_a));
    assert!(out_b.status.success(), "stderr: {}", stderr(&out_b));

    let id_of = |o: &Output| {
        stdout(o)
            .lines()
            .find(|l| l.contains("workspace id:"))
            .map(|l| l.split("workspace id:").nth(1).unwrap().trim().to_string())
            .expect("id")
    };
    let id_a = id_of(&out_a);
    let id_b = id_of(&out_b);
    assert_ne!(id_a, id_b, "independent workspaces get distinct ids");

    // Re-init in A must not change B's manifest bytes.
    let b_bytes = fs::read(manifest_path(&b)).unwrap();
    let re = run(&a, &["init"]);
    assert!(re.status.success());
    assert_eq!(fs::read(manifest_path(&b)).unwrap(), b_bytes);
    assert!(manifest_path(&a).exists());
}

// ---------------------------------------------------------------------------
// Secret-safety sweep across foundation surfaces
// ---------------------------------------------------------------------------

#[test]
fn foundation_outputs_never_leak_token_shaped_secrets() {
    let dir = tempdir().expect("tempdir");
    assert!(run(dir.path(), &["init"]).status.success());

    let surfaces: Vec<Vec<&str>> = vec![
        vec!["--help"],
        vec!["status"],
        vec!["--version"],
        vec!["init"],
        vec!["mcp", "--help"],
        vec!["mcp", "serve", "--help"],
        vec!["agent", "--help"],
    ];
    for args in surfaces {
        let out = run_env(dir.path(), &args, &[("AWH_API_KEY", SYNTHETIC_SECRET)]);
        // Some surfaces exit non-zero (mcp serve --help exits 0; init is
        // idempotent) — leak-safety is the invariant under test here.
        let text = stdout(&out);
        let err = stderr(&out);
        assert!(
            !text.contains(SYNTHETIC_SECRET) && !err.contains(SYNTHETIC_SECRET),
            "secret leaked via {args:?}: {text} | {err}"
        );
    }
}

// ---------------------------------------------------------------------------
// Deterministic failure behavior (no panic / no backtrace)
// ---------------------------------------------------------------------------

#[test]
fn foundation_failures_are_clean_errors_not_panics() {
    let dir = tempdir().expect("tempdir");
    // File as workspace root via explicit --path.
    let file = dir.path().join("plain-file");
    fs::write(&file, "x").unwrap();
    let out = run(dir.path(), &["init", "--path", "plain-file"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(!err.contains("panicked"), "got panic: {err}");
    assert!(!err.contains("RUST_BACKTRACE"), "got backtrace: {err}");
    assert!(err.to_lowercase().contains("not a directory"));

    // Unknown subcommand: clap usage error, non-zero, structured.
    let out = run(dir.path(), &["definitely-not-a-command"]);
    assert!(!out.status.success());
    assert!(!stdout(&out).contains("panicked"));
    let err = stderr(&out);
    assert!(
        err.contains("Usage") || err.contains("unrecognized"),
        "got: {err}"
    );
}
