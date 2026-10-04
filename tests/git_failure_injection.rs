//! TP04 (§27): failure injection for the Git boundary — git executable
//! failure and the per-invocation timeout, exercised with REAL child
//! processes (a hanging `git` wrapper, and a PATH with no git at all).
//!
//! This is deliberately its OWN test binary: PATH is process-global state,
//! and every test in a binary runs in parallel threads of one process.
//! Isolating the mutation here means no sibling test in this process can
//! observe the mutated PATH (the same discipline the `dirs`/HOME lesson in
//! AGENTS.md established). Sibling test binaries are separate processes
//! and are unaffected.
//!
//! Unix-only: the hanging wrapper is a shell script (§30 cross-platform
//! honesty — a Windows equivalent would need a `.bat`+PATHEXT scheme; the
//! timeout logic itself is platform-independent tokio code already proven
//! by the 3-OS suite).

#![cfg(unix)]

use agent_workspace_hub::services::git::GitService;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tempfile::tempdir;

/// Restores the ambient PATH captured before any mutation.
struct PathGuard {
    original: String,
}

impl PathGuard {
    fn set(path: &str) -> Self {
        let original = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", path);
        Self { original }
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        std::env::set_var("PATH", &self.original);
    }
}

/// A disposable repository with one commit.
fn repo_with_commit(root: &Path) {
    for args in [
        &["init", "--quiet"][..],
        &["config", "user.email", "tp04-inject@example.invalid"][..],
        &["config", "user.name", "TP04 Inject"][..],
    ] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("spawn git")
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }
    std::fs::write(root.join("README.md"), "base\n").unwrap();
    let ok = Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok);
    let ok = Command::new("git")
        .args(["commit", "--quiet", "-m", "base"])
        .current_dir(root)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok);
}

/// The full PATH is restored even on assertion failure, and both phases
/// run inside ONE test so the binary has no parallel sibling to race.
#[test]
fn git_executable_failure_and_timeout_fail_closed() {
    // ---- Phase 1: timeout is bounded --------------------------------
    // A `git` wrapper that sleeps 30s, resolved FIRST on PATH. The real
    // thing under test is AWH's own tokio timeout around the child — a
    // 300ms budget must cut the invocation off and surface a deterministic
    // "timed out" error, never a hang and never a partial success.
    let dir = tempdir().expect("tempdir");
    repo_with_commit(dir.path());
    let wrapper_dir = tempdir().expect("wrapper tempdir");
    let wrapper = wrapper_dir.path().join("git");
    std::fs::write(&wrapper, "#!/bin/sh\nexec sleep 30\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mutated = format!(
        "{}:{}",
        wrapper_dir.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let service = GitService::open(dir.path())
        .unwrap()
        .with_timeout(Duration::from_millis(300));
    let _guard = PathGuard::set(&mutated);

    let started = std::time::Instant::now();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(service.status());
    let elapsed = started.elapsed();
    drop(_guard);

    let error = result.expect_err("a hanging git must produce an error, not a hang");
    assert!(
        error.to_string().contains("timed out"),
        "deterministic timeout error: {error}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "bounded far below the 30s child sleep: {elapsed:?}"
    );
    // Fail closed means no fabricated output snuck through.
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(status.status.success(), "real git still fine after restore");
    assert_eq!(String::from_utf8_lossy(&status.stdout), "");

    // ---- Phase 2: git missing entirely ------------------------------
    // A PATH with no git: spawn fails; the boundary reports an error for
    // every operation class — never a silent zero-exit success, and the
    // timeout must NOT swallow the spawn failure as a fake timeout.
    let empty = tempdir().expect("empty path dir");
    let service = GitService::open(dir.path())
        .unwrap()
        .with_timeout(Duration::from_secs(5));
    let _guard = PathGuard::set(&empty.path().display().to_string());

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for result in [
        runtime.block_on(service.status()),
        runtime.block_on(service.diff(None)),
        runtime.block_on(service.stage(".")),
        runtime.block_on(service.commit("x")),
    ] {
        let error = result.expect_err("missing git must fail closed");
        assert!(
            !error.to_string().contains("timed out"),
            "spawn failure must not masquerade as a timeout: {error}"
        );
    }
    // is_repo degrades to false rather than panicking.
    assert!(!runtime.block_on(service.is_repo()));
}
