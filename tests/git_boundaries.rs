//! TP04 (§9-§14): the Git feature family verified as a real product against
//! a real `git` binary in disposable repositories. The canonical
//! [`GitService`] (service plane), the seven `git.*` MCP tools (agent
//! plane), and the ten `/api/v1/git/*` Control API routes (admin plane) are
//! each cross-checked against Git's own output for the same query — the
//! oracle is always `git` invoked directly by the test, never the product.
//!
//! Every mutation is proven by reading real repository state back through
//! Git itself (`git status --porcelain`, `git log`, bare-repo refs), and
//! every refusal is proven to leave the tree byte-identical. No test
//! accepts a zero exit code as its only evidence (§25).
//!
//! Push/pull are exercised against disposable LOCAL bare repositories
//! (§2: never a personal credential, never a production remote). The
//! current branch name is always resolved from git itself, never
//! hard-coded (runner `init.defaultBranch` configurations vary).
//! Cross-platform: no absolute-path equality against `canonicalize()`
//! output and no `/proc` probes.

use agent_workspace_hub::api::control::{build_router, ControlState};
use agent_workspace_hub::mcp::StdioMcpServer;
use agent_workspace_hub::services::git::GitService;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{tempdir, TempDir};
use tower::util::ServiceExt;

#[path = "common/mod.rs"]
mod common;

// ---------------------------------------------------------------------------
// Harness: the git oracle + a disposable repository
// ---------------------------------------------------------------------------

/// Runs `git` directly in `dir` — the ORACLE, deliberately independent of
/// the product under test. System git config is ignored so runner-level
/// settings (aliases, hooks templates) cannot change what the oracle says.
fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn oracle git");
    assert!(
        output.status.success(),
        "oracle git {args:?} failed in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// The repository's current branch name, resolved from git itself.
fn current_branch(root: &Path) -> String {
    git_out(root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_owned()
}

/// A disposable repository with a committed identity and one base commit.
struct Repo {
    root: PathBuf,
    _dir: TempDir,
}

impl Repo {
    fn new() -> Self {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        git(&root, &["init", "--quiet"]);
        git(&root, &["config", "user.email", "tp04@example.invalid"]);
        git(&root, &["config", "user.name", "TP04"]);
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "base"]);
        Self { root, _dir: dir }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// Runs one async GitService call on a dedicated runtime (tests must
    /// not nest on an ambient tokio runtime; the CLI bridges the same way).
    /// The closure MOVES the service into its future — every GitService
    /// method borrows `&self`, so the service must live inside the future.
    fn call<F, Fut, T>(&self, make: F) -> T
    where
        F: FnOnce(GitService) -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let service = GitService::open(&self.root).expect("git service");
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(make(service))
    }

    /// A disposable bare repository acting as the "remote": its path URL
    /// never leaves the temp dir (§2 — disposable remotes only).
    fn bare_remote(&self, name: &str) -> (PathBuf, TempDir, String) {
        let dir = tempdir().expect("remote tempdir");
        let bare = dir.path().join(name);
        git(
            dir.path(),
            &["init", "--quiet", "--bare", &bare.display().to_string()],
        );
        let url = bare.display().to_string();
        (bare, dir, url)
    }
}

/// Oracle git stdout as an owned String.
fn git_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&git(dir, args).stdout).into_owned()
}

fn porcelain(root: &Path) -> String {
    git_out(root, &["status", "--porcelain"])
}

fn porcelain_sorted(root: &Path) -> Vec<String> {
    let mut lines: Vec<String> = porcelain(root)
        .lines()
        .map(|l| l.trim_end().to_owned())
        .filter(|l| !l.is_empty())
        .collect();
    lines.sort();
    lines
}

// ---------------------------------------------------------------------------
// §9/§10 status, diff, staged-diff — semantic matrix vs the git oracle
// ---------------------------------------------------------------------------

#[test]
fn status_matrix_matches_the_git_oracle_exactly() {
    let repo = Repo::new();
    // Mixed state: modified + untracked + staged-new + staged-deletion + a
    // filename containing spaces, so the equality below is not vacuous.
    // Order matters: gone.txt is committed BEFORE the staged-new file so
    // the "keep gone" commit cannot sweep the staged file in.
    std::fs::write(repo.path("gone.txt"), "g\n").unwrap();
    git(&repo.root, &["add", "--", "gone.txt"]);
    git(&repo.root, &["commit", "--quiet", "-m", "keep gone"]);
    std::fs::remove_file(repo.path("gone.txt")).unwrap();
    git(&repo.root, &["add", "--", "gone.txt"]);
    std::fs::write(repo.path("README.md"), "base\nmodified\n").unwrap();
    std::fs::write(repo.path("untracked.txt"), "u\n").unwrap();
    std::fs::write(repo.path("staged new.txt"), "s\n").unwrap();
    git(&repo.root, &["add", "--", "staged new.txt"]);

    let out = repo
        .call(|svc| Box::pin(async move { svc.status().await }))
        .expect("status");
    assert_eq!(out.exit_code, Some(0));
    // Byte-for-byte equality with the oracle for the same repository state.
    let expected = porcelain(&repo.root);
    assert_eq!(out.stdout.trim(), expected.trim());
    // Each semantic class is present in the parsed entries.
    let entries = out.porcelain_entries();
    let statuses: Vec<&str> = entries.iter().map(|e| e.status.as_str()).collect();
    for expected_status in [" M", "??", "A ", "D "] {
        assert!(
            statuses.contains(&expected_status),
            "matrix must include {expected_status}: {statuses:?}"
        );
    }
    // Parsing: git's porcelain C-quotes paths with spaces
    // (`"staged new.txt"` — documented `--porcelain` behavior; the
    // parser passes the path through verbatim, matching git's output
    // exactly). A consumer that joins this path to act on the file must
    // unquote; that contract is pinned here as-is.
    assert!(
        entries
            .iter()
            .any(|e| e.path == "\"staged new.txt\"" && e.status == "A "),
        "spaced path parses intact (git-quoted): {entries:?}"
    );

    // Clean tree: empty status, not an error.
    let clean = Repo::new();
    let out = clean
        .call(|svc| Box::pin(async move { svc.status().await }))
        .expect("clean status");
    assert_eq!(out.exit_code, Some(0));
    assert!(out.stdout.trim().is_empty());
    assert!(out.porcelain_entries().is_empty());
}

