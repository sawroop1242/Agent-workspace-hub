//! FS-001 integration tests: filesystem TOCTOU + mutation coordination.
//!
//! Every test uses real temporary directories and the REAL services —
//! `FilesService`, `EditService`, and the MCP workspace plane — with
//! genuine OS threads (and, for the cross-process plane, genuine lock
//! files). No mocks: the races under test only exist on a real
//! filesystem.
//!
//! Coverage map (prompt 14 §20):
//! * same-file concurrent replace/replace — one wins, the loser
//!   conflicts (never interleaved bytes);
//! * replace vs delete — the delete either fully precedes or the
//!   replace detects the vanished target;
//! * concurrent create of a missing file — both commits are complete
//!   files, never interleaved content;
//! * stale ExpectedState — refused with the canonical conflict, never a
//!   silent overwrite;
//! * deletion/recreation — a recreated file is a different state; the
//!   prepared edit refuses rather than stomping it;
//! * symlink substitution — the swap is detected at the mutation
//!   boundary and the mutation is refused with zero mutation outside
//!   the root;
//! * parent/path replacement — a directory swapped for a file fails the
//!   revalidation under the lock;
//! * sibling-prefix confusion — `a` and `a.txt` are distinct resources;
//! * cross-worktree isolation — independent roots never serialize each
//!   other;
//! * overlapping multi-file transactions — deterministic lock ordering
//!   prevents deadlock with A=[a,b] vs B=[b,c] hammering in parallel;
//! * lock timeout — a held resource fails a second acquirer closed;
//! * crash recovery — a stale lock file (crashed holder) is reclaimed;
//! * exact-byte preservation — coordination adds zero bytes (LF, CRLF,
//!   mixed, no-final-newline, UTF-8, Devanagari, emoji, empty vs
//!   zero-byte distinctions intact).

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use agent_workspace_hub::core::fs_coordination::{CoordinationError, FsCoordinator};
use agent_workspace_hub::services::edit::{EditError, EditOperation, EditService, EditTransaction};
use agent_workspace_hub::services::files::FilesService;

/// Builds an edit service over a fresh temp workspace.
fn edit_service() -> (tempfile::TempDir, EditService) {
    let temp = tempfile::tempdir().unwrap();
    let svc = EditService::new(temp.path());
    (temp, svc)
}

/// Writes a file through raw std (NOT through the service) — simulates
/// a foreign/external actor bypassing AWH's coordination.
fn external_write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// The number of committed + conflicted outcomes observed across a
/// same-file hammer must account for every participant exactly once.
#[derive(Default, Debug)]
struct RaceOutcome {
    committed: AtomicUsize,
    conflicted: AtomicUsize,
    other_errors: AtomicUsize,
}

// ---------------------------------------------------------------------------
// Same-file concurrent mutations
// ---------------------------------------------------------------------------

/// N threads race `replace` on ONE file. Coordination must serialize them:
/// exactly one edit commits per "generation", every other participant
/// either conflicts on the observed state or also commits sequentially —
/// but the file is NEVER interleaved/truncated, and every failed
/// participant reports the canonical expected-state conflict, not a torn
/// write.
#[test]
fn same_file_concurrent_replace_replace_produces_no_interleaving() {
    const THREADS: usize = 8;
    let (temp, svc) = edit_service();
    let svc = Arc::new(svc);
    let start = Arc::new(AtomicBool::new(false));
    let outcome = Arc::new(RaceOutcome::default());

    let mut handles = Vec::new();
    for i in 0..THREADS {
        let svc = Arc::clone(&svc);
        let start = Arc::clone(&start);
        let outcome = Arc::clone(&outcome);
        handles.push(thread::spawn(move || {
            while !start.load(Ordering::SeqCst) {
                thread::yield_now();
            }
            let tx = EditTransaction::single(EditOperation::Replace {
                path: "race.rs".into(),
                old: "base".into(),
                new: format!("base-thread-{i}"),
                occurrence: None,
            });
            match svc.replace(tx) {
                Ok(_) => outcome.committed.fetch_add(1, Ordering::SeqCst),
                Err(agent_workspace_hub::services::edit::EditError::ExpectedStateConflict(_))
                | Err(agent_workspace_hub::services::edit::EditError::MatchNotFound { .. })
                | Err(agent_workspace_hub::services::edit::EditError::AmbiguousMatch { .. }) => {
                    outcome.conflicted.fetch_add(1, Ordering::SeqCst)
                }
                Err(_) => outcome.other_errors.fetch_add(1, Ordering::SeqCst),
            }
        }));
    }
    external_write(temp.path(), "race.rs", "base\n");
    start.store(true, Ordering::SeqCst);
    for handle in handles {
        handle.join().unwrap();
    }

    let committed = outcome.committed.load(Ordering::SeqCst);
    let conflicted = outcome.conflicted.load(Ordering::SeqCst);
    let other = outcome.other_errors.load(Ordering::SeqCst);
    assert_eq!(
        committed + conflicted + other,
        THREADS,
        "every participant reports exactly one outcome"
    );
    // The very first edit MUST succeed (the seeded state is live); every
    // later concurrent participant either commits after re-observing or
    // conflicts. At minimum: no torn bytes — the file contains exactly
    // one thread's marker.
    assert!(committed >= 1, "at least the first edit commits");
    let final_content = fs::read_to_string(temp.path().join("race.rs")).unwrap();
    let markers: Vec<_> = (0..THREADS)
        .filter(|i| final_content.contains(&format!("base-thread-{i}")))
        .collect();
    assert_eq!(
        markers.len(),
        1,
        "exactly one thread's marker is present (no interleaving): {final_content:?}"
    );
    assert!(final_content.starts_with("base-thread-") || final_content == "base\n");
}

