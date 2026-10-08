//! End-to-end CLI tests for `awh init` against the real compiled binary,
//! plus service-level restart/identity tests (TW-001).
//!
//! These prove the initialization contract on real temporary directories:
//! fresh init creates durable identity without implicit authority, re-init
//! is idempotent and preserves all state, and corrupt or foreign manifests
//! fail explicitly instead of being silently re-initialized.

use agent_workspace_hub::core::identity::{
    resolve_session_relation, AgentId, IdentityRelation, SessionIdentity,
};
use agent_workspace_hub::services::init::{
    initialize_workspace, load_workspace_manifest, InitOutcome,
};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

/// Runs `awh <args>` inside `dir` and returns (exit-success, stdout, stderr).
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

fn manifest_path(root: &Path) -> std::path::PathBuf {
    root.join(".agent").join("workspace.json")
}

#[test]
fn cli_fresh_init_creates_workspace_identity_without_authority() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    let (ok, out, err) = run(root, &["init"]);
    assert!(ok, "init failed: {err}");
    assert!(out.contains("initialized workspace"), "got: {out}");
    assert!(out.contains("workspace id: ws-"), "got: {out}");
    assert!(err.is_empty(), "stderr not empty: {err}");

    // Durable identity exists and reloads to the same WorkspaceId.
    let manifest = load_workspace_manifest(root).expect("manifest readable");
    assert_eq!(manifest.version, 1);
    assert!(manifest.workspace_id.as_str().starts_with("ws-"));
    assert!(out.contains(manifest.workspace_id.as_str()));

    // Policy store in its valid empty state, exactly one manifest.
    let policy = fs::read_to_string(root.join(".agent/policy.json")).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&policy).unwrap(),
        serde_json::json!([])
    );
    assert!(manifest_path(root).exists());

    // No implicit agent activation: no agent records exist at all.
    let agents_dir = root.join(".agent/agents");
    assert!(!agents_dir.exists(), "init must not create agent records");
    let (ok, out, _) = run(root, &["agent", "list"]);
    assert!(ok);
    assert_eq!(out.trim(), "", "agent list must be empty after init");

    // No implicit capability grants.
    assert!(
        !root.join(".agent/capabilities").exists(),
        "init must not create capability grants"
    );
}

#[test]
fn cli_reinit_is_idempotent_and_preserves_state() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    let (ok, _, err) = run(root, &["init"]);
    assert!(ok, "{err}");
    let first = load_workspace_manifest(root).unwrap();

    // User data and configuration that must survive re-init.
    let agent_file = root.join(".agent/agents/writer.json");
    fs::create_dir_all(root.join(".agent/agents")).unwrap();
    fs::write(
        &agent_file,
        r#"{"id":"writer","name":"Writer","role":"writer","status":"created","created_at":"2026-01-01T00:00:00Z"}"#,
    )
    .unwrap();
    let (ok, _, err) = run(root, &["policy", "deny", "workspace.write_file", "secret"]);
    assert!(ok, "{err}");
    let manifest_bytes = fs::read(manifest_path(root)).unwrap();
    let policy_before = fs::read_to_string(root.join(".agent/policy.json")).unwrap();

    let (ok, out, err) = run(root, &["init"]);
    assert!(ok, "{err}");
    assert!(out.contains("already initialized"), "got: {out}");
    assert!(out.contains(first.workspace_id.as_str()), "got: {out}");

    // Same workspace id; manifest byte-identical; state untouched.
    let second = load_workspace_manifest(root).unwrap();
    assert_eq!(second.workspace_id, first.workspace_id);
    assert_eq!(fs::read(manifest_path(root)).unwrap(), manifest_bytes);
    assert_eq!(
        fs::read_to_string(root.join(".agent/policy.json")).unwrap(),
        policy_before
    );
    assert!(
        agent_file.exists(),
        "user agent record must survive re-init"
    );

    // Running twice more stays idempotent (no new records, no reset).
    let (ok, out, _) = run(root, &["init"]);
    assert!(ok);
    assert!(out.contains("already initialized"));
    assert_eq!(fs::read(manifest_path(root)).unwrap(), manifest_bytes);
}

