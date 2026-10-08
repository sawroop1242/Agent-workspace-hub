use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Root workspace manager. Projects remain ordinary directories so the
/// workspace is portable between the Python and Rust implementations.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// Creates a workspace rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the workspace root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Creates the workspace root directory if it does not exist.
    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create workspace: {}", self.root.display()))
    }

    /// Returns the path at which a project named `name` would live.
    pub fn project_path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// Creates a project directory (with `.agent`) under the workspace and returns its path.
    pub fn create_project(&self, name: &str) -> Result<PathBuf> {
        let path = self.project_path(name);
        fs::create_dir_all(path.join(".agent"))
            .with_context(|| format!("failed to create project: {name}"))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_root_report_the_workspace_root() {
        let ws = Workspace::new("/data/ws");
        assert_eq!(ws.root(), Path::new("/data/ws"));
    }

    #[test]
    fn ensure_creates_missing_root_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("workspace");
        assert!(!root.exists());
        Workspace::new(&root).ensure().unwrap();
        assert!(root.is_dir());
    }

    #[test]
    fn ensure_is_idempotent_for_existing_root() {
        let temp = tempfile::tempdir().unwrap();
        let ws = Workspace::new(temp.path());
        ws.ensure().unwrap();
        ws.ensure().unwrap();
        assert!(temp.path().is_dir());
    }

    #[test]
    fn project_path_joins_name_under_root() {
        let ws = Workspace::new("/data/ws");
        assert_eq!(ws.project_path("alpha"), PathBuf::from("/data/ws/alpha"));
    }

    #[test]
    fn create_project_creates_dir_with_agent_metadata_dir() {
        let temp = tempfile::tempdir().unwrap();
        let ws = Workspace::new(temp.path());
        let path = ws.create_project("alpha").unwrap();
        assert_eq!(path, temp.path().join("alpha"));
        assert!(path.is_dir());
        assert!(path.join(".agent").is_dir());
    }
}
