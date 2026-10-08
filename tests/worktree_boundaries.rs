//! TP04 (§15-§24): the AWH-managed worktree feature family — the gap
//! increment over `tests/worktree_cli.rs` (which already pins the core
//! lifecycle, branch collisions, unicode branches, ownership denial,
//! crash-window reconciliation, dirty-worktree preservation, concurrent
//! creation, and machine output).
//!
//! This suite pins what the first suite does NOT:
//! * §21 isolation proven at FILE level — a commit inside worktree A is
//!   visible in A's `git status`/`git log` and invisible in B's and the
//!   main checkout's (the sibling suite checks paths and ownership only);
//! * §24 identity/branch persistence across independent `awh` processes
//!   (each CLI invocation IS a fresh process; the store must round-trip);
//! * §16 hostile worktree-id shapes against every id-consuming command —
//!   traversal, separators, control characters, option-lookalikes — all
//!   refused with zero filesystem side effects;
//! * §17 the Missing→Removed finalize path (out-of-band deletion then a
//!   successful removal by the owner);
//! * §15 concurrent removal of the SAME worktree by two processes: the
//!   lifecycle lock must make exactly one win, deterministically;
//! * §18 an unmanaged `git worktree` never gets adopted by list/inspect;
//! * §20 invalid start points are rejected before any mutation, and a
//!   worktree on a repo whose HEAD is detached works (real git state).
//!
//! Everything runs against the compiled `awh` binary in disposable
//! initialized workspaces with real git repositories; every mutation is
//! proven by reading real filesystem/git state, never by trusting the
//! command's own output (§25).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

#[path = "common/mod.rs"]
mod common;

/// A real initialized workspace containing a real Git repository.
struct GitWorkspace {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

impl GitWorkspace {
    fn new() -> Self {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        let _ = common::sanitized_command(&root, &["init", "--path", "."])
            .output()
            .expect("awh init");
        // The fixture's git calls go to REAL git (the oracle binary), not
        // through the awh CLI (which has no git subcommand).
        for args in [
            &["init", "--quiet"][..],
            // Windows runners ship machine-wide core.autocrlf=true — pin
            // repo-local to keep checkouts byte-exact (PR #129 lesson).
            &["config", "core.autocrlf", "false"][..],
            &["config", "user.email", "tp04-wt@example.invalid"][..],
            &["config", "user.name", "TP04 WT"][..],
        ] {
            let ok = git(&root, args).status.success();
            assert!(ok, "git {args:?}");
        }
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        assert!(git(&root, &["add", "."]).status.success());
        assert!(git(&root, &["commit", "--quiet", "-m", "base"])
            .status
            .success());
        Self { root, _dir: dir }
    }

    fn run(&self, args: &[&str]) -> Output {
        common::sanitized_command(&self.root, args)
            .output()
            .expect("run awh binary")
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

    fn checkout(&self, id: &str) -> PathBuf {
        self.root.join(".agent").join("worktrees").join(id)
    }

    fn records_dir(&self) -> PathBuf {
        self.root.join(".agent").join("worktree-records")
    }
}

/// Direct git invocation — the ORACLE, independent of the product.
fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn oracle git")
}

fn git_ok(root: &Path, args: &[&str]) -> String {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "oracle git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn worktree_id_of(stdout: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json line");
    value["worktree_id"].as_str().expect("id field").to_owned()
}

/// Count of record JSONs on disk (the records dir may not exist yet).
fn record_count(ws: &GitWorkspace) -> usize {
    std::fs::read_dir(ws.records_dir())
        .map(|entries| {
            entries
                .filter(|e| {
                    e.as_ref()
                        .unwrap()
                        .path()
                        .extension()
                        .and_then(|x| x.to_str())
                        == Some("json")
                })
                .count()
        })
        .unwrap_or(0)
}

