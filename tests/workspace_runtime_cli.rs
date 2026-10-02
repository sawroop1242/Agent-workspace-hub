//! TP02 — Workspace Runtime: human-behavior CLI tests for the workspace
//! lifecycle contract on the real compiled binary.
//!
//! Scope: the workspace lifecycle/identity boundary itself (`awh init`,
//! durable `.agent/workspace.json` manifest, canonical-root binding,
//! restart persistence, isolation, fail-closed state handling). Sibling
//! suites already cover the initialize/race basics (`tests/init_cli.rs`)
//! and the service internals (`src/services/init.rs` tests); this suite
//! adds the TP02-required workflows that were previously untested at the
//! CLI boundary: unrelated-file preservation, path normalization edges,
//! moved-workspace rejection, two-workspace isolation, and the current
//! `awh status` bootstrap contract (per docs/error.md §6).
//!
//! The final-target `awh workspace create|list|open|info|remove` commands
//! are NOT implemented on this branch: `awh init [--path]` is the create
//! equivalent, and no fake tests pretend otherwise (see
//! docs/testing-prompts/reports/02-workspace-runtime-report.md).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

/// Runs `awh <args>` inside `dir`, returns (exit-success, stdout, stderr).
fn run(dir: &Path, args: &[&str]) -> (bool, String, String) {
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

fn manifest_path(root: &Path) -> PathBuf {
    root.join(".agent").join("workspace.json")
}

fn init_ok(dir: &Path, path: &str) -> PathBuf {
    let (ok, out, err) = run(dir, &["init", "--path", path]);
    assert!(ok, "init {path} failed: {err}\nstdout: {out}");
    assert!(out.contains("initialized workspace"), "got: {out}");
    PathBuf::from(dir).join(path)
}

/// Recursively snapshots every regular file's bytes under `root`.
fn tree_snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .expect("read_dir")
            .map(|e| e.expect("dir entry"))
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push((path.clone(), fs::read(&path).expect("read file")));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

#[test]
fn init_preserves_unrelated_files_byte_for_byte() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::write(root.join("README.md"), "# project\n").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("notes.bin"), [0u8, 1, 2, 250, 251]).unwrap();

    let before = tree_snapshot(root);
    let (ok, out, err) = run(root, &["init"]);
    assert!(ok, "init failed: {err}\nstdout: {out}");

    let after = tree_snapshot(root);
    for (path, bytes) in &before {
        let current = fs::read(path).expect("unrelated file must survive init");
        assert_eq!(&current, bytes, "init modified unrelated file {path:?}");
    }
    // Init state is purely additive: everything that existed still exists.
    assert!(after.len() > before.len());
    assert!(manifest_path(root).exists());
}

#[test]
fn init_creates_missing_parent_directories_and_binds_that_root() {
    let dir = tempdir().expect("tempdir");
    let target = dir.path().join("a/b/c");
    let root = init_ok(dir.path(), "a/b/c");

    assert!(manifest_path(&root).exists(), "state under a/b/c");
    // The recorded root is the canonical target root, not the CWD and not
    // an intermediate directory. Compare canonical-to-canonical (the
    // parsed manifest VALUE against the canonicalized path): raw-text
    // containment falsely fails on Windows, where canonical paths carry
    // backslashes that JSON-escape inside the file bytes.
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(manifest_path(&root)).unwrap()).unwrap();
    let canonical = target.canonicalize().unwrap();
    assert_eq!(
        manifest["workspace_root"].as_str().expect("root string"),
        canonical.to_str().expect("utf8 path"),
        "manifest must record the canonical target root"
    );
    // No state leaks into any intermediate directory or the CWD.
    for intermediate in [dir.path(), &dir.path().join("a"), &dir.path().join("a/b")] {
        assert!(
            !manifest_path(intermediate).exists(),
            "state leaked into {:?}",
            intermediate
        );
    }
}

#[test]
fn init_accepts_trailing_slash_and_dot_path_forms() {
    let dir = tempdir().expect("tempdir");
    let root = init_ok(dir.path(), "ws/");
    assert!(manifest_path(&root).exists());
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(manifest_path(&root)).unwrap()).unwrap();
    let canonical = dir.path().join("ws").canonicalize().unwrap();
    assert_eq!(
        manifest["workspace_root"].as_str().expect("root string"),
        canonical.to_str().expect("utf8 path")
    );

    let dir2 = tempdir().expect("tempdir");
    let root2 = init_ok(dir2.path(), ".");
    assert!(manifest_path(&root2).exists());
    let manifest2: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(manifest_path(&root2)).unwrap()).unwrap();
    assert_eq!(
        manifest2["workspace_root"].as_str().expect("root string"),
        dir2.path()
            .canonicalize()
            .unwrap()
            .to_str()
            .expect("utf8 path"),
        "dot path must bind the canonical CWD"
    );
}