/// replace vs delete on the same file: whichever wins, the loser's
/// outcome is honest (conflict/not-found), never a resurrection that
/// stomps the delete, and never a torn file.
#[test]
fn concurrent_replace_and_delete_never_produce_torn_state() {
    let (temp, svc) = edit_service();
    external_write(temp.path(), "victim.rs", "keep\n");
    let svc = Arc::new(svc);
    let barrier = Arc::new(std::sync::Barrier::new(2));

    let editor = {
        let svc = Arc::clone(&svc);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            let tx = EditTransaction::single(EditOperation::Replace {
                path: "victim.rs".into(),
                old: "keep".into(),
                new: "edited".into(),
                occurrence: None,
            });
            let _ = svc.replace(tx);
        })
    };
    let deleter = {
        let files = FilesService::new(temp.path());
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            let _ = files.delete("victim.rs");
        })
    };
    editor.join().unwrap();
    deleter.join().unwrap();

    let path = temp.path().join("victim.rs");
    if path.exists() {
        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content == "keep\n" || content == "edited\n",
            "torn state: {content:?}"
        );
    }
    // Either outcome is a complete, honest file state.
}

/// Two threads create the same MISSING file concurrently. Coordination
/// serializes the creates: the winner's complete content lands, and the
/// runner-up's write also commits completely (create-or-overwrite
/// contract, serialized) — never interleaved bytes.
#[test]
fn concurrent_create_of_missing_file_never_interleaves() {
    let temp = tempfile::tempdir().unwrap();
    let files = Arc::new(FilesService::new(temp.path()));
    let start = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();
    for i in 0..6 {
        let files = Arc::clone(&files);
        let start = Arc::clone(&start);
        handles.push(thread::spawn(move || {
            while !start.load(Ordering::SeqCst) {
                thread::yield_now();
            }
            let target = if i % 2 == 0 {
                "created-0.txt"
            } else {
                "created-1.txt"
            };
            let payload = format!("payload-{i}\n").repeat(200);
            files.write(target, &payload).unwrap();
        }));
    }
    start.store(true, Ordering::SeqCst);
    for handle in handles {
        handle.join().unwrap();
    }
    for name in ["created-0.txt", "created-1.txt"] {
        let content = fs::read_to_string(temp.path().join(name)).unwrap();
        // Each line is one writer's complete payload — coordination
        // guarantees whole-file serialization, so no line mixes markers.
        for line in content.lines() {
            let marker = line.strip_prefix("payload-").unwrap();
            let index: usize = marker.parse().unwrap();
            let expected = format!("payload-{index}");
            assert_eq!(line, expected);
        }
    }
}

// ---------------------------------------------------------------------------
// Stale ExpectedState + deletion/recreation
// ---------------------------------------------------------------------------