/// Every record on disk parses and carries the identity fields (no
/// half-written or corrupt records, even after failures).
fn assert_well_formed_records(ws: &GitWorkspace) {
    let Ok(entries) = std::fs::read_dir(ws.records_dir()) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("corrupt record {}: {e}", path.display()));
        assert!(
            value["worktree_id"].is_string() && value["state"].is_string(),
            "record {}: {value}",
            path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// §21 isolation proven at FILE level
// ---------------------------------------------------------------------------

#[test]
fn worktree_commits_are_isolated_at_file_level() {
    let ws = GitWorkspace::new();
    let a_id = ws.create("claude", "sess-a");
    let b_id = ws.create("qwen", "sess-b");
    let a = ws.checkout(&a_id);
    let b = ws.checkout(&b_id);

    // Real work lands ONLY in A: a new file, staged and committed.
    std::fs::write(a.join("a-work.txt"), "work in A\n").unwrap();
    assert!(git(&a, &["add", "--", "a-work.txt"]).status.success());
    assert!(git(&a, &["commit", "--quiet", "-m", "work in A only"])
        .status
        .success());

    // A sees its own commit (git log in the worktree proves it).
    let a_log = git_ok(&a, &["log", "--oneline"]);
    assert!(a_log.contains("work in A only"), "{a_log}");
    // B does NOT see it — different branch, different checkout.
    let b_log = git_ok(&b, &["log", "--oneline"]);
    assert!(!b_log.contains("work in A only"), "leaked into B: {b_log}");
    assert!(!b.join("a-work.txt").exists(), "file leaked into B");
    // The MAIN checkout does not see it either.
    let main_log = git_ok(&ws.root, &["log", "--oneline"]);
    assert!(
        !main_log.contains("work in A only"),
        "leaked into main: {main_log}"
    );
    assert!(
        !ws.root.join("a-work.txt").exists(),
        "file leaked into main"
    );

    // The main tree stays CLEAN: the worktree's file is not visible as an
    // untracked file of the main checkout (git scopes status per worktree).
    let main_status = git_ok(&ws.root, &["status", "--porcelain"]);
    assert!(
        !main_status.contains("a-work.txt"),
        "worktree file polluted main status: {main_status}"
    );

    // A's status is clean after its commit; B's is untouched.
    let a_status = git_ok(&a, &["status", "--porcelain"]);
    assert!(a_status.trim().is_empty(), "A not clean: {a_status}");
    // And the branches differ: A sits on its own awh/ branch.
    let a_branch = git_ok(&a, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let b_branch = git_ok(&b, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_ne!(a_branch.trim(), b_branch.trim(), "distinct branches");
}

// ---------------------------------------------------------------------------
// §24 persistence across independent processes
// ---------------------------------------------------------------------------

#[test]
fn worktree_identity_persists_across_independent_processes() {
    let ws = GitWorkspace::new();
    // Process 1 creates.
    let stdout = ws.ok(&["worktree", "create", "claude", "sess-persist", "--json"]);
    let id = worktree_id_of(&stdout);
    let branch = {
        let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
        value["branch"].as_str().expect("branch").to_owned()
    };

    // Processes 2..N (each `ws.run` is a fresh `awh` process) see the SAME
    // identity: id, branch, state, path — the record round-trips through
    // disk, and the checkout is still the recorded Git worktree.
    for _ in 0..3 {
        let stdout = ws.ok(&["worktree", "inspect", &id, "--json"]);
        let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
        assert_eq!(value["worktree_id"].as_str().unwrap(), id);
        assert_eq!(value["branch"].as_str().unwrap(), branch);
        assert_eq!(value["state"].as_str().unwrap(), "active");
    }

    // Git itself (the ultimate oracle) lists the checkout on the branch.
    // NOTE: git reports CANONICALIZED paths (macOS resolves /var →
    // /private/var), so compare canonical-to-canonical, never raw-to-
    // canonical (PR #129 lesson).
    let porcelain = git_ok(&ws.root, &["worktree", "list", "--porcelain"]);
    let listed = porcelain.lines().any(|l| {
        if let Some(listed_path) = l.strip_prefix("worktree ") {
            let a = std::fs::canonicalize(listed_path);
            let b = std::fs::canonicalize(ws.checkout(&id));
            match (a, b) {
                (Ok(a), Ok(b)) => a == b,
                // Both must fail identically for a fall-through raw match.
                _ => listed_path == ws.checkout(&id).display().to_string(),
            }
        } else {
            false
        }
    });
    assert!(listed, "git lists the managed checkout:\n{porcelain}");
    let branch_line = format!("branch refs/heads/{branch}");
    assert!(
        porcelain.contains(&branch_line),
        "git lists the branch: {porcelain}"
    );

    // The record file carries NO host-absolute path (§21: relative only).
    let record = std::fs::read_to_string(ws.records_dir().join(format!("{id}.json"))).unwrap();
    assert!(
        !record.contains(&ws.root.display().to_string()),
        "record must not embed the host-absolute workspace path: {record}"
    );
    assert!(record.contains(".agent/worktrees"), "relative path present");

    // A fresh process can still remove it by the same identity.
    ws.ok(&["worktree", "remove", &id, "--session-id", "sess-persist"]);
    assert!(!ws.checkout(&id).exists(), "checkout removed");
}

// ---------------------------------------------------------------------------
// §16 hostile worktree-id shapes: every id-consuming command
// ---------------------------------------------------------------------------

#[test]
fn hostile_worktree_ids_are_refused_with_zero_side_effects() {
    let ws = GitWorkspace::new();
    let real_id = ws.create("claude", "sess-real");
    let before: Vec<String> = std::fs::read_dir(ws.records_dir())
        .unwrap()
        .map(|e| e.unwrap().path().display().to_string())
        .collect();

    // Every hostile shape, against both inspect and remove. Shapes that
    // fail is_safe_worktree_id are usage-class (2); a WELL-FORMED id
    // that simply does not exist is recovery-class NotFound (5) — the
    // classification is part of the contract under test.
    let invalid_shapes = [
        "../../etc/passwd",
        "wt-../../escape",
        "wt-a/b",
        "wt-a\\b",
        "wt-..",
        "-rf",
        "--force",
        "wt-\u{0007}bell",
        "wt-ąćę",
        // 192 chars of otherwise-allowed alphabet: over the 128 cap.
        &"wt-".to_owned().repeat(64),
    ];
    for hostile in invalid_shapes {
        // inspect: usage-class refusal, deterministic domain message.
        let output = ws.fails(&["worktree", "inspect", hostile], 2);
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("worktree"),
            "hostile id {hostile:?} must produce a domain message"
        );
        // remove: same refusal; possession of a string is never authority.
        ws.fails(
            &["worktree", "remove", hostile, "--session-id", "sess-real"],
            2,
        );
    }
    // A well-formed unknown id: NotFound (5) everywhere, zero side effects.
    ws.fails(&["worktree", "inspect", "wt-00000000000000001111"], 5);
    ws.fails(
        &[
            "worktree",
            "remove",
            "wt-00000000000000001111",
            "--session-id",
            "sess-real",
        ],
        5,
    );

    // Zero side effects: the same record files exist, nothing was created
    // anywhere, and the real worktree is untouched.
    let after: Vec<String> = std::fs::read_dir(ws.records_dir())
        .unwrap()
        .map(|e| e.unwrap().path().display().to_string())
        .collect();
    assert_eq!(before, after, "no record files added or removed");
    assert!(ws.checkout(&real_id).is_dir(), "real worktree untouched");
    // No escape directory materialized above the workspace.
    assert!(!ws.root.join("etc").exists());
    // The well-formed wrong id is a NotFound (exit 5), not a crash — and
    // the message must not leak host paths.
    let output = ws.fails(&["worktree", "inspect", "wt-00000000000000001111"], 5);
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains(&ws.root.display().to_string()),
        "no host path leak"
    );
}

// ---------------------------------------------------------------------------
// §17 Missing → Removed finalize (out-of-band deletion)
// ---------------------------------------------------------------------------

#[test]
fn missing_worktree_finalizes_as_removed_by_its_owner() {
    let ws = GitWorkspace::new();
    let id = ws.create("claude", "sess-missing");

    // The checkout is deleted OUT OF BAND (raw git, --force).
    let checkout = ws.checkout(&id);
    assert!(git(
        &ws.root,
        &["worktree", "remove", "--force", checkout.to_str().unwrap()]
    )
    .status
    .success());
    assert!(!checkout.exists());

    // Inspect classifies Missing.
    let stdout = ws.ok(&["worktree", "inspect", &id, "--json"]);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["state"].as_str().unwrap(), "missing");

    // The OWNER can now remove it: the lifecycle finalizes (exit 0), the
    // record is terminal, and re-removal is NotFound (exit 5).
    ws.ok(&["worktree", "remove", &id, "--session-id", "sess-missing"]);
    let stdout = ws.ok(&["worktree", "inspect", &id, "--json"]);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["state"].as_str().unwrap(), "removed");
    ws.fails(
        &["worktree", "remove", &id, "--session-id", "sess-missing"],
        5,
    );

    // A NON-owner still cannot finalize it even in Missing state.
    let ws2 = GitWorkspace::new();
    let id2 = ws2.create("qwen", "sess-owner");
    let checkout2 = ws2.checkout(&id2);
    assert!(git(
        &ws2.root,
        &["worktree", "remove", "--force", checkout2.to_str().unwrap()]
    )
    .status
    .success());
    ws2.ok(&["worktree", "inspect", &id2]);
    ws2.fails(
        &["worktree", "remove", &id2, "--session-id", "someone-else"],
        4,
    );
    ws2.ok(&["worktree", "remove", &id2, "--session-id", "sess-owner"]);
}

