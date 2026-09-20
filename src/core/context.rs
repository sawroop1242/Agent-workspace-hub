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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_project_points_at_agent_context_md() {
        let temp = tempfile::tempdir().unwrap();
        let store = ContextStore::for_project(temp.path());
        assert_eq!(store.path(), temp.path().join(".agent/context.md"));
    }

    #[test]
    fn read_on_missing_file_is_empty_string_not_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let store = ContextStore::for_project(temp.path());
        assert_eq!(store.read().unwrap(), "");
    }

    #[test]
    fn write_creates_parent_dirs_and_read_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let store = ContextStore::for_project(temp.path());
        store.write("# Context\n\nSome notes.").unwrap();
        assert!(store.path().is_file());
        assert_eq!(store.read().unwrap(), "# Context\n\nSome notes.");
    }

    #[test]
    fn write_overwrites_previous_content() {
        let temp = tempfile::tempdir().unwrap();
        let store = ContextStore::for_project(temp.path());
        store.write("first version").unwrap();
        store.write("second version").unwrap();
        assert_eq!(store.read().unwrap(), "second version");
    }

    #[test]
    fn write_empty_string_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let store = ContextStore::for_project(temp.path());
        store.write("").unwrap();
        assert_eq!(store.read().unwrap(), "");
    }
}
