//! TUI backend abstraction.
//!
//! The TUI never manipulates the filesystem, Git, or processes directly.
//! It calls a [`WorkspaceBackend`], which is implemented locally by
//! [`LocalBackend`] (backed by the application services in
//! [`crate::services`]) and remotely by a future HTTPS backend. This keeps
//! the same UI usable against local, LAN, or cloud AWH without embedding
//! transport-specific logic in the screens.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::services::files::{FileMeta, ListEntry, SearchHit};
use crate::services::git::GitOutput;
use crate::services::terminal::ExecOutcome;

/// Snapshot of workspace state for the Dashboard screen.
#[derive(Debug, Clone, Default)]
pub struct DashboardSnapshot {
    /// Absolute path of the workspace root.
    pub root: PathBuf,
    /// Number of projects managed by the workspace.
    pub project_count: usize,
    /// True when the root is inside a Git work tree.
    pub is_git_repo: bool,
    /// Current branch name, when known.
    pub branch: Option<String>,
    /// Number of dirty entries in `git status --porcelain`.
    pub dirty_entries: usize,
    /// Non-fatal problems surfaced as warnings on the dashboard.
    pub warnings: Vec<String>,
}

/// Backend operations available to every TUI screen.
pub trait WorkspaceBackend {
    fn dashboard(&self) -> Result<DashboardSnapshot>;

    fn list_projects(&self) -> Result<Vec<String>>;
    fn create_project(&self, name: &str) -> Result<()>;
    fn delete_project(&self, name: &str) -> Result<()>;

    fn list_dir(&self, relative: &str) -> Result<Vec<ListEntry>>;
    fn read_file(&self, relative: &str) -> Result<String>;
    /// Returns true when the write happened, false when it was refused.
    fn write_file(&self, relative: &str, content: &str) -> Result<bool>;
    fn delete_path(&self, relative: &str) -> Result<()>;
    fn rename_path(&self, from: &str, to: &str) -> Result<()>;
    fn create_dir(&self, relative: &str) -> Result<()>;
    fn meta(&self, relative: &str) -> Result<FileMeta>;
    fn search_files(&self, needle: &str, limit: usize) -> Result<Vec<SearchHit>>;

    fn git_status(&self) -> Result<GitOutput>;
    fn git_log(&self, limit: usize) -> Result<GitOutput>;
    fn git_commit(&self, message: &str) -> Result<GitOutput>;
    fn git_stage(&self, path: &str) -> Result<GitOutput>;
    fn git_unstage(&self, path: &str) -> Result<GitOutput>;
    fn git_diff(&self, staged: bool, path: Option<&str>) -> Result<GitOutput>;

    fn terminal_run(&self, program: &str, args: &[String]) -> Result<ExecOutcome>;
}

/// Local implementation over the shared application services. The TUI event
/// loop is synchronous, so the backend owns the tokio runtime the async
/// services require.
pub struct LocalBackend {
    root: PathBuf,
    runtime: tokio::runtime::Runtime,
}