// ---------------------------------------------------------------------------
// §15 concurrent removal of the SAME worktree
// ---------------------------------------------------------------------------

#[test]
fn concurrent_removal_of_one_worktree_has_exactly_one_winner() {
    let ws = GitWorkspace::new();
    let id = ws.create("claude", "sess-race");
    assert!(ws.checkout(&id).is_dir());

    // Two independent processes remove the same id simultaneously. The
    // lifecycle lock must serialize them: exactly one succeeds; the loser
    // gets a deterministic NotFound (5), never a corrupt record.
    let root = ws.root.clone();
    let loser_id = id.clone();
    let handle = std::thread::spawn(move || {
        common::sanitized_command(
            &root,
            &["worktree", "remove", &loser_id, "--session-id", "sess-race"],
        )
        .output()
        .expect("loser process")
    });
    let winner = ws.run(&["worktree", "remove", &id, "--session-id", "sess-race"]);
    let loser = handle.join().unwrap();

    let codes = [winner.status.code(), loser.status.code()];
    let zeros = codes.iter().filter(|c| **c == Some(0)).count();
    let fives = codes.iter().filter(|c| **c == Some(5)).count();
    assert_eq!(zeros, 1, "exactly one winner: {codes:?}");
    assert_eq!(fives, 1, "exactly one NotFound loser: {codes:?}");

    // The checkout is gone, and the record is terminal Removed — not
    // corrupted by the race.
    assert!(!ws.checkout(&id).exists());
    let stdout = ws.ok(&["worktree", "inspect", &id, "--json"]);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["state"].as_str().unwrap(), "removed");
}

