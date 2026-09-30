//! End-to-end CLI tests for the MEM-001 memory surface: `awh memory
//! list|get|search|add|update|delete` against the real compiled binary.
//!
//! Every `run` is a fresh process, so `.agent/memory.json` persistence and
//! reload across invocations are exercised implicitly. Scope filtering,
//! partial-update semantics, safe-delete fail-closed behavior, bounded
//! output, and durable audit events are pinned here.

use std::process::{Command, Stdio};
use tempfile::tempdir;

/// Runs `awh <args>` inside `dir` with optional stdin, returning
/// (exit-success, stdout, stderr).
fn run_with_stdin(dir: &std::path::Path, args: &[&str], stdin: &str) -> (bool, String, String) {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn awh binary");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait for awh");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    run_with_stdin(dir, args, "")
}

/// Extracts the minted id from `memory add` output.
fn id_from_add(out: &str) -> String {
    out.lines()
        .find_map(|l| l.strip_prefix("added memory entry "))
        .and_then(|l| l.split(' ').next())
        .expect("id in add output")
        .to_owned()
}

#[test]
fn memory_add_list_get_search_delete_lifecycle() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // add with explicit content, scope, tags
    let (ok, out, err) = run(
        root,
        &[
            "memory",
            "add",
            "--content",
            "cargo workspace layout",
            "--scope",
            "Project",
            "--tag",
            "build",
            "--tag",
            "rust",
        ],
    );
    assert!(ok, "memory add failed: {err}");
    let id = id_from_add(&out);
    assert!(out.contains("scope Project"), "got: {out}");

    // add a second one via stdin at Global scope
    let (ok, out, err) = run_with_stdin(
        root,
        &["memory", "add", "--scope", "Global"],
        "global convention notes",
    );
    assert!(ok, "memory add 2 failed: {err}");
    let global_id = id_from_add(&out);

    // list shows both, tab-separated rows
    let (ok, out, err) = run(root, &["memory", "list"]);
    assert!(ok, "memory list failed: {err}");
    assert!(out.contains(&id), "got: {out}");
    assert!(out.contains(&global_id), "got: {out}");
    assert!(out.contains("cargo workspace layout"), "got: {out}");

    // list --scope Global shows only the global entry
    let (ok, out, err) = run(root, &["memory", "list", "--scope", "Global"]);
    assert!(ok, "memory list --scope failed: {err}");
    assert!(out.contains(&global_id), "got: {out}");
    assert!(
        !out.contains(&id),
        "project entry leaked into Global: {out}"
    );

    // get returns the JSON record
    let (ok, out, err) = run(root, &["memory", "get", "--id", &id]);
    assert!(ok, "memory get failed: {err}");
    assert!(
        out.contains("\"content\": \"cargo workspace layout\""),
        "got: {out}"
    );
    assert!(out.contains("\"tags\""), "got: {out}");

    // search by content substring
    let (ok, out, err) = run(root, &["memory", "search", "--query", "workspace"]);
    assert!(ok, "memory search failed: {err}");
    assert!(out.contains(&id), "got: {out}");
    assert!(!out.contains(&global_id), "got: {out}");

    // search by tag
    let (ok, out, err) = run(root, &["memory", "search", "--query", "rust"]);
    assert!(ok, "tag search failed: {err}");
    assert!(out.contains(&id), "got: {out}");

    // delete then fail-closed get
    let (ok, out, err) = run(root, &["memory", "delete", "--id", &id]);
    assert!(ok, "memory delete failed: {err}");
    assert!(
        out.contains(&format!("deleted memory entry {id}")),
        "got: {out}"
    );
    let (ok, _, err) = run(root, &["memory", "get", "--id", &id]);
    assert!(!ok, "get after delete must fail");
    assert!(err.contains("not found"), "got: {err}");
    let (ok, _, err) = run(root, &["memory", "delete", "--id", &id]);
    assert!(!ok, "second delete must fail closed");
    assert!(err.contains("not found"), "got: {err}");
}

