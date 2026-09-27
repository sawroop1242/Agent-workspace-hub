//! GIT-001 tests: the AWH-managed worktree lifecycle proven against
//! real temporary Git repositories through the compiled `awh` binary —
//! isolation, ownership, containment, branch semantics, crash-window
//! reconciliation, and dirty-worktree preservation, asserting exit
//! codes and real filesystem/Git state (never just command output).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

/// A real initialized workspace containing a real Git repository.
struct GitWorkspace {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

impl GitWorkspace {
    fn new() -> Self {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        run_in(&root, &["init", "--path", "."]);
        // A real Git repository with one committed file (worktrees need
        // at least one commit for HEAD to exist).
        let _ = Command::new("git")
            .args(["init"])
            .current_dir(&root)
            .output();
        let _ = Command::new("git")
            .args(["config", "user.email", "test@example.invalid"])
            .current_dir(&root)
            .output();
        let _ = Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(&root)
            .output();
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        let _ = Command::new("git")
            .args(["add", "."])
            .current_dir(&root)
            .output();
        let _ = Command::new("git")
            .args(["commit", "-m", "base"])
            .current_dir(&root)
            .output();
        Self { root, _dir: dir }
    }

    fn run(&self, args: &[&str]) -> Output {
        run_in(&self.root, args)
    }

    fn ok(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "expected success, exit {:?}: stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("utf-8 stdout")
    }

