use anyhow::{Context as AnyhowContext, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Persistent project context stored in `.agent/context.md`.
#[derive(Debug, Clone)]
pub struct ContextStore {
    path: PathBuf,
}

impl ContextStore {
    /// Creates a context store rooted at the given project's `.agent/context.md`.
    pub fn for_project(project_path: &Path) -> Self {
        Self {
            path: project_path.join(".agent/context.md"),
        }
    }

    /// Returns the backing context file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the current context, returning an empty string if it does not exist.
    pub fn read(&self) -> Result<String> {
        if !self.path.exists() {
            return Ok(String::new());
        }
        fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))
    }

    /// Overwrites the context file with `content`, creating parent directories as needed.
    pub fn write(&self, content: &str) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, content)
            .with_context(|| format!("failed to write {}", self.path.display()))
    }
}
