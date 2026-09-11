//! End-to-end CLI tests for `awh agent` against the real compiled binary.
//!
//! These exercise the full `agent create → list → grant → inspect → revoke`
//! lifecycle over a temp workspace directory, verifying both the plain-text
//! output and the durable side effects written under `.agent/`.

use std::process::Command;
use tempfile::tempdir;

/// Runs `awh <args>` inside `dir` and returns (exit-success, stdout, stderr).
fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn awh binary");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn agent_lifecycle_create_list_grant_inspect_revoke() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // create: plain-text confirmation, status starts at `created`.
    let (ok, out, err) = run(
        root,
        &["agent", "create", "writer", "Writer", "--role", "writer"],
    );
    assert!(ok, "agent create failed: {err}");
    assert!(out.contains("created agent writer (Writer)"), "got: {out}");
    assert!(out.contains("status: created"), "got: {out}");
    let agent_file = root.join(".agent").join("agents").join("writer.json");
    assert!(agent_file.exists(), "agent record must be persisted");

    // create a second agent so `list` has a real sort to do.
    run(
        root,
        &["agent", "create", "alpha", "Alpha", "--role", "researcher"],
    );
    let (ok, out, err) = run(root, &["agent", "list"]);
    assert!(ok, "agent list failed: {err}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "both agents listed, got: {out}");
    assert!(
        lines[0].starts_with("alpha "),
        "list must be sorted by id, got: {out}"
    );
    assert!(lines[1].starts_with("writer "), "got: {out}");
    assert!(
        lines[1].contains("created"),
        "status must be printed, got: {out}"
    );

    // grant: writes a capability grant linked to the agent.
    let (ok, out, err) = run(
        root,
        &[
            "agent",
            "grant",
            "writer",
            "--permission",
            "filesystem",
            "--scope",
            "src/",
        ],
    );
    assert!(ok, "agent grant failed: {err}");
    assert!(out.contains("granted filesystem to writer"), "got: {out}");
    let grant_file = root
        .join(".agent")
        .join("capabilities")
        .join("writer-filesystem.json");
    assert!(grant_file.exists(), "grant record must be persisted");

    // inspect: shows the agent AND its grants.
    let (ok, out, err) = run(root, &["agent", "inspect", "writer"]);
    assert!(ok, "agent inspect failed: {err}");
    assert!(out.contains("id: writer"), "got: {out}");
    assert!(out.contains("role: writer"), "got: {out}");
    assert!(out.contains("grants (1)"), "got: {out}");
    assert!(out.contains("writer-filesystem"), "got: {out}");
    assert!(out.contains("scope: src/"), "got: {out}");

    // a second grant stacks on the same agent.
    let (ok, second_out, err) = run(
        root,
        &["agent", "grant", "writer", "--permission", "network"],
    );
    assert!(ok, "second grant failed: {err}");
    assert!(
        second_out.contains("granted network to writer"),
        "got: {second_out}"
    );
    let (ok, out, _) = run(root, &["agent", "inspect", "writer"]);
    assert!(ok && out.contains("grants (2)"), "got: {out}");

    // revoke: removes the grant; a second revoke reports it was missing.
    let (ok, out, err) = run(root, &["agent", "revoke", "writer-filesystem"]);
    assert!(ok, "agent revoke failed: {err}");
    assert!(
        out.contains("revoked grant: writer-filesystem"),
        "got: {out}"
    );
    assert!(!grant_file.exists(), "grant file must be deleted");
    let (ok, out, _) = run(root, &["agent", "revoke", "writer-filesystem"]);
    assert!(ok, "revoking a missing grant must not fail");
    assert!(
        out.contains("grant not found: writer-filesystem"),
        "got: {out}"
    );

    // after revocation, inspect shows the remaining grant only.
    let (ok, out, _) = run(root, &["agent", "inspect", "writer"]);
    assert!(ok && out.contains("grants (1)"), "got: {out}");
    assert!(out.contains("network"), "got: {out}");
}

#[test]
fn agent_grant_rejects_invalid_permission_and_missing_agent() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // grant for an unknown agent fails.
    let (ok, out, err) = run(
        root,
        &["agent", "grant", "ghost", "--permission", "network"],
    );
    assert!(!ok, "grant to a missing agent must fail: {out}{err}");
    assert!(err.contains("agent not found: ghost"), "got: {err}");

    // invalid permission label fails with the accepted vocabulary.
    run(
        root,
        &["agent", "create", "writer", "Writer", "--role", "writer"],
    );
    let (ok, out, err) = run(
        root,
        &["agent", "grant", "writer", "--permission", "nonsense"],
    );
    assert!(!ok, "invalid permission must fail: {out}{err}");
    assert!(
        err.contains("invalid permission \"nonsense\""),
        "error must name the bad value: {err}"
    );
    assert!(
        err.contains("filesystem"),
        "error must list valid options: {err}"
    );

    // inspect for an unknown agent fails with a clear error.
    let (ok, out, err) = run(root, &["agent", "inspect", "ghost"]);
    assert!(!ok, "inspect of a missing agent must fail: {out}{err}");
    assert!(err.contains("agent not found: ghost"), "got: {err}");
}

#[test]
fn agent_create_rejects_unsafe_ids() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    let (ok, out, err) = run(
        root,
        &["agent", "create", "../escape", "Bad", "--role", "writer"],
    );
    assert!(!ok, "traversal id must fail: {out}{err}");
    assert!(err.contains("invalid agent id"), "got: {err}");
    let (ok, _, _) = run(root, &["agent", "create", "a/b", "Bad", "--role", "writer"]);
    assert!(!ok, "separator id must fail");
    // nothing escaped into the store.
    let agents_dir = root.join(".agent").join("agents");
    assert!(!agents_dir.join("escape.json").exists());
    assert!(
        !root.join("escape.json").exists(),
        "no file written outside .agent"
    );
}