impl LocalBackend {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime"),
        }
    }

    /// Absolute workspace root this backend operates on.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl WorkspaceBackend for LocalBackend {
    fn dashboard(&self) -> Result<DashboardSnapshot> {
        let project_count = crate::core::project::ProjectStore::list(&self.root)?.len();

        let git = crate::services::git::GitService::open(&self.root)?;
        let is_git_repo = self.runtime.block_on(git.is_repo());
        let mut branch = None;
        let mut dirty_entries = 0;
        if is_git_repo {
            if let Ok(status) = self.runtime.block_on(git.status()) {
                dirty_entries = status.porcelain_entries().len();
            }
            if let Ok(out) = self.runtime.block_on(git.branch()) {
                let name = out.stdout.trim();
                if !name.is_empty() {
                    branch = Some(name.to_string());
                }
            }
        }
        Ok(DashboardSnapshot {
            root: self.root.clone(),
            project_count,
            is_git_repo,
            branch,
            dirty_entries,
            warnings: Vec::new(),
        })
    }

    fn list_projects(&self) -> Result<Vec<String>> {
        Ok(crate::core::project::ProjectStore::list(&self.root)?
            .into_iter()
            .map(|p| p.name)
            .collect())
    }

    fn create_project(&self, name: &str) -> Result<()> {
        crate::services::projects::validate_project_name(name)?;
        crate::core::workspace::Workspace::new(&self.root).create_project(name)?;
        Ok(())
    }

    fn delete_project(&self, name: &str) -> Result<()> {
        crate::services::projects::validate_project_name(name)?;
        let path = crate::core::workspace::Workspace::new(&self.root).project_path(name);
        if !path.is_dir() {
            anyhow::bail!("project not found: {name}");
        }
        std::fs::remove_dir_all(path)?;
        Ok(())
    }

    fn list_dir(&self, relative: &str) -> Result<Vec<ListEntry>> {
        let svc = crate::services::files::FilesService::new(&self.root);
        svc.list(relative)
    }

    fn read_file(&self, relative: &str) -> Result<String> {
        let svc = crate::services::files::FilesService::new(&self.root);
        svc.read(relative)
    }

    fn write_file(&self, relative: &str, content: &str) -> Result<bool> {
        let svc = crate::services::files::FilesService::new(&self.root);
        svc.write(relative, content)?;
        Ok(true)
    }

    fn delete_path(&self, relative: &str) -> Result<()> {
        let svc = crate::services::files::FilesService::new(&self.root);
        svc.delete(relative)
    }

    fn rename_path(&self, from: &str, to: &str) -> Result<()> {
        let svc = crate::services::files::FilesService::new(&self.root);
        svc.rename(from, to)
    }

    fn create_dir(&self, relative: &str) -> Result<()> {
        let svc = crate::services::files::FilesService::new(&self.root);
        svc.create_dir(relative)
    }

    fn meta(&self, relative: &str) -> Result<FileMeta> {
        let svc = crate::services::files::FilesService::new(&self.root);
        svc.meta(relative)
    }

    fn search_files(&self, needle: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let svc = crate::services::files::FilesService::new(&self.root);
        svc.search(needle, limit)
    }

    fn git_status(&self) -> Result<GitOutput> {
        let git = crate::services::git::GitService::open(&self.root)?;
        self.runtime.block_on(git.status())
    }

    fn git_log(&self, limit: usize) -> Result<GitOutput> {
        let git = crate::services::git::GitService::open(&self.root)?;
        self.runtime.block_on(git.log(limit))
    }

    fn git_commit(&self, message: &str) -> Result<GitOutput> {
        let git = crate::services::git::GitService::open(&self.root)?;
        self.runtime.block_on(git.commit(message))
    }

    fn git_stage(&self, path: &str) -> Result<GitOutput> {
        let git = crate::services::git::GitService::open(&self.root)?;
        self.runtime.block_on(git.stage(path))
    }

    fn git_unstage(&self, path: &str) -> Result<GitOutput> {
        let git = crate::services::git::GitService::open(&self.root)?;
        self.runtime.block_on(git.unstage(path))
    }

    fn git_diff(&self, staged: bool, path: Option<&str>) -> Result<GitOutput> {
        let git = crate::services::git::GitService::open(&self.root)?;
        if staged {
            self.runtime.block_on(git.diff_staged(path))
        } else {
            self.runtime.block_on(git.diff(path))
        }
    }

    fn terminal_run(&self, program: &str, args: &[String]) -> Result<ExecOutcome> {
        let terminal = crate::services::terminal::TerminalService::new(&self.root);
        self.runtime.block_on(terminal.run(program, args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_backend_dashboard_reports_empty_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new(tmp.path().to_path_buf());
        let snap = backend.dashboard().unwrap();
        assert_eq!(snap.root, tmp.path().canonicalize().unwrap());
        assert_eq!(snap.project_count, 0);
        assert!(!snap.is_git_repo);
    }

    #[test]
    fn local_backend_project_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new(tmp.path().to_path_buf());
        assert!(backend.create_project("alpha").is_ok());
        assert_eq!(backend.list_projects().unwrap(), vec!["alpha"]);
        backend.delete_project("alpha").unwrap();
        assert!(backend.list_projects().unwrap().is_empty());
    }

    #[test]
    fn local_backend_rejects_bad_project_names() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new(tmp.path().to_path_buf());
        assert!(backend.create_project("../escape").is_err());
        assert!(backend.create_project("").is_err());
    }

    #[test]
    fn local_backend_file_roundtrip_via_services() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new(tmp.path().to_path_buf());
        backend.write_file("note.txt", "hello").unwrap();
        assert_eq!(backend.read_file("note.txt").unwrap(), "hello");
        backend.delete_path("note.txt").unwrap();
        assert!(backend.read_file("note.txt").is_err());
    }

    #[test]
    fn local_backend_traversal_is_blocked() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new(tmp.path().to_path_buf());
        assert!(backend.read_file("../etc/passwd").is_err());
        assert!(backend.list_dir("../../").is_err());
    }
}
