use anyhow::{bail, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Filesystem store scoped to a root directory, rejecting path traversal.
pub struct FileStore {
    root: PathBuf,
}

impl FileStore {
    /// Creates a `FileStore` rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the store's root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolves a relative path under the root, rejecting absolute paths and
    /// any `..` or root components to prevent escaping the root.
    pub fn resolve(&self, relative: impl AsRef<Path>) -> Result<PathBuf> {
        let relative = relative.as_ref();
        if relative.is_absolute() {
            bail!("absolute paths are not allowed")
        }
        for component in relative.components() {
            if matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            ) {
                bail!("path traversal is not allowed")
            }
        }
        Ok(self.root.join(relative))
    }

    /// Reads a file under the root as UTF-8 text.
    pub fn read(&self, relative: impl AsRef<Path>) -> Result<String> {
        Ok(fs::read_to_string(self.resolve(relative)?)?)
    }

    /// Writes `content` to a file under the root, creating parent directories.
    pub fn write(&self, relative: impl AsRef<Path>, content: &str) -> Result<()> {
        let path = self.resolve(relative)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }

    /// Returns whether a file under the root exists.
    pub fn exists(&self, relative: impl AsRef<Path>) -> Result<bool> {
        Ok(self.resolve(relative)?.exists())
    }
}