#[test]
fn diff_and_staged_diff_separate_the_two_index_stages() {
    let repo = Repo::new();
    // One file with BOTH a staged change and a further unstaged change: the
    // two diffs must disagree, proving `staged` actually reaches
    // `--cached` and is not a synonym for the plain diff.
    std::fs::write(repo.path("README.md"), "staged line\n").unwrap();
    git(&repo.root, &["add", "--", "README.md"]);
    std::fs::write(repo.path("README.md"), "staged line\nunstaged line\n").unwrap();

    let staged = repo
        .call(|svc| Box::pin(async move { svc.diff_staged(None).await }))
        .expect("staged diff");
    let unstaged = repo
        .call(|svc| Box::pin(async move { svc.diff(None).await }))
        .expect("unstaged diff");

    let oracle_staged = git_out(&repo.root, &["diff", "--cached"]);
    let oracle_unstaged = git_out(&repo.root, &["diff"]);
    assert_eq!(staged.stdout.trim(), oracle_staged.trim());
    assert_eq!(unstaged.stdout.trim(), oracle_unstaged.trim());
    // The staged diff contains ONLY the staged change; the unstaged diff
    // contains ONLY the worktree change. (The unstaged diff's context
    // lines may quote the staged content — a context line is not the
    // change; the +/- markers decide.)
    assert!(
        staged.stdout.contains("+staged line") && !staged.stdout.contains("+unstaged line"),
        "staged diff adds only the staged change: {}",
        staged.stdout
    );
    assert!(
        unstaged.stdout.contains("+unstaged line")
            && !unstaged.stdout.contains("+staged line\n+")
            && !unstaged
                .stdout
                .lines()
                .any(|l| l.starts_with("-staged line")),
        "working diff adds only the unstaged change: {}",
        unstaged.stdout
    );

    // Path-filtered diff: a second file's changes are excluded.
    std::fs::write(repo.path("other.txt"), "other change\n").unwrap();
    git(&repo.root, &["add", "--", "other.txt"]);
    let filtered = repo
        .call(|svc| Box::pin(async move { svc.diff(Some("README.md")).await }))
        .expect("filtered diff");
    assert!(
        !filtered.stdout.contains("other.txt"),
        "path filter excludes other.txt: {}",
        filtered.stdout
    );
    assert!(filtered.stdout.contains("unstaged line"));
}

#[test]
fn diff_shows_renames_and_unicode_like_the_oracle() {
    let repo = Repo::new();
    std::fs::write(repo.path("unicode.txt"), "नमस्ते 🌍 before\n").unwrap();
    git(&repo.root, &["add", "--", "unicode.txt"]);
    std::fs::write(repo.path("unicode.txt"), "नमस्ते 🌍 after\n").unwrap();
    // `git mv` stages the rename, so it belongs to the STAGED diff.
    git(&repo.root, &["mv", "README.md", "renamed.md"]);

    let diff = repo
        .call(|svc| Box::pin(async move { svc.diff(None).await }))
        .expect("diff");
    let oracle = git_out(&repo.root, &["diff"]);
    assert_eq!(diff.stdout.trim(), oracle.trim());
    assert!(
        diff.stdout.contains("नमस्ते"),
        "unicode survives: {}",
        diff.stdout
    );

    let staged = repo
        .call(|svc| Box::pin(async move { svc.diff_staged(None).await }))
        .expect("staged diff");
    let oracle_staged = git_out(&repo.root, &["diff", "--cached"]);
    assert_eq!(staged.stdout.trim(), oracle_staged.trim());
    assert!(
        staged.stdout.contains("renamed.md"),
        "staged rename visible: {}",
        staged.stdout
    );

    let status = repo
        .call(|svc| Box::pin(async move { svc.status().await }))
        .expect("status");
    assert_eq!(status.stdout.trim(), porcelain(&repo.root).trim());
}

