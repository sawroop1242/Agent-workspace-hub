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
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create workspace: {}", self.root.display()))
    }

    pub fn project_path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub fn create_project(&self, name: &str) -> Result<PathBuf> {
        let path = self.project_path(name);
        fs::create_dir_all(path.join(".agent"))
            .with_context(|| format!("failed to create project: {name}"))?;
        Ok(path)
    }
}