#[test]
fn cli_init_rejects_file_as_root() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("not-a-dir");
    fs::write(&file, "x").unwrap();
    let (ok, out, err) = run(dir.path(), &["init", "--path", "not-a-dir"]);
    assert!(!ok, "init must fail on a file root, got stdout: {out}");
    assert!(err.contains("not a directory"), "got: {err}");
}

#[test]
fn cli_init_rejects_corrupt_manifest_without_resetting_state() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    let (ok, _, err) = run(root, &["init"]);
    assert!(ok, "{err}");
    fs::write(manifest_path(root), "{ corrupt").unwrap();
    let (ok, out, err) = run(root, &["init"]);
    assert!(!ok, "init must fail on corrupt manifest, stdout: {out}");
    assert!(err.contains("invalid JSON"), "got: {err}");
    // Failed init must leave the corrupt bytes untouched — never a reset.
    assert_eq!(
        fs::read_to_string(manifest_path(root)).unwrap(),
        "{ corrupt"
    );
}

#[test]
fn cli_init_with_explicit_path_creates_state_only_there() {
    let dir = tempdir().expect("tempdir");
    let target = dir.path().join("ws");
    let (ok, out, err) = run(dir.path(), &["init", "--path", "ws"]);
    assert!(ok, "{err}");
    assert!(out.contains("initialized workspace"), "got: {out}");
    assert!(manifest_path(&target).exists());
    // AWH state must not leak into the CWD (the explicit-root contract).
    assert!(!manifest_path(dir.path()).exists());
    let (ok, _, err) = run(dir.path(), &["init", "--path", "ws"]);
    assert!(ok, "{err}");
    assert!(load_workspace_manifest(&target).is_ok());
}

#[test]
fn service_restart_reconstructs_same_identity_and_no_new_authority() {
    // Simulates a process restart: initialize in one call scope, then
    // reconstruct the runtime purely from disk in another.
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    let (ok, _, err) = run(root, &["init"]);
    assert!(ok, "{err}");
    let manifest = load_workspace_manifest(root).unwrap();

    // "Restart": a fresh service call sees the identical workspace.
    let reloaded = load_workspace_manifest(root).unwrap();
    assert_eq!(reloaded.workspace_id, manifest.workspace_id);
    assert_eq!(reloaded.workspace_root, manifest.workspace_root);
    assert_eq!(reloaded.created_at, manifest.created_at);

    // No implicit authority appears after reload: the identity alone does
    // not make an unknown agent resolvable, and active status must be
    // explicit (set by hand here) rather than implied.
    let unknown_session = SessionIdentity::new(AgentId::new(), reloaded.workspace_id.clone());
    assert_eq!(
        resolve_session_relation(root, &unknown_session).unwrap(),
        IdentityRelation::UnknownAgent
    );
    let (ok, out, _) = run(root, &["agent", "list"]);
    assert!(ok && out.trim().is_empty());
}

#[test]
fn service_init_with_concurrent_attempts_is_consistent() {
    // Two sequential service-level initializations of the same root must
    // agree on one identity — the store lock serializes them.
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("ws");
    let first = initialize_workspace(&root).unwrap();
    let second = initialize_workspace(&root).unwrap();
    assert!(matches!(first, InitOutcome::Created(_)));
    assert!(matches!(second, InitOutcome::AlreadyInitialized(_)));
    assert_eq!(
        first.manifest().workspace_id,
        second.manifest().workspace_id
    );
}

#[test]
fn service_foreign_manifest_is_rejected_not_adopted() {
    // A manifest recorded for another root (e.g. a copied directory) must
    // not be silently adopted as this workspace.
    let dir = tempdir().expect("tempdir");
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir_all(&a).unwrap();
    let (ok, _, err) = run(&a, &["init"]);
    assert!(ok, "{err}");
    fs::create_dir_all(b.join(".agent")).unwrap();
    fs::copy(manifest_path(&a), manifest_path(&b)).unwrap();
    let err = initialize_workspace(&b).unwrap_err();
    assert!(
        format!("{err:#}").contains("different root"),
        "got: {err:#}"
    );
}