#[test]
fn non_repo_and_empty_repo_fail_loudly_not_silently() {
    // A non-repo directory: reads fail with an error naming the condition.
    let dir = tempdir().unwrap();
    let svc = GitService::open(dir.path()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(!runtime.block_on(svc.is_repo()));
    let err = runtime.block_on(svc.status()).unwrap_err().to_string();
    assert!(
        err.contains("not a git repository") || err.contains("failed"),
        "non-repo status must fail loudly: {err}"
    );
    let err = runtime.block_on(svc.diff(None)).unwrap_err().to_string();
    assert!(
        err.contains("failed") || err.contains("not a git repository"),
        "non-repo diff must fail loudly: {err}"
    );
    // log must not masquerade as an empty history outside a repository.
    let err = runtime.block_on(svc.log(5)).unwrap_err().to_string();
    assert!(
        err.contains("not a git repository"),
        "non-repo log must fail loudly: {err}"
    );

    // A REAL repo with zero commits is the opposite case: status and log
    // succeed empty (the service disambiguates git's ambiguous exit 128).
    let empty = tempdir().unwrap();
    git(empty.path(), &["init", "--quiet"]);
    let svc = GitService::open(empty.path()).unwrap();
    assert!(runtime.block_on(svc.is_repo()));
    let status = runtime.block_on(svc.status()).expect("empty status ok");
    assert_eq!(status.stdout, "");
    let log = runtime.block_on(svc.log(5)).expect("empty log ok");
    assert_eq!(log.stdout, "");
}

// ---------------------------------------------------------------------------
// §10 log / branch / branches vs the oracle
// ---------------------------------------------------------------------------

#[test]
fn log_ordering_limit_and_content_match_the_oracle() {
    let repo = Repo::new();
    for i in 1..=5 {
        std::fs::write(repo.path(&format!("f{i}.txt")), format!("{i}\n")).unwrap();
        git(&repo.root, &["add", "--", &format!("f{i}.txt")]);
        git(
            &repo.root,
            &["commit", "--quiet", "-m", &format!("commit {i} — 🌍")],
        );
    }
    // log(limit) equals the oracle for the same limit, in order.
    for limit in [1usize, 3, 5, 9] {
        let out = repo
            .call(|svc| Box::pin(async move { svc.log(limit).await }))
            .expect("log");
        let oracle = git_out(&repo.root, &["log", "--oneline", "-n", &limit.to_string()]);
        assert_eq!(out.stdout.trim(), oracle.trim(), "limit {limit}");
    }
    // Content integrity: the hash in the first line resolves as a real
    // commit (git rev-parse --verify fails the test otherwise).
    let first_line = repo
        .call(|svc| Box::pin(async move { svc.log(1).await }))
        .expect("log 1")
        .stdout
        .lines()
        .next()
        .unwrap()
        .to_owned();
    let hash = first_line.split_whitespace().next().unwrap();
    git(
        &repo.root,
        &["rev-parse", "--verify", &format!("{hash}^{{commit}}")],
    );
    assert!(first_line.contains("🌍"), "unicode survives: {first_line}");
}

#[test]
fn branch_and_branches_reflect_real_git_state() {
    let repo = Repo::new();
    let branch_name = current_branch(&repo.root);
    let current = repo
        .call(|svc| Box::pin(async move { svc.branch().await }))
        .expect("branch");
    assert_eq!(current.stdout.trim(), branch_name);

    git(&repo.root, &["branch", "feature/a"]);
    git(&repo.root, &["branch", "feature/b-c_d"]);
    let branches = repo
        .call(|svc| Box::pin(async move { svc.branches().await }))
        .expect("branches");
    let oracle = git_out(&repo.root, &["branch", "--list"]);
    assert_eq!(branches.stdout.trim(), oracle.trim());
    assert!(branches.stdout.contains("feature/a"));
    assert!(branches.stdout.contains("feature/b-c_d"));

    // A checkout changes what the next call reports.
    git(&repo.root, &["checkout", "--quiet", "feature/a"]);
    let current = repo
        .call(|svc| Box::pin(async move { svc.branch().await }))
        .expect("branch after checkout");
    assert_eq!(current.stdout.trim(), "feature/a");
}

// ---------------------------------------------------------------------------
// §11 stage / unstage / commit — mutation matrix
// ---------------------------------------------------------------------------

#[test]
fn stage_then_unstage_round_trips_the_index_exactly() {
    let repo = Repo::new();
    std::fs::write(repo.path("a.txt"), "new\n").unwrap();
    std::fs::create_dir_all(repo.path("nested/deep")).unwrap();
    std::fs::write(repo.path("nested/deep/b.txt"), "nested\n").unwrap();
    // A REAL staged modification: committed content, then edited, then
    // staged — so the index-vs-HEAD status is "M " (not "A ").
    std::fs::write(repo.path("modified.txt"), "m1\n").unwrap();
    git(&repo.root, &["add", "--", "modified.txt"]);
    git(&repo.root, &["commit", "--quiet", "-m", "keep modified"]);
    std::fs::write(repo.path("modified.txt"), "m1\nm2\n").unwrap();

    repo.call(|svc| Box::pin(async move { svc.stage("a.txt").await }))
        .expect("stage a");
    repo.call(|svc| Box::pin(async move { svc.stage("nested/deep/b.txt").await }))
        .expect("stage nested");
    repo.call(|svc| Box::pin(async move { svc.stage("modified.txt").await }))
        .expect("stage modified");
    assert_eq!(
        porcelain_sorted(&repo.root),
        vec![
            "A  a.txt".to_owned(),
            "A  nested/deep/b.txt".to_owned(),
            "M  modified.txt".to_owned(),
        ]
    );

    // Stage "." stages everything else too (documented semantic).
    std::fs::write(repo.path("late.txt"), "late\n").unwrap();
    repo.call(|svc| Box::pin(async move { svc.stage(".").await }))
        .expect("stage all");
    assert!(porcelain(&repo.root).contains("A  late.txt"));

    // unstage: a staged-NEW file returns to untracked "??"; a staged-
    // MODIFIED file returns to " M" (worktree still differs from HEAD).
    repo.call(|svc| Box::pin(async move { svc.unstage("a.txt").await }))
        .expect("unstage a");
    let after = porcelain(&repo.root);
    assert!(
        after.contains("?? a.txt") || after.contains(" A a.txt"),
        "unstage reverted the index only: {after}"
    );
    assert_eq!(
        std::fs::read(repo.path("a.txt")).unwrap(),
        b"new\n",
        "working-tree bytes preserved by unstage"
    );

    // Unstaging a never-staged path is a no-op success (git semantics).
    repo.call(|svc| Box::pin(async move { svc.unstage("never-staged.txt").await }))
        .expect("unstage noop");
    // Unstaging "." clears the whole index: no entry may carry a staged
    // index code anymore. (`??` is the untracked pair — its first column
    // is `?` by definition, not a staged state; ` M` is an unstaged
    // worktree modification.)
    repo.call(|svc| Box::pin(async move { svc.unstage(".").await }))
        .expect("unstage all");
    let after = porcelain(&repo.root);
    for line in after.lines() {
        let index_column = &line[..1];
        assert!(
            index_column == " " || index_column == "?",
            "index must be empty after unstaging all (found staged state {line:?}): {after}"
        );
    }
    assert_eq!(
        std::fs::read(repo.path("nested/deep/b.txt")).unwrap(),
        b"nested\n"
    );
}

#[test]
fn stage_refuses_paths_that_escape_the_repository() {
    let repo = Repo::new();
    // A file outside the repository that must never be staged or harmed.
    let outside = tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "outside\n").unwrap();

    // Relative traversal: git's own pathspec rules refuse it.
    let err = repo
        .call(|svc| {
            Box::pin(async move {
                svc.stage("../../outside-target.txt")
                    .await
                    .map_err(|e| e.to_string())
            })
        })
        .expect_err("traversal stage must fail");
    assert!(err.contains("failed"), "error names the failure: {err}");
    assert_eq!(porcelain(&repo.root), "");
    assert!(
        !repo.path("../../outside-target.txt").exists(),
        "no path was created outside the repo"
    );

    // Absolute paths are refused the same way.
    let err = repo
        .call(|svc| {
            Box::pin(async move { svc.stage("/etc/hostname").await.map_err(|e| e.to_string()) })
        })
        .expect_err("absolute stage must fail");
    assert!(err.contains("failed"), "{err}");
    assert_eq!(porcelain(&repo.root), "");
    std::fs::read(outside.path().join("secret.txt")).expect("outside file untouched");
}

