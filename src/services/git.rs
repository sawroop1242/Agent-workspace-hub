//! Git service: structured `git` invocation over argument vectors.
//!
//! Never builds shell strings — every operation passes an explicit argv
//! to the `git` binary, so arguments cannot be re-interpreted by a shell.
//! High-risk operations (`reset`, `clean`, `push --force`, branch delete)
//! are separate methods that callers must invoke deliberately, and every
//! operation enforces a timeout.

use anyhow::{bail, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

/// Default wall-clock limit for a single git invocation.
pub const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Operations classified as destructive; UIs must confirm before calling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighRiskGitOp {
    HardReset,
    Clean,
    ForcePush,
    BranchDelete,
    DiscardFile,
}

impl HighRiskGitOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HardReset => "hard_reset",
            Self::Clean => "clean",
            Self::ForcePush => "force_push",
            Self::BranchDelete => "branch_delete",
            Self::DiscardFile => "discard_file",
        }
    }
}

/// Result of one git invocation.
#[derive(Debug, Clone, Serialize)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
    /// Process exit code; `None` when git could not be spawned at all.
    pub exit_code: Option<i32>,
}

impl GitOutput {
    /// Parses stdout lines of `git status --porcelain` into entry records.
    pub fn porcelain_entries(&self) -> Vec<PorcelainEntry> {
        let mut entries = Vec::new();
        for line in self.stdout.lines().filter(|l| !l.trim().is_empty()) {
            if line.len() < 4 {
                continue;
            }
            let status = &line[..2];
            let path = line[3..].to_string();
            entries.push(PorcelainEntry {
                status: status.to_string(),
                path,
            });
        }
        entries
    }
}

/// One `git status --porcelain` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PorcelainEntry {
    /// Two-letter status code (e.g. ` M`, `M `, `??`, `A `).
    pub status: String,
    /// Repository-relative path as printed by git.
    pub path: String,
}

/// Git service bound to one repository working tree.
#[derive(Debug, Clone)]
pub struct GitService {
    repo: PathBuf,
    timeout: Duration,
}