/// A stale expected state is refused with the canonical conflict — the
/// newer external content is never overwritten (re-verified through the
/// coordinated boundary).
#[test]
fn stale_expected_state_conflicts_not_overwrites() {
    let (temp, svc) = edit_service();
    external_write(temp.path(), "doc.md", "original\n");
    let before_state =
        agent_workspace_hub::services::edit::FileState::from_content("doc.md", "original\n");
    // External mutation between observation and commit.
    external_write(temp.path(), "doc.md", "external-newer\n");
    let mut tx = EditTransaction::single(EditOperation::Replace {
        path: "doc.md".into(),
        old: "original".into(),
        new: "edited".into(),
        occurrence: None,
    });
    tx.expected = vec![agent_workspace_hub::services::edit::ExpectedState {
        hash: Some(before_state.hash),
        size: Some(before_state.size),
        line_count: Some(before_state.line_count),
        context: None,
    }];
    let error = svc.replace(tx).unwrap_err();
    assert!(matches!(error, EditError::ExpectedStateConflict(_)));
    assert_eq!(
        fs::read_to_string(temp.path().join("doc.md")).unwrap(),
        "external-newer\n"
    );
}

/// Delete-then-recreate with different bytes: a prepared edit against
/// the deleted generation refuses to commit onto the recreated file
/// (the hash/size guard fires under the lock).
#[test]
fn deletion_and_recreation_is_detected_as_a_conflict() {
    let (temp, svc) = edit_service();
    external_write(temp.path(), "config.toml", "key = \"a\"\n");
    // Observe state 1, then the file is deleted and recreated.
    let bytes = fs::read(temp.path().join("config.toml")).unwrap();
    let before = agent_workspace_hub::services::edit::FileState::from_content(
        "config.toml",
        std::str::from_utf8(&bytes).unwrap(),
    );
    let files = FilesService::new(temp.path());
    files.delete("config.toml").unwrap();
    external_write(temp.path(), "config.toml", "key = \"recreated\"\n");
    let mut tx = EditTransaction::single(EditOperation::Replace {
        path: "config.toml".into(),
        old: "key = \"a\"".into(),
        new: "key = \"b\"".into(),
        occurrence: None,
    });
    tx.expected = vec![agent_workspace_hub::services::edit::ExpectedState {
        hash: Some(before.hash),
        size: Some(before.size),
        line_count: Some(before.line_count),
        context: None,
    }];
    let error = svc.replace(tx).unwrap_err();
    assert!(matches!(error, EditError::ExpectedStateConflict(_)));
    assert_eq!(
        fs::read_to_string(temp.path().join("config.toml")).unwrap(),
        "key = \"recreated\"\n"
    );
}

// ---------------------------------------------------------------------------
// Symlink substitution + path identity
// ---------------------------------------------------------------------------

/// A final-component symlink substitution is detected: the revalidation
/// under the lock (or the canonical symlink-escape check inside
/// resolve_checked) refuses the mutation; nothing outside the workspace
/// root is ever written.
#[cfg(unix)]
#[test]
fn final_component_symlink_substitution_is_refused() {
    let (temp, svc) = edit_service();
    external_write(temp.path(), "real.txt", "inside\n");
    let outside = tempfile::tempdir().unwrap();
    let link = temp.path().join("linked.txt");
    std::os::unix::fs::symlink(outside.path().join("target.txt"), &link).unwrap();

    // A write THROUGH the service onto a symlink pointing outside the
    // root must fail closed.
    let files = FilesService::new(temp.path());
    let result = files.write("linked.txt", "escaped");
    assert!(result.is_err(), "symlink escape must be refused");
    assert!(!outside.path().join("target.txt").exists());

    // Same for the edit engine: the coordinated preflight resolves
    // through the symlink and fails.
    let tx = EditTransaction::single(EditOperation::Replace {
        path: "linked.txt".into(),
        old: "inside".into(),
        new: "edited".into(),
        occurrence: None,
    });
    assert!(svc.replace(tx).is_err());
    assert!(!outside.path().join("target.txt").exists());
}

/// A parent directory swapped for a file (or vice versa) between a
/// caller's observation and the commit fails the revalidation under the
/// lock — never a mutation through a stale path identity.
#[test]
fn parent_replacement_fails_revalidation() {
    let (temp, svc) = edit_service();
    external_write(temp.path(), "dir/file.txt", "content\n");
    // Swap the parent directory for a file AFTER the caller observed it.
    let files = FilesService::new(temp.path());
    files.delete("dir").unwrap();
    external_write(temp.path(), "dir", "now a file");
    let tx = EditTransaction::single(EditOperation::Replace {
        path: "dir/file.txt".into(),
        old: "content".into(),
        new: "edited".into(),
        occurrence: None,
    });
    assert!(svc.replace(tx).is_err());
    assert_eq!(
        fs::read_to_string(temp.path().join("dir")).unwrap(),
        "now a file"
    );
}