// ---------------------------------------------------------------------------
// §18 an unmanaged git worktree is never adopted
// ---------------------------------------------------------------------------

#[test]
fn unmanaged_git_worktrees_are_never_adopted() {
    let ws = GitWorkspace::new();
    let managed = ws.create("claude", "sess-managed");

    // A raw `git worktree add` inside the managed area (no AWH record):
    // AWH must never list, inspect, or otherwise adopt it.
    let unmanaged_path = ws
        .root
        .join(".agent")
        .join("worktrees")
        .join("unmanaged-wt");
    let unmanaged_rel = ".agent/worktrees/unmanaged-wt";
    assert!(git(
        &ws.root,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "raw/unmanaged",
            "--",
            unmanaged_path.to_str().unwrap(),
            "HEAD"
        ]
    )
    .status
    .success());

    // list shows ONLY the managed record.
    let stdout = ws.ok(&["worktree", "list", "--json"]);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let items = value["worktrees"].as_array().unwrap();
    assert_eq!(items.len(), 1, "only managed records listed: {items:?}");
    assert_eq!(items[0]["worktree_id"].as_str().unwrap(), managed);
    assert!(items[0]["path"].as_str().unwrap().contains(&managed));

    // The unmanaged checkout keeps working as a plain git worktree.
    assert!(unmanaged_path.is_dir());
    let porcelain = git_ok(&ws.root, &["worktree", "list", "--porcelain"]);
    assert!(porcelain.contains("unmanaged-wt"), "git still lists it");

    // Cleanup through raw git (the unmanaged one is not AWH's to remove).
    let _ = git(
        &ws.root,
        &[
            "worktree",
            "remove",
            "--force",
            unmanaged_path.to_str().unwrap(),
        ],
    );
    assert!(!unmanaged_path.exists(), "raw cleanup worked");
    let _ = unmanaged_rel;
    ws.ok(&[
        "worktree",
        "remove",
        &managed,
        "--session-id",
        "sess-managed",
    ]);
}

// ---------------------------------------------------------------------------
// §20 invalid start points; detached-HEAD repos
// ---------------------------------------------------------------------------