    fn fails(&self, args: &[&str], code: i32) -> Output {
        let output = self.run(args);
        assert_eq!(
            output.status.code(),
            Some(code),
            "expected exit {code}, got {:?}: stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn create(&self, agent: &str, session: &str) -> String {
        let stdout = self.ok(&["worktree", "create", agent, session, "--json"]);
        worktree_id_of(&stdout)
    }
}

fn run_in(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("run awh binary")
}

fn worktree_id_of(stdout: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json line");
    value["worktree_id"].as_str().expect("id field").to_owned()
}

// ---------------------------------------------------------------------------
// Lifecycle basics
// ---------------------------------------------------------------------------

#[test]
fn worktree_create_list_inspect_remove_lifecycle() {
    let ws = GitWorkspace::new();
    let stdout = ws.ok(&["worktree", "create", "claude", "sess-1", "--json"]);
    let id = worktree_id_of(&stdout);
    assert!(id.starts_with("wt-"));

    // The checkout exists at the id-derived managed path.
    let checkout = ws.root.join(".agent").join("worktrees").join(&id);
    assert!(checkout.is_dir(), "managed checkout exists");
    assert!(
        checkout.join("README.md").exists(),
        "worktree shares repo content"
    );

    // list reports it as active.
    let stdout = ws.ok(&["worktree", "list"]);
    assert!(stdout.contains(&id), "{stdout}");
    assert!(stdout.contains("active"), "{stdout}");

    // inspect reconciles against Git and confirms active.
    let stdout = ws.ok(&["worktree", "inspect", &id]);
    assert!(stdout.contains("active"), "{stdout}");

    // remove (by the owning session) removes and verifies.
    let stdout = ws.ok(&[
        "worktree",
        "remove",
        &id,
        "--session-id",
        "sess-1",
        "--json",
    ]);
    assert!(stdout.contains("removed"), "{stdout}");
    assert!(!checkout.exists(), "checkout gone after remove");

    // Repeated remove of a Removed worktree → NotFound (recovery class).
    ws.fails(&["worktree", "remove", &id, "--session-id", "sess-1"], 5);
}

#[test]
fn worktree_requires_a_real_git_repository() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    // Initialized AWH workspace, but NOT a Git repository.
    run_in(root, &["init", "--path", "."]);
    let output = run_in(root, &["worktree", "create", "a", "s"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("git"),
        "non-Git workspace rejected explicitly: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // No shared-directory fallback: nothing was created.
    assert!(!root.join(".agent").join("worktrees").exists());
}

// ---------------------------------------------------------------------------
// Branch/ref semantics (§9)
// ---------------------------------------------------------------------------

#[test]
fn worktree_branch_defaults_and_explicit_branches() {
    let ws = GitWorkspace::new();
    // Default branch namespace awh/<agent>/<session>.
    let stdout = ws.ok(&["worktree", "create", "claude", "sess-a", "--json"]);
    assert!(stdout.contains("awh/claude/sess-a"), "{stdout}");

    // Explicit valid branch.
    ws.ok(&[
        "worktree",
        "create",
        "qwen",
        "sess-b",
        "--branch",
        "feature/x",
    ]);

    // The same branch cannot be checked out by a second worktree — Git
    // refuses, AWH surfaces a structured conflict, never a silent
    // switch of another worktree.
    ws.fails(
        &[
            "worktree",
            "create",
            "other",
            "sess-c",
            "--branch",
            "feature/x",
        ],
        4,
    );

    // Invalid ref names are rejected BEFORE any Git mutation.
    ws.fails(
        &["worktree", "create", "x", "sess-d", "--branch", "..bad"],
        2,
    );
    ws.fails(&["worktree", "create", "x", "sess-e", "--branch", "-rf"], 2);
}

#[test]
fn worktree_unicode_branch_names_are_safe() {
    let ws = GitWorkspace::new();
    let stdout = ws.ok(&[
        "worktree",
        "create",
        "claude",
        "sess-1",
        "--branch",
        "नमस्ते/🌍",
        "--json",
    ]);
    let id = worktree_id_of(&stdout);
    let checkout = ws.root.join(".agent").join("worktrees").join(&id);
    assert!(checkout.is_dir(), "unicode branch worktree created");
    ws.ok(&["worktree", "remove", &id, "--session-id", "sess-1"]);
}

// ---------------------------------------------------------------------------
// Ownership / cross-agent isolation (§10, §24)
// ---------------------------------------------------------------------------

#[test]
fn worktree_two_agent_isolation_and_cross_access_denied() {
    let ws = GitWorkspace::new();
    let a_id = ws.create("claude", "sess-a");
    let b_id = ws.create("qwen", "sess-b");

    let a_checkout = ws.root.join(".agent").join("worktrees").join(&a_id);
    let b_checkout = ws.root.join(".agent").join("worktrees").join(&b_id);
    assert!(a_checkout.is_dir() && b_checkout.is_dir());
    assert_ne!(a_checkout, b_checkout);

    // Ownership invariant: A cannot remove B's worktree (exit 4, B kept).
    ws.fails(&["worktree", "remove", &b_id, "--session-id", "sess-a"], 4);
    assert!(b_checkout.is_dir(), "cross-agent removal left B's worktree");

    // B CAN remove its own.
    ws.ok(&["worktree", "remove", &b_id, "--session-id", "sess-b"]);
    assert!(!b_checkout.exists());
    // A's worktree was never touched by B's lifecycle.
    assert!(a_checkout.is_dir());
}

#[test]
fn worktree_effective_root_binding_for_session() {
    let ws = GitWorkspace::new();
    let id = ws.create("claude", "sess-a");
    let a_checkout = ws.root.join(".agent").join("worktrees").join(&id);

    // The service contract: a session with an active worktree resolves
    // to the worktree root; another session resolves to the workspace.
    // Verified via the store API through the library (the thin-adapter
    // boundary, §11).
    let store = agent_workspace_hub::services::worktree::WorktreeStore::new(&ws.root);
    let bound = store.resolve_effective_root("sess-a").unwrap();
    let unbound = store.resolve_effective_root("sess-other").unwrap();
    assert_eq!(bound, a_checkout, "bound session sees its worktree root");
    assert_eq!(unbound, ws.root, "unbound session sees the workspace root");
}

// ---------------------------------------------------------------------------
// Recovery / reconciliation (§14)
// ---------------------------------------------------------------------------

#[test]
fn worktree_crash_windows_reconcile_deterministically() {
    // Window A: reservation persisted, Git never ran (checkout absent)
    // → classified Missing, never fabricated Active.
    let ws = GitWorkspace::new();
    let record = agent_workspace_hub::services::worktree::WorktreeRecord {
        schema_version: agent_workspace_hub::services::worktree::WORKTREE_SCHEMA_VERSION,
        worktree_id: "wt-9000000000000000-1-1".into(),
        workspace_id: "ws-test".into(),
        agent_id: "claude".into(),
        session_id: "sess-crash".into(),
        repo_id: "deadbeef".into(),
        path: ".agent/worktrees/wt-9000000000000000-1-1".into(),
        branch: "awh/claude/sess-crash".into(),
        state: agent_workspace_hub::services::worktree::WorktreeState::Creating,
        created_at: chrono::Utc::now().to_rfc3339(),
        removed_at: None,
    };
    let store = agent_workspace_hub::services::worktree::WorktreeStore::new(&ws.root);
    std::fs::create_dir_all(ws.root.join(".agent").join("worktree-records")).unwrap();
    std::fs::write(
        ws.root
            .join(".agent")
            .join("worktree-records")
            .join("wt-9000000000000000-1-1.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();

    // Window A: no checkout → inspect classifies Missing.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let state = runtime.block_on(async {
        store
            .inspect("wt-9000000000000000-1-1")
            .await
            .unwrap()
            .state
    });
    assert_eq!(
        state,
        agent_workspace_hub::services::worktree::WorktreeState::Missing
    );

    // Window B: reservation + real Git checkout at the exact id path →
    // unambiguous completion to Active (§14 step 7).
    let ws2 = GitWorkspace::new();
    let checkout_dir = ws2
        .root
        .join(".agent")
        .join("worktrees")
        .join("wt-9000000000000001-1-1");
    let git = Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "awh/claude/sess-crash2",
            "--",
            checkout_dir.to_str().unwrap(),
            "HEAD",
        ])
        .current_dir(&ws2.root)
        .output()
        .unwrap();
    assert!(git.status.success());
    let record_b = agent_workspace_hub::services::worktree::WorktreeRecord {
        schema_version: agent_workspace_hub::services::worktree::WORKTREE_SCHEMA_VERSION,
        worktree_id: "wt-9000000000000001-1-1".into(),
        workspace_id: "ws-test".into(),
        agent_id: "claude".into(),
        session_id: "sess-crash2".into(),
        repo_id: "deadbeef".into(),
        path: ".agent/worktrees/wt-9000000000000001-1-1".into(),
        branch: "awh/claude/sess-crash2".into(),
        state: agent_workspace_hub::services::worktree::WorktreeState::Creating,
        created_at: chrono::Utc::now().to_rfc3339(),
        removed_at: None,
    };
    std::fs::create_dir_all(ws2.root.join(".agent").join("worktree-records")).unwrap();
    std::fs::write(
        ws2.root
            .join(".agent")
            .join("worktree-records")
            .join("wt-9000000000000001-1-1.json"),
        serde_json::to_vec(&record_b).unwrap(),
    )
    .unwrap();
    let store_b = agent_workspace_hub::services::worktree::WorktreeStore::new(&ws2.root);
    let state_b = runtime.block_on(async {
        store_b
            .inspect("wt-9000000000000001-1-1")
            .await
            .unwrap()
            .state
    });
    assert_eq!(
        state_b,
        agent_workspace_hub::services::worktree::WorktreeState::Active
    );
}

#[test]
fn worktree_missing_out_of_band_is_classified_missing() {
    let ws = GitWorkspace::new();
    let id = ws.create("claude", "sess-a");

    // The checkout is deleted out of band (git worktree remove directly).
    let checkout = ws.root.join(".agent").join("worktrees").join(&id);
    let _ = Command::new("git")
        .args(["worktree", "remove", "--force", checkout.to_str().unwrap()])
        .current_dir(&ws.root)
        .output();

    let stdout = ws.ok(&["worktree", "inspect", &id]);
    assert!(stdout.contains("missing"), "{stdout}");
    assert!(!stdout.contains("active"), "never fabricated healthy");
}

#[test]
fn worktree_corrupt_metadata_fails_closed() {
    let ws = GitWorkspace::new();
    let id = ws.create("claude", "sess-a");
    // Corrupt the record.
    let path = ws
        .root
        .join(".agent")
        .join("worktree-records")
        .join(format!("{id}.json"));
    std::fs::write(&path, b"{corrupt").unwrap();

    ws.fails(&["worktree", "inspect", &id], 5);
    ws.fails(&["worktree", "list"], 5);
    ws.fails(&["worktree", "remove", &id, "--session-id", "sess-a"], 5);
    // The checkout itself is untouched by the metadata corruption.
    assert!(ws.root.join(".agent").join("worktrees").join(&id).is_dir());
}

// ---------------------------------------------------------------------------
// Removal safety (§13)
// ---------------------------------------------------------------------------

#[test]
fn worktree_dirty_removal_preserves_uncommitted_changes() {
    let ws = GitWorkspace::new();
    let id = ws.create("claude", "sess-a");
    let checkout = ws.root.join(".agent").join("worktrees").join(&id);

    // Simulate agent work: an uncommitted modification inside the worktree.
    std::fs::write(checkout.join("wip.txt"), "uncommitted agent work\n").unwrap();

    let output = ws.fails(&["worktree", "remove", &id, "--session-id", "sess-a"], 5);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("uncommitted"),
        "dirty refusal names the condition: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // The worktree and the agent's uncommitted work are PRESERVED.
    assert!(checkout.is_dir(), "dirty worktree preserved");
    assert!(
        checkout.join("wip.txt").exists(),
        "uncommitted work not discarded"
    );
}

// ---------------------------------------------------------------------------
// Concurrency (§15)
// ---------------------------------------------------------------------------

#[test]
fn worktree_concurrent_creation_is_deterministic() {
    let ws = GitWorkspace::new();
    // Four sessions creating concurrently: unique ids, unique paths, no
    // duplicate ownership, all active afterwards.
    let root = ws.root.clone();
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let root = root.clone();
            std::thread::spawn(move || {
                let output = Command::new(env!("CARGO_BIN_EXE_awh"))
                    .args([
                        "worktree",
                        "create",
                        "claude",
                        &format!("sess-c{i}"),
                        "--json",
                    ])
                    .current_dir(&root)
                    .output()
                    .unwrap();
                (
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout).to_string(),
                )
            })
        })
        .collect();
    let mut ids = Vec::new();
    for handle in handles {
        let (code, stdout) = handle.join().unwrap();
        assert_eq!(code, Some(0), "concurrent create failed: {stdout}");
        ids.push(worktree_id_of(&stdout));
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 4, "four distinct worktree ids: {ids:?}");

    // Same session requesting twice: exactly one worktree.
    let ws2 = GitWorkspace::new();
    ws2.ok(&["worktree", "create", "claude", "sess-dup"]);
    ws2.fails(&["worktree", "create", "claude", "sess-dup"], 4);
}

// ---------------------------------------------------------------------------
// CLI stream hygiene + machine output (§19)
// ---------------------------------------------------------------------------

#[test]
fn worktree_json_output_is_parseable_and_streams_clean() {
    let ws = GitWorkspace::new();
    let stdout = ws.ok(&["worktree", "create", "claude", "sess-j", "--json"]);
    let value: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("machine output is valid JSON");
    assert_eq!(value["status"], "created");
    assert!(value["worktree_id"].is_string());
    assert!(!stdout.contains("INFO"), "no tracing on stdout");

    // Listing is bounded + deterministic and machine-readable in --json.
    let stdout = ws.ok(&["worktree", "list", "--json"]);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let items = value["worktrees"].as_array().unwrap();
    assert_eq!(items.len(), 1, "only this workspace's worktree listed");
}