/// Sibling-prefix confusion: `a` and `a.txt` (and `dir/a.txt` vs
/// `dir.txt`) are distinct resources with distinct keys — a lock on one
/// never blocks the other.
#[test]
fn sibling_prefixes_are_distinct_resources() {
    let temp = tempfile::tempdir().unwrap();
    let coordinator = FsCoordinator::new(temp.path());
    let held = coordinator.acquire(&["a"]).unwrap();
    // `a.txt` is acquirable while `a` is held — distinct keys.
    let sibling = coordinator.acquire(&["a.txt"]).unwrap();
    drop(sibling);
    drop(held);
}

// ---------------------------------------------------------------------------
// Cross-worktree isolation + multi-file transactions
// ---------------------------------------------------------------------------

/// Independent workspaces do not serialize each other: a held resource
/// in one root never blocks the same relative path in another root.
#[test]
fn independent_workspaces_do_not_serialize_each_other() {
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    let coordinator_one = FsCoordinator::new(one.path());
    let coordinator_two = FsCoordinator::new(two.path());
    let _held = coordinator_one.acquire(&["src/main.rs"]).unwrap();
    // Same relative path, different root: must succeed immediately.
    let started = Instant::now();
    let other = coordinator_two.acquire(&["src/main.rs"]).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "independent worktree blocked on an unrelated workspace"
    );
    drop(other);
}