#[test]
fn moved_workspace_is_rejected_without_reset() {
    // Canonical-root binding: the identity follows the root, not the
    // directory. Renaming an initialized workspace away from its recorded
    // root must fail closed (never silently re-bind the identity).
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("ws");
    fs::create_dir_all(&root).unwrap();
    let (ok, _, err) = run(&root, &["init"]);
    assert!(ok, "{err}");
    let bytes = fs::read(manifest_path(&root)).unwrap();

    let moved = dir.path().join("ws-moved");
    fs::rename(&root, &moved).unwrap();
    let (ok, out, err) = run(&moved, &["init"]);
    assert!(!ok, "moved workspace must not re-init, got: {out}");
    assert!(
        err.contains("recorded workspace root does not resolve"),
        "got: {err}"
    );
    // Fail closed: the foreign state was neither rewritten nor reset.
    assert_eq!(fs::read(manifest_path(&moved)).unwrap(), bytes);
}

#[test]
fn moved_workspace_with_replaced_old_root_reports_foreign_root() {
    // Variant of the foreign-manifest flow: the old root still resolves
    // (to unrelated content) while the moved copy carries A's identity —
    // the manifest must still be rejected, never adopted.
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("ws");
    fs::create_dir_all(&root).unwrap();
    let (ok, _, err) = run(&root, &["init"]);
    assert!(ok, "{err}");
    let bytes = fs::read(manifest_path(&root)).unwrap();

    let moved = dir.path().join("ws-moved");
    fs::rename(&root, &moved).unwrap();
    fs::create_dir_all(&root).unwrap();
    let (ok, out, err) = run(&moved, &["init"]);
    assert!(!ok, "got: {out}");
    assert!(err.contains("different root"), "got: {err}");
    assert_eq!(fs::read(manifest_path(&moved)).unwrap(), bytes);
}

#[test]
fn two_workspaces_have_distinct_identities_and_roots() {
    // Workflow B: A and B coexist with no shared identity or state.
    let dir = tempdir().expect("tempdir");
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    let (ok, out_a, err) = run(&a, &["init"]);
    assert!(ok, "{err}");
    let (ok, out_b, err) = run(&b, &["init"]);
    assert!(ok, "{err}");

    let id_a = out_a
        .lines()
        .find(|l| l.contains("workspace id:"))
        .expect("id printed")
        .trim_start_matches("workspace id: ")
        .to_string();
    let id_b = out_b
        .lines()
        .find(|l| l.contains("workspace id:"))
        .expect("id printed")
        .trim_start_matches("workspace id: ")
        .to_string();
    assert_ne!(id_a, id_b, "two workspaces must never share an identity");
    assert!(id_a.starts_with("ws-") && id_b.starts_with("ws-"));

    // Each manifest records its own canonical root and its own identity.
    // Canonical-to-canonical comparison (parsed JSON value) so Windows
    // backslash JSON-escaping cannot falsely fail the root assertion.
    for (root, id) in [(&a, &id_a), (&b, &id_b)] {
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(manifest_path(root)).unwrap()).unwrap();
        assert_eq!(manifest["workspace_id"].as_str().expect("id"), id);
        assert_eq!(
            manifest["workspace_root"].as_str().expect("root string"),
            root.canonicalize().unwrap().to_str().expect("utf8 path")
        );
    }

    // Isolation: neither workspace's state mentions the other's identity.
    let a_state = tree_snapshot(&a);
    for (path, bytes) in &a_state {
        let text = String::from_utf8_lossy(bytes);
        assert!(
            !text.contains(&id_b),
            "workspace A references B's identity at {path:?}"
        );
    }
    let b_state = tree_snapshot(&b);
    for (path, bytes) in &b_state {
        let text = String::from_utf8_lossy(bytes);
        assert!(
            !text.contains(&id_a),
            "workspace B references A's identity at {path:?}"
        );
    }
}