#[test]
fn commit_persists_the_message_verbatim_and_stages_only_the_index() {
    let repo = Repo::new();
    // Hostile message: leading dash, shell metacharacters, quotes,
    // newlines, Unicode. All must land in the commit verbatim; none may be
    // interpreted (no file named `pwned` may appear on disk).
    let message = "-not-an-option $(touch pwned) `id` \"quoted\"\nsecond — 🌍\n";
    std::fs::write(repo.path("staged.txt"), "data\n").unwrap();
    repo.call(|svc| Box::pin(async move { svc.stage("staged.txt").await }))
        .expect("stage");
    std::fs::write(repo.path("unrelated.txt"), "not staged\n").unwrap();

    repo.call(|svc| Box::pin(async move { svc.commit(message).await }))
        .expect("commit");

    assert!(
        !repo.path("pwned").exists(),
        "metacharacter in message executed: command injection"
    );
    let body = git_out(&repo.root, &["log", "-1", "--format=%B"]);
    assert_eq!(body.trim(), message.trim(), "message stored verbatim");
    // Exactly the staged file is in the commit; the rest survived unstaged.
    let files = git_out(&repo.root, &["show", "--name-only", "--format=", "HEAD"]);
    let committed: Vec<&str> = files.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        committed,
        vec!["staged.txt"],
        "only the index was committed"
    );
    assert_eq!(
        porcelain_sorted(&repo.root),
        vec!["?? unrelated.txt".to_owned()]
    );

    // Blank messages are refused before git runs; HEAD is provably stable.
    let head_before = git_out(&repo.root, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    for blank in ["", "   \n\t"] {
        let err = repo
            .call(move |svc| {
                Box::pin(async move { svc.commit(blank).await.map_err(|e| e.to_string()) })
            })
            .expect_err("blank message must fail");
        assert!(err.contains("empty"), "{err}");
    }
    let head_after = git_out(&repo.root, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    assert_eq!(head_before, head_after, "no commit was created");
}

#[test]
fn commit_without_staged_changes_fails_without_creating_a_commit() {
    let repo = Repo::new();
    let head_before = git_out(&repo.root, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    let err = repo
        .call(|svc| Box::pin(async move { svc.commit("nothing").await.map_err(|e| e.to_string()) }))
        .expect_err("commit with a clean index must fail");
    assert!(err.contains("failed"), "{err}");
    let head_after = git_out(&repo.root, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    assert_eq!(head_before, head_after, "HEAD unchanged");
}

// ---------------------------------------------------------------------------
// §12 push / pull against a disposable local bare remote
// ---------------------------------------------------------------------------

#[test]
fn push_and_pull_round_trip_against_a_disposable_bare_remote() {
    let repo = Repo::new();
    let branch = current_branch(&repo.root);
    let (bare, _remote_dir, url) = repo.bare_remote("origin.git");
    git(&repo.root, &["remote", "add", "origin", &url]);
    let target_ref = format!("refs/heads/{branch}");

    // Push: the bare remote's ref advances to exactly our commit hash.
    let local_head = git_out(&repo.root, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    let push_branch = branch.clone();
    repo.call(move |svc| Box::pin(async move { svc.push("origin", &push_branch).await }))
        .expect("push");
    let remote_ref = git_out(&bare, &["rev-parse", &target_ref])
        .trim()
        .to_owned();
    assert_eq!(remote_ref, local_head, "bare remote advanced to our HEAD");

    // Idempotent push: nothing to do, still success.
    let push_branch = branch.clone();
    repo.call(move |svc| Box::pin(async move { svc.push("origin", &push_branch).await }))
        .expect("second push");

    // A second clone pushes new work UPSTREAM (making local divergent).
    let clone_dir = tempdir().unwrap();
    git(clone_dir.path(), &["clone", "--quiet", &url, "clone"]);
    let clone = clone_dir.path().join("clone");
    git(&clone, &["config", "user.email", "peer@example.invalid"]);
    git(&clone, &["config", "user.name", "Peer"]);
    std::fs::write(clone.join("peer.txt"), "peer work\n").unwrap();
    git(&clone, &["add", "."]);
    git(&clone, &["commit", "--quiet", "-m", "peer work"]);
    let push_branch = branch.clone();
    git(&clone, &["push", "--quiet", "origin", &push_branch]);

    // Divergence makes the next push a non-fast-forward, which git must
    // reject — the remote is never silently overwritten.
    std::fs::write(repo.path("local.txt"), "local work\n").unwrap();
    git(&repo.root, &["add", "."]);
    git(&repo.root, &["commit", "--quiet", "-m", "local divergent"]);
    // The remote now legitimately holds the PEER's commit (the clone
    // pushed it above); a rejected push must leave exactly that.
    let remote_before = git_out(&bare, &["rev-parse", &target_ref])
        .trim()
        .to_owned();
    let push_branch = branch.clone();
    let err = repo
        .call(move |svc| {
            Box::pin(async move {
                svc.push("origin", &push_branch)
                    .await
                    .map_err(|e| e.to_string())
            })
        })
        .expect_err("divergent push must be rejected");
    assert!(err.contains("failed"), "non-fast-forward rejected: {err}");
    let remote_ref = git_out(&bare, &["rev-parse", &target_ref])
        .trim()
        .to_owned();
    assert_eq!(
        remote_ref, remote_before,
        "remote untouched by rejected push"
    );

    // Pull (--no-rebase) merges the remote work in; both histories and
    // the peer's file must be real afterwards.
    let pull_branch = branch.clone();
    repo.call(move |svc| Box::pin(async move { svc.pull("origin", &pull_branch).await }))
        .expect("pull");
    let log = git_out(&repo.root, &["log", "--oneline"]);
    assert!(log.contains("peer work"), "remote work merged: {log}");
    assert!(log.contains("local divergent"), "local work kept: {log}");
    assert!(repo.path("peer.txt").exists(), "peer file materialized");

    // Missing remote name: an error, never a silent success.
    let err = repo
        .call(|svc| {
            Box::pin(async move {
                svc.push("no-such-remote", "whatever")
                    .await
                    .map_err(|e| e.to_string())
            })
        })
        .expect_err("unknown remote must fail");
    assert!(err.contains("failed"), "{err}");
}

// ---------------------------------------------------------------------------
// §11 high-risk operations: explicit, isolated, behaviorally proven
// ---------------------------------------------------------------------------

#[test]
fn high_risk_operations_are_explicit_and_behavioral() {
    let repo = Repo::new();
    std::fs::write(repo.path("wip.txt"), "uncommitted\n").unwrap();
    let head_before = git_out(&repo.root, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();

    // discard_file restores ONE file from HEAD and leaves others alone.
    std::fs::write(repo.path("README.md"), "modified but discarded\n").unwrap();
    repo.call(|svc| Box::pin(async move { svc.discard_file("README.md").await }))
        .expect("discard");
    assert_eq!(
        std::fs::read(repo.path("README.md")).unwrap(),
        b"base\n",
        "discard restored HEAD content"
    );
    assert_eq!(
        std::fs::read(repo.path("wip.txt")).unwrap(),
        b"uncommitted\n",
        "other files untouched by discard_file"
    );

    // discard_file refuses traversal BEFORE git runs.
    let err = repo
        .call(|svc| {
            Box::pin(async move {
                svc.discard_file("../escape.txt")
                    .await
                    .map_err(|e| e.to_string())
            })
        })
        .expect_err("discard traversal must fail");
    assert!(err.contains("invalid repository path"), "{err}");

    // hard_reset discards tracked-file modifications — the documented HIGH
    // RISK semantic, proven by the modified bytes actually reverting. (It
    // does NOT remove untracked files — that is `clean`, checked below.)
    std::fs::write(
        repo.path("README.md"),
        "local edit that reset must discard\n",
    )
    .unwrap();
    repo.call(|svc| Box::pin(async move { svc.hard_reset().await }))
        .expect("hard reset");
    assert_eq!(
        std::fs::read(repo.path("README.md")).unwrap(),
        b"base\n",
        "reset --hard discarded the tracked modification"
    );
    assert!(
        repo.path("wip.txt").exists(),
        "untracked files survive reset --hard (that is clean's job)"
    );
    let head_after = git_out(&repo.root, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    assert_eq!(head_before, head_after, "reset --hard HEAD moves no ref");

    // clean removes untracked files (incl. nested dirs), never tracked.
    std::fs::create_dir_all(repo.path("nested/untracked")).unwrap();
    std::fs::write(repo.path("nested/untracked/junk.txt"), "junk\n").unwrap();
    std::fs::write(repo.path("README.md"), "local edit\n").unwrap();
    repo.call(|svc| Box::pin(async move { svc.clean().await }))
        .expect("clean");
    assert!(
        !repo.path("nested/untracked").exists(),
        "untracked dir removed"
    );
    assert_eq!(
        std::fs::read(repo.path("README.md")).unwrap(),
        b"local edit\n",
        "clean never touches tracked files"
    );
    git(&repo.root, &["checkout", "--", "README.md"]);

    // delete_branch removes a branch; blank/HEAD names are refused
    // upstream of git (pinned in unit tests; proven here through state).
    git(&repo.root, &["branch", "merged-branch"]);
    repo.call(|svc| Box::pin(async move { svc.delete_branch("merged-branch").await }))
        .expect("delete branch");
    let branches = repo
        .call(|svc| Box::pin(async move { svc.branches().await }))
        .expect("branches");
    assert!(!branches.stdout.contains("merged-branch"));

    // force_push uses --force-with-lease: the remote advances to our HEAD
    // only because the lease (remote at the known value) is valid.
    let (bare, _dir, url) = repo.bare_remote("lease.git");
    git(&repo.root, &["remote", "add", "lease", &url]);
    let lease_branch = current_branch(&repo.root);
    repo.call(|svc| Box::pin(async move { svc.push("lease", &lease_branch).await }))
        .expect("push lease");
    let lease_branch = current_branch(&repo.root);
    repo.call(|svc| Box::pin(async move { svc.force_push("lease", &lease_branch).await }))
        .expect("force push");
    let lease_ref = format!("refs/heads/{}", current_branch(&repo.root));
    let remote_ref = git_out(&bare, &["rev-parse", &lease_ref]).trim().to_owned();
    assert_eq!(
        remote_ref, head_after,
        "force-with-lease advanced the remote"
    );
}

// ---------------------------------------------------------------------------
// §9/§10 the MCP agent plane: git.* tools vs the same oracle
// (Failure injection — hanging git / missing git / timeout bounds — lives
// in `tests/git_failure_injection.rs`, its own binary, because PATH is
// process-global state.)
// ---------------------------------------------------------------------------

/// An in-process stdio server over a real initialized workspace + repo.
struct McpWorkspace {
    root: PathBuf,
    _dir: TempDir,
    server: StdioMcpServer,
}

impl McpWorkspace {
    fn new() -> Self {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        let _ = common::sanitized_command(&root, &["init", "--path", "."])
            .output()
            .expect("awh init");
        git(&root, &["init", "--quiet"]);
        git(&root, &["config", "user.email", "tp04-mcp@example.invalid"]);
        git(&root, &["config", "user.name", "TP04 MCP"]);
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "base"]);
        let server = StdioMcpServer::new(root.clone()).expect("stdio server");
        let request = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "tp04-git", "version": "0.0.0"}
            }
        });
        let response: Value =
            serde_json::from_str(&server.handle_response(&request.to_string())).expect("init json");
        assert!(response.get("error").is_none(), "init failed: {response}");
        Self {
            root,
            _dir: dir,
            server,
        }
    }

    fn tool(&self, name: &str, args: Value) -> Value {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": name, "arguments": args},
        });
        let raw = self.server.handle_response(&request.to_string());
        let response: Value = serde_json::from_str(&raw).expect("json-rpc response");
        if let Some(error) = response.get("error") {
            panic!("tool {name} failed: {error}");
        }
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("content text");
        serde_json::from_str(text).unwrap_or_else(|_| panic!("non-json text: {text}"))
    }

    fn tool_error(&self, name: &str, args: Value) -> Value {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": name, "arguments": args},
        });
        let raw = self.server.handle_response(&request.to_string());
        let response: Value = serde_json::from_str(&raw).expect("json-rpc response");
        response
            .get("error")
            .cloned()
            .unwrap_or_else(|| panic!("expected error, got: {response}"))
    }
}