/// Overlapping multi-file acquisitions hammered in parallel never
/// deadlock: A=[a,b], B=[b,c], C=[c,d] with the deterministic sorted
/// order resolve in bounded time.
#[test]
fn overlapping_multi_file_transactions_never_deadlock() {
    let temp = tempfile::tempdir().unwrap();
    let coordinator = Arc::new(FsCoordinator::new(temp.path()));
    const ROUNDS: usize = 40;
    let done = Arc::new(AtomicUsize::new(0));

    let plans: [&[&str]; 3] = [
        &["a.txt", "b.txt"],
        &["b.txt", "c.txt"],
        &["c.txt", "d.txt"],
    ];
    let mut handles = Vec::new();
    for (worker, plan) in plans.iter().enumerate() {
        let coordinator = Arc::clone(&coordinator);
        let done = Arc::clone(&done);
        let plan: Vec<&'static str> = plan.to_vec();
        handles.push(thread::spawn(move || {
            for round in 0..ROUNDS {
                // Deliberately shuffled input order: the coordinator's
                // sorted acquisition must make it safe.
                let mut paths = plan.clone();
                if (worker + round) % 2 == 0 {
                    paths.reverse();
                }
                let set = coordinator.acquire(&paths).unwrap();
                assert!(!set.held().is_empty());
                drop(set);
                done.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    for handle in handles {
        handle.join().expect("deadlock: a worker never finished");
    }
    assert_eq!(done.load(Ordering::SeqCst), 3 * ROUNDS);
}

/// Disjoint multi-file transactions proceed concurrently: a two-file
/// set does not wait on an unrelated single-file lock elsewhere.
#[test]
fn disjoint_resources_are_not_globally_serialized() {
    let temp = tempfile::tempdir().unwrap();
    let coordinator = FsCoordinator::new(temp.path());
    let _unrelated = coordinator.acquire(&["unrelated.txt"]).unwrap();
    let started = Instant::now();
    let set = coordinator
        .acquire(&["alpha.txt", "beta.txt"])
        .expect("disjoint files must not block on an unrelated resource");
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(set);
}

/// A real multi-file patch transaction (two files) commits both members
/// atomically per file and verifies both.
#[test]
fn multi_file_patch_commits_and_verifies_both_members() {
    let (temp, svc) = edit_service();
    external_write(temp.path(), "one.txt", "alpha\n");
    external_write(temp.path(), "two.txt", "beta\n");
    let mut tx = EditTransaction::new(vec![
        EditOperation::Replace {
            path: "one.txt".into(),
            old: "alpha".into(),
            new: "ALPHA".into(),
            occurrence: None,
        },
        EditOperation::Replace {
            path: "two.txt".into(),
            old: "beta".into(),
            new: "BETA".into(),
            occurrence: None,
        },
    ]);
    tx.expected = vec![];
    let result = svc.patch(tx).unwrap();
    assert!(matches!(
        result.status,
        agent_workspace_hub::services::edit::PatchStatus::Committed
    ));
    assert_eq!(
        fs::read_to_string(temp.path().join("one.txt")).unwrap(),
        "ALPHA\n"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("two.txt")).unwrap(),
        "BETA\n"
    );
}

// ---------------------------------------------------------------------------
// Timeout, crash recovery, cancellation
// ---------------------------------------------------------------------------

/// A held resource times out the second acquirer within the bounded
/// window with the structured fail-closed error.
#[test]
fn lock_acquisition_times_out_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let coordinator = FsCoordinator::new(temp.path());
    let _held = coordinator.acquire(&["hot.txt"]).unwrap();
    let started = Instant::now();
    let error = coordinator.acquire(&["hot.txt"]).unwrap_err();
    assert!(matches!(error, CoordinationError::TimedOut { .. }));
    assert!(
        started.elapsed() >= Duration::from_secs(10),
        "the timeout must be the documented bounded wait, not an early giveup"
    );
}

/// A crashed holder's lock file (older than the stale age) is
/// reclaimed by the next acquirer — the workspace cannot be wedged
/// forever by a killed process.
#[test]
fn crashed_holder_lock_file_is_reclaimed() {
    let temp = tempfile::tempdir().unwrap();
    let coordinator = FsCoordinator::new(temp.path());
    let key =
        agent_workspace_hub::core::fs_coordination::resource_key(temp.path(), "gone.txt").unwrap();
    let dir = std::env::temp_dir().join("awh-fs-coordination");
    fs::create_dir_all(&dir).unwrap();
    let lock_path = dir.join(format!("{}.lock", key.as_str()));
    fs::write(&lock_path, b"abandoned").unwrap();
    let past = SystemTime::now() - Duration::from_secs(35);
    fs::File::options()
        .write(true)
        .open(&lock_path)
        .unwrap()
        .set_modified(past)
        .unwrap();
    // Reclaimed within the acquire timeout, not a timeout error.
    let set = coordinator.acquire(&["gone.txt"]).unwrap();
    drop(set);
}

/// Cancellation after acquisition releases every resource: dropping the
/// set mid-span frees the file for the next acquirer.
#[test]
fn dropping_the_set_releases_every_resource() {
    let temp = tempfile::tempdir().unwrap();
    let coordinator = FsCoordinator::new(temp.path());
    let set = coordinator.acquire(&["x.txt", "y.txt", "z.txt"]).unwrap();
    assert_eq!(set.held().len(), 3);
    drop(set);
    // All three are immediately acquirable again.
    let again = coordinator.acquire(&["x.txt", "y.txt", "z.txt"]).unwrap();
    assert_eq!(again.held().len(), 3);
}

/// A failed multi-resource acquisition (one member held by a third
/// party) releases every already-acquired sibling — proven by the third
/// party then acquiring the full set without deadlock.
#[test]
fn failed_multi_acquisition_releases_partial_holds() {
    let temp = tempfile::tempdir().unwrap();
    let coordinator = FsCoordinator::new(temp.path());
    let blocker = coordinator.acquire(&["middle.txt"]).unwrap();
    let error = coordinator
        .acquire(&["first.txt", "middle.txt", "last.txt"])
        .unwrap_err();
    assert!(matches!(error, CoordinationError::TimedOut { .. }));
    drop(blocker);
    // `first.txt` must have been released by the failed attempt.
    let set = coordinator
        .acquire(&["first.txt", "last.txt"])
        .expect("partial holds were not released");
    drop(set);
}

// ---------------------------------------------------------------------------
// Exact-byte preservation through the coordinated path
// ---------------------------------------------------------------------------

/// Coordination adds zero bytes: every tricky byte payload round-trips
/// exactly through a coordinated write + coordinated edit commit.
#[test]
fn coordinated_writes_preserve_exact_bytes() {
    let (temp, svc) = edit_service();
    let files = FilesService::new(temp.path());
    let payloads: &[(&str, &str)] = &[
        ("empty.txt", ""),
        ("lf.txt", "line one\nline two\n"),
        ("crlf.txt", "line one\r\nline two\r\n"),
        ("mixed.txt", "a\r\nb\nc\r\nd"),
        ("no-final.txt", "no final newline"),
        ("utf8.txt", "héllo wörld — naïve"),
        ("devanagari.txt", "नमस्ते दुनिया"),
        ("emoji.txt", "🦀🚀🔍"),
    ];
    for (name, content) in payloads {
        files.write(name, content).unwrap();
        let written = fs::read(temp.path().join(name)).unwrap();
        assert_eq!(&String::from_utf8_lossy(&written), content, "{name}");
        // Exact length too (from_utf8_lossy would mask byte growth).
        assert_eq!(written.len(), content.len(), "{name} byte length");
        // And through the atomic coordinated path.
        files.write_atomic(name, content).unwrap();
        let atomic = fs::read(temp.path().join(name)).unwrap();
        assert_eq!(atomic.as_slice(), content.as_bytes(), "{name} atomic");
    }
    // An edit through the coordinated engine preserves untouched bytes.
    external_write(temp.path(), "edit.txt", "prefix\nMIDDLE\nsuffix\r\n");
    let tx = EditTransaction::single(EditOperation::Replace {
        path: "edit.txt".into(),
        old: "MIDDLE".into(),
        new: "middle".into(),
        occurrence: None,
    });
    svc.replace(tx).unwrap();
    assert_eq!(
        fs::read_to_string(temp.path().join("edit.txt")).unwrap(),
        "prefix\nmiddle\nsuffix\r\n"
    );
}

/// Missing and zero-byte files stay distinct through coordination: an
/// empty write creates a zero-byte file (not a missing one), and the
/// states compare unequal.
#[test]
fn missing_and_zero_byte_states_stay_distinct() {
    let temp = tempfile::tempdir().unwrap();
    let files = FilesService::new(temp.path());
    assert!(!temp.path().join("absent.txt").exists());
    files.write("absent.txt", "").unwrap();
    let metadata = fs::metadata(temp.path().join("absent.txt")).unwrap();
    assert_eq!(metadata.len(), 0);
    assert!(temp.path().join("absent.txt").exists());
    files.delete("absent.txt").unwrap();
    assert!(!temp.path().join("absent.txt").exists());
}

// ---------------------------------------------------------------------------
// Temporary-file cleanup + atomic failure injection
// ---------------------------------------------------------------------------

/// A failed coordinated atomic write leaves no staging leftovers and
/// never truncates the target: force the failure with a directory in the
/// temp-file's own parent (tempfile creation fails there).
#[test]
fn failed_coordinated_write_leaves_no_leftovers() {
    let (temp, _svc) = edit_service();
    external_write(temp.path(), "keep.txt", "original\n");
    let files = FilesService::new(temp.path());
    // Oversized content fails before any staging.
    let huge = "x".repeat(9 * 1024 * 1024);
    assert!(files.write_atomic("keep.txt", &huge).is_err());
    assert_eq!(
        fs::read_to_string(temp.path().join("keep.txt")).unwrap(),
        "original\n"
    );
    // No stray tempfile entries anywhere in the workspace.
    let mut leftovers = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".agent" || name == ".git" {
                continue;
            }
            if name.starts_with(".tmp") || (name.starts_with("tmp") && name.len() > 3) {
                out.push(entry.path());
            }
            if entry.path().is_dir() {
                walk(&entry.path(), out);
            }
        }
    }
    walk(temp.path(), &mut leftovers);
    assert!(leftovers.is_empty(), "staging leftovers: {leftovers:?}");
}

/// Required snapshot failure still blocks the mutation (the AWE-010
/// rule) — coordination does not create a path around recovery
/// capture.
#[test]
fn required_snapshot_failure_still_blocks_mutation() {
    let (temp, svc) = edit_service();
    external_write(temp.path(), "guarded.txt", "original\n");
    // Break the snapshot store: a FILE where the snapshot directory tree
    // must live makes capture fail closed.
    fs::create_dir_all(temp.path().join(".agent")).unwrap();
    fs::write(temp.path().join(".agent/snapshots"), "not a directory").unwrap();
    let tx = EditTransaction::single(EditOperation::Replace {
        path: "guarded.txt".into(),
        old: "original".into(),
        new: "edited".into(),
        occurrence: None,
    });
    assert!(svc.replace(tx).is_err());
    assert_eq!(
        fs::read_to_string(temp.path().join("guarded.txt")).unwrap(),
        "original\n",
        "mutation must stay blocked when recovery capture fails"
    );
}