#[test]
fn status_banner_is_state_independent_per_documented_contract() {
    // Current contract (docs/error.md §6 pins this output): `awh status`
    // is a bootstrap banner proving the binary runs — it is NOT a
    // workspace-info surface. It must not crash in any directory state,
    // and its output is identical whether or not a workspace exists.
    // Workspace inspection (identity/root reporting) is classified
    // Unproven in the TP02 report: `awh workspace info` is unimplemented.
    let dir = tempdir().expect("tempdir");
    let uninit = dir.path().join("uninit");
    fs::create_dir_all(&uninit).unwrap();

    let (ok, out, err) = run(&uninit, &["status"]);
    assert!(ok, "{err}");
    assert_eq!(
        out.trim_end(),
        "Agent Workspace Hub — Rust\nstatus: bootstrap complete"
    );

    let initialized = dir.path().join("init");
    fs::create_dir_all(&initialized).unwrap();
    let (ok, _, err) = run(&initialized, &["init"]);
    assert!(ok, "{err}");
    let (ok2, out2, err2) = run(&initialized, &["status"]);
    assert!(ok2, "{err2}");
    assert_eq!(out, out2, "banner must be state-independent");

    // Even a foreign/corrupt workspace must not break the banner command.
    let corrupt = dir.path().join("corrupt");
    fs::create_dir_all(corrupt.join(".agent")).unwrap();
    fs::write(manifest_path(&corrupt), "{ not json").unwrap();
    let (ok3, out3, err3) = run(&corrupt, &["status"]);
    assert!(ok3, "{err3}");
    assert_eq!(out, out3);
}

#[test]
fn init_fails_closed_when_agent_dir_is_a_regular_file() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::write(root.join(".agent"), "not a directory").unwrap();

    let (ok, out, err) = run(root, &["init"]);
    assert!(!ok, "got: {out}");
    assert!(
        err.contains("failed to create workspace state directory"),
        "got: {err}"
    );
    // Fail closed: no state was smuggled anywhere else.
    assert!(!root.join(".agent/workspace.json").exists());
    assert!(root.join(".agent").is_file(), "pre-existing file untouched");
}

#[test]
fn manifest_with_missing_required_field_fails_closed_at_cli() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    let (ok, _, err) = run(root, &["init"]);
    assert!(ok, "{err}");
    let path = manifest_path(root);
    let poisoned = fs::read_to_string(&path)
        .unwrap()
        .replace("\"workspace_root\"", "\"workspace_root_x\"");
    fs::write(&path, poisoned.clone()).unwrap();

    let (ok, out, err) = run(root, &["init"]);
    assert!(
        !ok,
        "init must fail on a missing required field, got: {out}"
    );
    assert!(err.contains("missing field"), "got: {err}");
    // Fail closed: the corrupt manifest was not repaired or replaced.
    assert_eq!(fs::read_to_string(&path).unwrap(), poisoned);
}

#[test]
fn init_is_idempotent_across_processes_with_full_state_compare() {
    // Workflows A + C at the real process boundary: after the first init,
    // every later `awh init` invocation (fresh process, new lock cycle)
    // must leave the ENTIRE durable state byte-identical.
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    let (ok, out, err) = run(root, &["init"]);
    assert!(ok, "{err}");
    let first_id = out
        .lines()
        .find(|l| l.contains("workspace id:"))
        .expect("id printed")
        .trim_start_matches("workspace id: ")
        .to_string();

    let before = tree_snapshot(&root.join(".agent"));
    for round in [2, 3] {
        let (ok, out, err) = run(root, &["init"]);
        assert!(ok, "round {round}: {err}");
        assert!(out.contains("already initialized"), "round {round}: {out}");
        assert!(
            out.contains(first_id.as_str()),
            "round {round} must report the same id: {out}"
        );
        let after = tree_snapshot(&root.join(".agent"));
        assert_eq!(
            before, after,
            "round {round} rewrote durable workspace state"
        );
    }
    assert!(
        !root.join(".agent/workspace.json.lock").exists(),
        "no lock residue"
    );
}

#[test]
fn wrong_prefix_workspace_id_is_accepted_by_documented_design() {
    // Typed-id validation is char-safety only by design: identity.rs pins
    // that a wrong-prefixed value passes `validate()` ("validation is
    // applied per type"), so init accepts a manifest whose workspace_id
    // lacks the `ws-` prefix. Anti-transfer security is carried by the
    // canonical-root binding, not by id shape. This test pins the current
    // boundary so it cannot change silently; the TP02 report flags
    // prefix enforcement at the manifest boundary as a hardening
    // opportunity for the maintainers.
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    let (ok, _, err) = run(root, &["init"]);
    assert!(ok, "{err}");
    let path = manifest_path(root);
    let edited = fs::read_to_string(&path)
        .unwrap()
        .replace("\"workspace_id\": \"ws-", "\"workspace_id\": \"xx-");
    fs::write(&path, edited.clone()).unwrap();

    let (ok, out, err) = run(root, &["init"]);
    assert!(
        ok,
        "char-safe wrong-prefixed id is accepted by design: {err}"
    );
    assert!(out.contains("already initialized"), "got: {out}");
    assert_eq!(fs::read_to_string(&path).unwrap(), edited);
}