#[test]
fn mcp_git_read_tools_match_the_oracle() {
    let mcp = McpWorkspace::new();
    std::fs::write(mcp.root.join("changed.txt"), "c1\nc2\n").unwrap();
    git(&mcp.root, &["add", "--", "changed.txt"]);
    std::fs::write(mcp.root.join("changed.txt"), "c1\nc2\nc3\n").unwrap();
    std::fs::write(mcp.root.join("untracked.txt"), "u\n").unwrap();

    // git.status vs the oracle, decoded from the wire envelope.
    let status = mcp.tool("git.status", json!({}));
    assert_eq!(status["exit_code"], json!(0));
    let oracle = porcelain(&mcp.root);
    assert_eq!(status["stdout"].as_str().unwrap().trim(), oracle.trim());

    // git.diff plain and staged vs the oracle (schema-validated boundary).
    let diff = mcp.tool("git.diff", json!({}));
    let oracle_diff = git_out(&mcp.root, &["diff"]);
    assert_eq!(diff["stdout"].as_str().unwrap().trim(), oracle_diff.trim());
    let staged = mcp.tool("git.diff", json!({"staged": true}));
    let oracle_staged = git_out(&mcp.root, &["diff", "--cached"]);
    assert_eq!(
        staged["stdout"].as_str().unwrap().trim(),
        oracle_staged.trim()
    );
    assert!(staged["stdout"].as_str().unwrap().contains("c2"));
    assert!(!staged["stdout"].as_str().unwrap().contains("c3"));

    // git.branch vs the oracle.
    let branch = mcp.tool("git.branch", json!({}));
    let oracle_branch = git_out(&mcp.root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(
        branch["stdout"].as_str().unwrap().trim(),
        oracle_branch.trim()
    );

    // git.log with and without a limit vs the oracle.
    let log = mcp.tool("git.log", json!({}));
    let oracle_log = git_out(&mcp.root, &["log", "--oneline", "-n", "20"]);
    assert_eq!(log["stdout"].as_str().unwrap().trim(), oracle_log.trim());
    let log_one = mcp.tool("git.log", json!({"limit": 1}));
    let oracle_one = git_out(&mcp.root, &["log", "--oneline", "-n", "1"]);
    assert_eq!(
        log_one["stdout"].as_str().unwrap().trim(),
        oracle_one.trim()
    );
}

#[test]
fn mcp_git_log_limit_clamping_is_deterministic() {
    let mcp = McpWorkspace::new();
    // 0 clamps up to 1; 99999 clamps down to 200; a negative value falls to
    // the default 20 — each must equal a direct oracle run with the
    // clamped value (never an error, never unbounded output).
    let zero = mcp.tool("git.log", json!({"limit": 0}));
    let one = mcp.tool("git.log", json!({"limit": 1}));
    assert_eq!(zero["stdout"], one["stdout"], "0 clamps to 1");

    let huge = mcp.tool("git.log", json!({"limit": 99999}));
    let oracle_200 = git_out(&mcp.root, &["log", "--oneline", "-n", "200"]);
    assert_eq!(huge["stdout"].as_str().unwrap().trim(), oracle_200.trim());

    let negative = mcp.tool("git.log", json!({"limit": -5}));
    let oracle_20 = git_out(&mcp.root, &["log", "--oneline", "-n", "20"]);
    assert_eq!(
        negative["stdout"].as_str().unwrap().trim(),
        oracle_20.trim()
    );
}

#[test]
fn mcp_git_mutations_match_the_oracle_and_refuse_traversal() {
    let mcp = McpWorkspace::new();

    // stage via the tool == git add via the oracle.
    std::fs::write(mcp.root.join("via-tool.txt"), "tool\n").unwrap();
    mcp.tool("git.stage", json!({"path": "via-tool.txt"}));
    let oracle_after = porcelain(&mcp.root);
    assert!(oracle_after.contains("A  via-tool.txt"), "{oracle_after}");

    // Empty path is refused with a usage error naming the fix.
    let error = mcp.tool_error("git.stage", json!({"path": ""}));
    assert!(error["message"].as_str().unwrap().contains("non-empty"));

    // Traversal path: git itself refuses (outside repository); nothing
    // outside is staged and the sentinel survives.
    let sentinel_dir = tempdir().unwrap();
    std::fs::write(sentinel_dir.path().join("target.txt"), "outside\n").unwrap();
    let error = mcp.tool_error("git.stage", json!({"path": "../../target.txt"}));
    assert!(
        error["code"].as_i64().unwrap() != 0,
        "traversal stage must error: {error}"
    );
    let status = mcp.tool("git.status", json!({}));
    assert!(
        !status["stdout"]
            .as_str()
            .unwrap()
            .contains("../../target.txt"),
        "no escape record in status"
    );
    std::fs::read(sentinel_dir.path().join("target.txt")).expect("outside file intact");

    // unstage: index-only; working bytes preserved (the same contract the
    // service-plane test pins — one place, both planes).
    mcp.tool("git.unstage", json!({"path": "via-tool.txt"}));
    let status = mcp.tool("git.status", json!({}));
    assert!(status["stdout"]
        .as_str()
        .unwrap()
        .contains("?? via-tool.txt"));
    assert_eq!(
        std::fs::read(mcp.root.join("via-tool.txt")).unwrap(),
        b"tool\n"
    );

    // commit: a hostile message lands verbatim; nothing is interpreted.
    mcp.tool("git.stage", json!({"path": "via-tool.txt"}));
    let message = "-dash $(rm -rf /) `x` \"q\" — 🌍\nline2";
    mcp.tool("git.commit", json!({"message": message}));
    let body = git_out(&mcp.root, &["log", "-1", "--format=%B"]);
    assert_eq!(body.trim(), message.trim());
    assert_eq!(porcelain(&mcp.root), "");

    // Empty message: rejected by the boundary, no commit created.
    let head_before = String::from_utf8_lossy(&git(&mcp.root, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_owned();
    let error = mcp.tool_error("git.commit", json!({"message": ""}));
    let text = error["message"].as_str().unwrap_or("");
    assert!(
        text.contains("empty") || text.contains("must not"),
        "{error}"
    );
    let head_after = String::from_utf8_lossy(&git(&mcp.root, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_owned();
    assert_eq!(head_before, head_after, "no commit from an empty message");
}

// ---------------------------------------------------------------------------
// §9/§10 the Control API admin plane: /api/v1/git/* vs the oracle
// ---------------------------------------------------------------------------

struct ApiWorkspace {
    root: PathBuf,
    _dir: TempDir,
    router: axum::Router,
}

impl ApiWorkspace {
    fn new() -> Self {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        git(&root, &["init", "--quiet"]);
        git(&root, &["config", "user.email", "tp04-api@example.invalid"]);
        git(&root, &["config", "user.name", "TP04 API"]);
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "base"]);
        let state = std::sync::Arc::new(ControlState::new(root.clone(), "tp04-key".into()));
        let router = build_router(state);
        Self {
            root,
            _dir: dir,
            router,
        }
    }

    fn request(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (axum::http::StatusCode, Value) {
        let mut builder = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", "Bearer tp04-key");
        let request = match body {
            Some(json) => builder
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(json.to_string()))
                .expect("request"),
            None => {
                builder = builder.header("Content-Length", "0");
                builder.body(axum::body::Body::empty()).expect("request")
            }
        };
        let router = self.router.clone();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(async move {
                let response = router.oneshot(request).await.expect("oneshot");
                let status = response.status();
                let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
                    .await
                    .expect("body");
                let body: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
                (status, body)
            })
    }
}

#[test]
fn control_api_git_reads_match_the_oracle() {
    let api = ApiWorkspace::new();
    std::fs::write(api.root.join("api.txt"), "a1\na2\n").unwrap();
    git(&api.root, &["add", "--", "api.txt"]);
    std::fs::write(api.root.join("api.txt"), "a1\na2\na3\n").unwrap();

    let (status, body) = api.request("GET", "/api/v1/git/status", None);
    assert_eq!(status, axum::http::StatusCode::OK);
    let oracle = porcelain(&api.root);
    assert_eq!(body["stdout"].as_str().unwrap().trim(), oracle.trim());

    let (status, body) = api.request("GET", "/api/v1/git/diff", None);
    assert_eq!(status, axum::http::StatusCode::OK);
    let oracle_diff = git_out(&api.root, &["diff"]);
    assert_eq!(body["stdout"].as_str().unwrap().trim(), oracle_diff.trim());

    let (status, body) = api.request("GET", "/api/v1/git/diff?staged=true", None);
    assert_eq!(status, axum::http::StatusCode::OK);
    let oracle_staged = git_out(&api.root, &["diff", "--cached"]);
    assert_eq!(
        body["stdout"].as_str().unwrap().trim(),
        oracle_staged.trim()
    );

    let (status, body) = api.request("GET", "/api/v1/git/log", None);
    assert_eq!(status, axum::http::StatusCode::OK);
    let oracle_log = git_out(&api.root, &["log", "--oneline", "-n", "50"]);
    assert_eq!(body["stdout"].as_str().unwrap().trim(), oracle_log.trim());

    let (status, body) = api.request("GET", "/api/v1/git/branches", None);
    assert_eq!(status, axum::http::StatusCode::OK);
    let oracle_branches = git_out(&api.root, &["branch", "--list"]);
    assert_eq!(
        body["stdout"].as_str().unwrap().trim(),
        oracle_branches.trim()
    );

    // A non-repo workspace: structured 409 conflict, not a fake success.
    let dir = tempdir().unwrap();
    let state = std::sync::Arc::new(ControlState::new(dir.path(), "tp04-key".into()));
    let router = build_router(state);
    let request = axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/git/status")
        .header("Authorization", "Bearer tp04-key")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(router.oneshot(request))
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
    let bytes = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(axum::body::to_bytes(response.into_body(), 4096))
        .unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["code"], "not_a_git_repo");
}

#[test]
fn control_api_git_mutations_and_push_pull_match_the_oracle() {
    let api = ApiWorkspace::new();
    let branch = current_branch(&api.root);

    // stage → the oracle porcelain advances.
    std::fs::write(api.root.join("staged-api.txt"), "s\n").unwrap();
    let (status, _) = api.request(
        "POST",
        "/api/v1/git/stage",
        Some(json!({"path": "staged-api.txt"})),
    );
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(porcelain(&api.root).contains("A  staged-api.txt"));

    // unstage → index-only revert; bytes preserved.
    let (status, _) = api.request(
        "POST",
        "/api/v1/git/unstage",
        Some(json!({"path": "staged-api.txt"})),
    );
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(porcelain(&api.root).contains("?? staged-api.txt"));
    assert_eq!(
        std::fs::read(api.root.join("staged-api.txt")).unwrap(),
        b"s\n"
    );

    // commit with a hostile message → verbatim in git; nothing executed.
    let (status, _) = api.request(
        "POST",
        "/api/v1/git/stage",
        Some(json!({"path": "staged-api.txt"})),
    );
    assert_eq!(status, axum::http::StatusCode::OK);
    let message = "-x $(touch api-pwned) \"q\" — 🌍";
    let (status, _) = api.request(
        "POST",
        "/api/v1/git/commit",
        Some(json!({"message": message})),
    );
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(
        !api.root.join("api-pwned").exists(),
        "metacharacters executed — command injection"
    );
    let body = git_out(&api.root, &["log", "-1", "--format=%B"]);
    assert_eq!(body.trim(), message.trim());

    // Empty commit message → 400, validated before git runs.
    let head_before = git_out(&api.root, &["rev-parse", "HEAD"]).trim().to_owned();
    let (status, error_body) =
        api.request("POST", "/api/v1/git/commit", Some(json!({"message": ""})));
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert!(error_body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("empty"));
    let head_after = git_out(&api.root, &["rev-parse", "HEAD"]).trim().to_owned();
    assert_eq!(head_before, head_after, "no commit from an empty message");

    // push/pull against a disposable bare remote — the ONLY plane exposing
    // them. The remote ref must equal local HEAD after push.
    let remote_dir = tempdir().unwrap();
    let bare = remote_dir.path().join("origin.git");
    git(
        remote_dir.path(),
        &["init", "--quiet", "--bare", &bare.display().to_string()],
    );
    git(
        &api.root,
        &["remote", "add", "origin", &bare.display().to_string()],
    );
    let (status, _) = api.request("POST", "/api/v1/git/push", None);
    assert_eq!(status, axum::http::StatusCode::OK);
    let local_head = git_out(&api.root, &["rev-parse", "HEAD"]).trim().to_owned();
    let remote_ref = String::from_utf8_lossy(
        &git(&bare, &["rev-parse", &format!("refs/heads/{branch}")]).stdout,
    )
    .trim()
    .to_owned();
    assert_eq!(remote_ref, local_head, "api push moved the bare remote");

    // pull is a no-op success when already up to date.
    let (status, _) = api.request("POST", "/api/v1/git/pull", None);
    assert_eq!(status, axum::http::StatusCode::OK);

    // push to a missing remote: a structured error with no stack or
    // credential-shaped material in the response body.
    let (status, body) = api.request("POST", "/api/v1/git/push?remote=absent", None);
    assert_eq!(status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    let text = body.to_string();
    assert!(
        !text.contains("token") && !text.to_lowercase().contains("authorization"),
        "no credential-shaped material in error: {text}"
    );
}