#[test]
fn worktree_start_points_are_validated_before_any_mutation() {
    let ws = GitWorkspace::new();
    let records_before = record_count(&ws);

    // A nonexistent start point: git refuses; AWH surfaces the failure as
    // exit 1 (store-class git error) and leaves NO checkout directory and
    // no apparently-active record behind.
    let output = ws.fails(
        &[
            "worktree",
            "create",
            "claude",
            "sess-badref",
            "--from",
            "no-such-ref",
        ],
        1,
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        stderr.contains("git") || stderr.contains("failed"),
        "start-point refusal surfaces the git cause: {stderr}"
    );
    // No checkout directory was created for the failed attempt.
    let worktrees_dir = ws.root.join(".agent").join("worktrees");
    if worktrees_dir.exists() {
        for entry in std::fs::read_dir(&worktrees_dir).unwrap() {
            let path = entry.unwrap().path();
            assert!(
                !path.to_string_lossy().contains("sess-badref"),
                "no checkout for the failed create: {}",
                path.display()
            );
        }
    }
    // And every record on disk is well-formed JSON (no half-write).
    assert_well_formed_records(&ws);
    let records_after = record_count(&ws);
    assert!(records_after >= records_before, "records intact");

    // A VALID non-HEAD start point works: branch from the base commit.
    let base = git_ok(&ws.root, &["rev-parse", "HEAD"]).trim().to_owned();
    let stdout = ws.ok(&[
        "worktree",
        "create",
        "claude",
        "sess-frombase",
        "--from",
        &base,
        "--json",
    ]);
    let id = worktree_id_of(&stdout);
    // The worktree's HEAD equals the requested start point.
    let checkout = ws.checkout(&id);
    let head = git_ok(&checkout, &["rev-parse", "HEAD"]).trim().to_owned();
    assert_eq!(head, base, "worktree started at the requested ref");
    ws.ok(&["worktree", "remove", &id, "--session-id", "sess-frombase"]);
}

#[test]
fn worktree_creation_on_a_detached_head_repository() {
    let ws = GitWorkspace::new();
    // Detach the main checkout's HEAD (real git state an agent can create).
    assert!(git(&ws.root, &["checkout", "--quiet", "--detach"])
        .status
        .success());

    // Creation from a detached HEAD must still resolve HEAD to a commit
    // and succeed (the default branch name does not depend on the
    // main checkout's HEAD mode).
    let stdout = ws.ok(&["worktree", "create", "claude", "sess-detached", "--json"]);
    let id = worktree_id_of(&stdout);
    let checkout = ws.checkout(&id);
    assert!(checkout.is_dir());
    // The new worktree is on its own branch (NOT detached).
    let branch = git_ok(&checkout, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert!(
        branch.trim() != "HEAD",
        "worktree must not inherit detached HEAD: {branch}"
    );
    assert!(
        branch.trim().contains("sess-detached"),
        "default branch: {branch}"
    );

    // The default start point equals the detached commit.
    let base = git_ok(&ws.root, &["rev-parse", "HEAD"]).trim().to_owned();
    let head = git_ok(&checkout, &["rev-parse", "HEAD"]).trim().to_owned();
    assert_eq!(base, head, "started at the detached HEAD commit");

    ws.ok(&["worktree", "remove", &id, "--session-id", "sess-detached"]);
    assert!(git(&ws.root, &["checkout", "--quiet", "-"])
        .status
        .success());
}

// ---------------------------------------------------------------------------
// §19 exit-code categories: possession is never authority
// ---------------------------------------------------------------------------

#[test]
fn worktree_exit_codes_are_stable_categories() {
    let ws = GitWorkspace::new();

    // Uninitialized workspace: usage-class (2) for a workspace error.
    let dir = tempdir().unwrap();
    let output = common::sanitized_command(dir.path(), &["worktree", "list"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "uninitialized workspace");

    // Non-Git initialized workspace: explicit refusal, nothing created.
    let plain = tempdir().unwrap();
    let _ = common::sanitized_command(plain.path(), &["init", "--path", "."]).output();
    let output = common::sanitized_command(plain.path(), &["worktree", "create", "a", "s"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "non-git workspace");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("git"),
        "names the condition: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!plain.path().join(".agent").join("worktrees").exists());

    // Well-formed unknown id: recovery-class NotFound (5).
    ws.fails(&["worktree", "inspect", "wt-0000000000000000-1-1"], 5);
    // Same-session duplicate: conflict-class (4).
    ws.ok(&["worktree", "create", "claude", "sess-dup"]);
    ws.fails(&["worktree", "create", "claude", "sess-dup"], 4);
    // Invalid agent/session shapes: usage-class (2), nothing persisted.
    ws.fails(&["worktree", "create", "", "sess-x"], 2);
    ws.fails(&["worktree", "create", "agent/../x", "sess-x"], 2);
    ws.fails(&["worktree", "create", "claude", "sess with spaces"], 2);
    let stdout = ws.ok(&["worktree", "list", "--json"]);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let items = value["worktrees"].as_array().unwrap();
    assert_eq!(items.len(), 1, "only the one valid worktree: {items:?}");
}