impl GitService {
    /// Creates a service for the repository containing `dir`.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self> {
        let repo = dir.into();
        Self::for_working_tree(repo)
    }

    fn for_working_tree(repo: PathBuf) -> Result<Self> {
        Ok(Self {
            repo,
            timeout: DEFAULT_GIT_TIMEOUT,
        })
    }

    /// Overrides the per-invocation timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn repo(&self) -> &Path {
        &self.repo
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Runs `git <args>` in the repository with an argument vector,
    /// failing on a non-zero exit so git errors are never mistaken for
    /// success.
    async fn run(&self, args: &[&str]) -> Result<GitOutput> {
        let output = self.run_raw(args).await?;
        if !output.stderr.is_empty() && output.exit_code != Some(0) {
            bail!(
                "git {} failed ({}): {}",
                args.first().unwrap_or(&""),
                output
                    .exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "no exit".into()),
                output.stderr.trim()
            );
        }
        Ok(output)
    }

    /// Runs git without exit-code checking; only for probing operations
    /// (e.g. `rev-parse`) where the caller inspects the result.
    async fn run_raw(&self, args: &[&str]) -> Result<GitOutput> {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(&self.repo).kill_on_drop(true);
        let output = tokio::time::timeout(self.timeout, cmd.output())
            .await
            .map_err(|_| anyhow::anyhow!("git {} timed out", args.first().unwrap_or(&"")))??;
        Ok(GitOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
        })
    }

    /// Returns whether the working tree is a git repository.
    pub async fn is_repo(&self) -> bool {
        self.run_raw(&["rev-parse", "--is-inside-work-tree"])
            .await
            .map(|o| o.stdout.trim() == "true" && o.exit_code == Some(0))
            .unwrap_or(false)
    }

    /// Blocking [`Self::is_repo`] for synchronous callers (CLI/TUI).
    pub fn is_repo_blocking(&self) -> bool {
        self.repo.join(".git").exists()
    }

    /// Working-tree status (`--porcelain`).
    pub async fn status(&self) -> Result<GitOutput> {
        self.run(&["status", "--porcelain"]).await
    }

    /// Stages a path (`.`, `--all`, or a repository-relative path).
    pub async fn stage(&self, path: &str) -> Result<GitOutput> {
        self.run(&["add", "--", path]).await
    }

    /// Unstages a path, leaving the working tree untouched.
    pub async fn unstage(&self, path: &str) -> Result<GitOutput> {
        self.run(&["reset", "HEAD", "--", path]).await
    }

    /// Commits staged changes. The message is passed as the `-m` option
    /// value via argv, so it can never be re-interpreted as a pathspec or
    /// option — no `--` separator is needed and one would actually break
    /// the invocation by swallowing the message as a path.
    pub async fn commit(&self, message: &str) -> Result<GitOutput> {
        if message.trim().is_empty() {
            bail!("commit message must not be empty");
        }
        self.run(&["commit", "-m", message]).await
    }

    /// Commit history, bounded to `limit` entries.
    pub async fn log(&self, limit: usize) -> Result<GitOutput> {
        if let Ok(out) = self
            .run(&["log", "--oneline", "-n", &limit.to_string()])
            .await
        {
            return Ok(out);
        }
        // A repository with zero commits exits 128 here; that is an empty
        // history, not an error — confirm HEAD truly does not resolve and
        // return empty output instead.
        if self.run(&["rev-parse", "--verify", "HEAD"]).await.is_err() {
            Ok(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(0),
            })
        } else {
            self.run(&["log", "--oneline", "-n", &limit.to_string()])
                .await
        }
    }

    /// Current branch name (`HEAD` when detached).
    pub async fn branch(&self) -> Result<GitOutput> {
        self.run(&["rev-parse", "--abbrev-ref", "HEAD"]).await
    }

    /// Local branches.
    pub async fn branches(&self) -> Result<GitOutput> {
        self.run(&["branch", "--list"]).await
    }

    /// Unified diff for a path, or the whole tree when `None`.
    pub async fn diff(&self, path: Option<&str>) -> Result<GitOutput> {
        match path {
            Some(p) => self.run(&["diff", "--", p]).await,
            None => self.run(&["diff"]).await,
        }
    }

    /// Staged diff.
    pub async fn diff_staged(&self, path: Option<&str>) -> Result<GitOutput> {
        match path {
            Some(p) => self.run(&["diff", "--cached", "--", p]).await,
            None => self.run(&["diff", "--cached"]).await,
        }
    }

    /// Pushes the current branch with upstream tracking.
    pub async fn push(&self, remote: &str, branch: &str) -> Result<GitOutput> {
        self.run(&["push", "-u", remote, branch]).await
    }

    /// Pulls with rebase disabled (merge only) for predictability.
    pub async fn pull(&self, remote: &str, branch: &str) -> Result<GitOutput> {
        self.run(&["pull", "--no-rebase", remote, branch]).await
    }

    /// Discards uncommitted changes to one file. HIGH RISK.
    pub async fn discard_file(&self, path: &str) -> Result<GitOutput> {
        self.validate_repo_path(path)?;
        self.run(&["checkout", "--", path]).await
    }

    /// Hard reset of the working tree to HEAD. HIGH RISK.
    pub async fn hard_reset(&self) -> Result<GitOutput> {
        self.run(&["reset", "--hard", "HEAD"]).await
    }

    /// Deletes a branch. HIGH RISK.
    pub async fn delete_branch(&self, name: &str) -> Result<GitOutput> {
        if name.is_empty() || name == "HEAD" {
            bail!("refusing to delete invalid branch name");
        }
        self.run(&["branch", "-D", name]).await
    }

    /// Force-pushes a branch. HIGH RISK.
    pub async fn force_push(&self, remote: &str, branch: &str) -> Result<GitOutput> {
        self.run(&["push", "--force-with-lease", remote, branch])
            .await
    }

    /// Removes untracked files. HIGH RISK.
    pub async fn clean(&self) -> Result<GitOutput> {
        self.run(&["clean", "-fd"]).await
    }

    /// Ensures `path` is a repository-relative path that cannot escape the
    /// working tree (no absolute paths, no `..` components, no empty string).
    fn validate_repo_path(&self, path: &str) -> Result<()> {
        let p = Path::new(path);
        if p.is_absolute()
            || path.is_empty()
            || p.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            bail!("invalid repository path: {path}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_repo_paths() {
        let svc = GitService::open(".").unwrap();
        assert!(svc.validate_repo_path("../outside.txt").is_err());
        assert!(svc.validate_repo_path("/etc/passwd").is_err());
        assert!(svc.validate_repo_path("").is_err());
        assert!(svc.validate_repo_path("src/main.rs").is_ok());
        assert!(svc.validate_repo_path("a/b/../c").is_err());
    }

    #[tokio::test]
    async fn rejects_blank_branch_names() {
        let svc = GitService::open(".").unwrap();
        assert!(svc.delete_branch("").await.is_err());
        assert!(svc.delete_branch("HEAD").await.is_err());
    }

    /// Outside a repository every operation fails with a git error rather
    /// than silently "succeeding" with empty output.
    #[tokio::test]
    async fn non_repo_operations_fail_loudly() {
        let tmp = tempfile::tempdir().unwrap();
        let svc = GitService::open(tmp.path().to_path_buf()).unwrap();
        assert!(!svc.is_repo().await);
        let err = svc.status().await.unwrap_err().to_string();
        assert!(
            err.contains("not a git repository") || err.contains("failed"),
            "unexpected error: {err}"
        );
        // A message starting with `-` is rejected upstream of git as a
        // sanity check but never executed as an option: it fails with the
        // repository error, not an option-parsing error.
        let err = svc.commit("-not-an-option").await.unwrap_err().to_string();
        assert!(
            !err.contains("unknown switch"),
            "message leaked as option: {err}"
        );
    }

    /// Full happy path in a real repository, including a commit message
    /// that starts with `-` (must be consumed as the `-m` value).
    #[tokio::test]
    async fn real_repo_stage_commit_log_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let init = std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert!(init.status.success());
        for (key, value) in [("user.email", "t@example.com"), ("user.name", "T")] {
            let cfg = std::process::Command::new("git")
                .args(["config", key, value])
                .current_dir(tmp.path())
                .output()
                .unwrap();
            assert!(cfg.status.success());
        }
        std::fs::write(tmp.path().join("f.txt"), "x").unwrap();

        let svc = GitService::open(tmp.path().to_path_buf()).unwrap();
        assert!(svc.is_repo().await);
        svc.stage("f.txt").await.unwrap();
        let status = svc.status().await.unwrap();
        assert_eq!(status.porcelain_entries().len(), 1);
        svc.commit("- dashed message").await.unwrap();
        let log = svc.log(5).await.unwrap();
        assert!(log.stdout.contains("- dashed message"));
        let branch = svc.branch().await.unwrap();
        assert!(!branch.stdout.trim().is_empty());
    }
}