#[test]
fn memory_update_is_partial_and_fails_closed() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    let (ok, out, err) = run(
        root,
        &[
            "memory",
            "add",
            "--content",
            "original content",
            "--tag",
            "before",
        ],
    );
    assert!(ok, "add failed: {err}");
    let id = id_from_add(&out);

    // content-only update keeps tags
    let (ok, out, err) = run(
        root,
        &[
            "memory",
            "update",
            "--id",
            &id,
            "--content",
            "updated content",
        ],
    );
    assert!(ok, "update failed: {err}");
    assert!(
        out.contains(&format!("updated memory entry {id}")),
        "got: {out}"
    );
    let (ok, out, _) = run(root, &["memory", "get", "--id", &id]);
    assert!(ok && out.contains("updated content"), "got: {out}");
    assert!(out.contains("before"), "tags must be preserved: {out}");

    // tags-only update replaces wholesale, keeps content
    let (ok, _, err) = run(root, &["memory", "update", "--id", &id, "--tag", "after"]);
    assert!(ok, "tag update failed: {err}");
    let (ok, out, _) = run(root, &["memory", "get", "--id", &id]);
    assert!(ok && out.contains("after"), "got: {out}");
    assert!(!out.contains("before"), "old tag must be gone: {out}");

    // update with no fields is a usage error (clap required_unless_present_any)
    let (ok, _, _) = run(root, &["memory", "update", "--id", &id]);
    assert!(!ok, "fieldless update must fail with usage error");

    // update unknown id fails closed
    let (ok, _, err) = run(
        root,
        &["memory", "update", "--id", "ghost", "--content", "x"],
    );
    assert!(!ok, "update unknown id must fail");
    assert!(err.contains("not found"), "got: {err}");
}

#[test]
fn memory_validation_and_corruption_fail_closed() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // invalid scope rejected with the canonical MCP error text
    let (ok, _, err) = run(
        root,
        &["memory", "add", "--content", "x", "--scope", "Universe"],
    );
    assert!(!ok, "invalid scope must fail");
    assert!(err.contains("invalid scope 'Universe'"), "got: {err}");

    // empty content rejected
    let (ok, ..) = run(root, &["memory", "add", "--content", ""]);
    assert!(!ok, "empty content must fail");

    // oversized content rejected (store cap 1 MiB) — pipe it via stdin so
    // the argument list stays within OS limits while exceeding the store cap.
    let big = "a".repeat(1024 * 1024 + 10);
    let (ok, _, err) = run_with_stdin(root, &["memory", "add"], &big);
    assert!(!ok, "oversized content must fail");
    assert!(err.contains("exceeds"), "got: {err}");

    // corrupt store fails closed on every operation, never silently empty
    std::fs::create_dir_all(root.join(".agent")).unwrap();
    std::fs::write(root.join(".agent/memory.json"), "{ broken").unwrap();
    let (ok, _, err) = run(root, &["memory", "list"]);
    assert!(!ok, "corrupt store must fail closed");
    assert!(!err.is_empty(), "expected an error message");
}

#[test]
fn memory_add_is_durable_and_audited() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    let (ok, out, err) = run(root, &["memory", "add", "--content", "durable note"]);
    assert!(ok, "add failed: {err}");
    let _id = id_from_add(&out);

    // persistence: .agent/memory.json exists and holds the entry
    let raw = std::fs::read_to_string(root.join(".agent/memory.json")).unwrap();
    assert!(raw.contains("durable note"), "got: {raw}");

    // mutation landed in the durable audit log
    let audit = std::fs::read_to_string(root.join(".agent/audit/audit.log")).unwrap();
    assert!(
        audit.contains("cli_memory_add"),
        "audit log must record cli_memory_add: {audit}"
    );
    // content never appears in the audit trail
    assert!(
        !audit.contains("durable note"),
        "audit must not leak raw memory content"
    );
}