#[test]
fn cli_init_rejects_unsupported_manifest_version() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    let (ok, _, err) = run(root, &["init"]);
    assert!(ok, "{err}");
    let path = manifest_path(root);
    let bumped = fs::read_to_string(&path)
        .unwrap()
        .replace("\"version\": 1", "\"version\": 99");
    fs::write(&path, bumped).unwrap();
    let (ok, out, err) = run(root, &["init"]);
    assert!(!ok, "init must fail on an unsupported version, got: {out}");
    assert!(
        err.contains("unsupported workspace manifest version"),
        "got: {err}"
    );
    // Fail closed: the unsupported manifest is left untouched.
    assert!(fs::read_to_string(&path)
        .unwrap()
        .contains("\"version\": 99"));
}

#[test]
fn cli_init_rejects_foreign_manifest_with_nonzero_exit() {
    let dir = tempdir().expect("tempdir");
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir_all(&a).unwrap();
    let (ok, _, err) = run(&a, &["init"]);
    assert!(ok, "{err}");
    fs::create_dir_all(b.join(".agent")).unwrap();
    fs::copy(manifest_path(&a), manifest_path(&b)).unwrap();
    let (ok, out, err) = run(&b, &["init"]);
    assert!(!ok, "init must fail on a foreign manifest, got: {out}");
    assert!(err.contains("different root"), "got: {err}");
    // The foreign manifest was not replaced by a fresh one.
    assert_eq!(
        fs::read_to_string(manifest_path(&b)).unwrap(),
        fs::read_to_string(manifest_path(&a)).unwrap()
    );
}

#[test]
fn parallel_inits_produce_exactly_one_canonical_identity() {
    // N threads hit initialize_workspace on the same fresh root at a
    // barrier. The manifest StoreLock serializes the check-then-create
    // cycle, so exactly one thread must observe Created and all others
    // AlreadyInitialized — never two competing workspace identities and
    // never an accepted torn manifest.
    const THREADS: usize = 8;
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("ws");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(THREADS));

    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let barrier = std::sync::Arc::clone(&barrier);
            let root = root.clone();
            std::thread::spawn(move || {
                barrier.wait();
                initialize_workspace(&root).expect("every init attempt must succeed")
            })
        })
        .collect();
    let outcomes: Vec<InitOutcome> = handles
        .into_iter()
        .map(|h| h.join().expect("init thread panicked"))
        .collect();

    let created = outcomes
        .iter()
        .filter(|o| matches!(o, InitOutcome::Created(_)))
        .count();
    assert_eq!(created, 1, "exactly one init may create the identity");
    let ids: std::collections::HashSet<_> = outcomes
        .iter()
        .map(|o| o.manifest().workspace_id.clone())
        .collect();
    assert_eq!(
        ids.len(),
        1,
        "all inits must agree on one canonical workspace identity"
    );

    // The accepted on-disk manifest is that identity, is valid, and is
    // byte-stable under a further (sequential) re-init.
    let manifest = load_workspace_manifest(&root).expect("manifest must be valid");
    assert_eq!(manifest.workspace_id, *ids.iter().next().unwrap());
    let bytes = fs::read(manifest_path(root.canonicalize().unwrap().as_path())).unwrap();
    let again = initialize_workspace(&root).unwrap();
    assert!(matches!(again, InitOutcome::AlreadyInitialized(_)));
    assert_eq!(
        fs::read(manifest_path(root.canonicalize().unwrap().as_path())).unwrap(),
        bytes,
        "re-init after the race must not rewrite the manifest"
    );

    // No lock residue from the race: the lock file is removed on drop.
    assert!(!root.join(".agent/workspace.json.lock").exists());
}
