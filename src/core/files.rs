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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_joins_relative_path_under_root() {
        let store = FileStore::new("/data/root");
        assert_eq!(
            store.resolve("a/b.txt").unwrap(),
            PathBuf::from("/data/root/a/b.txt")
        );
    }

    #[test]
    fn resolve_allows_filename_that_is_just_dots() {
        let store = FileStore::new("/data/root");
        assert!(store.resolve("..hidden.txt").is_ok());
    }

    #[test]
    fn resolve_rejects_parent_dir_traversal() {
        let store = FileStore::new("/data/root");
        assert!(store.resolve("../escape.txt").is_err());
        assert!(store.resolve("a/../../escape.txt").is_err());
    }

    #[test]
    fn resolve_rejects_absolute_paths() {
        let store = FileStore::new("/data/root");
        assert!(store.resolve("/etc/passwd").is_err());
    }

    #[test]
    fn write_then_read_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let store = FileStore::new(temp.path());
        store.write("nested/dir/file.txt", "hello").unwrap();
        assert_eq!(store.read("nested/dir/file.txt").unwrap(), "hello");
        assert!(store.exists("nested/dir/file.txt").unwrap());
        assert!(!store.exists("nested/dir/missing.txt").unwrap());
    }

    #[test]
    fn root_returns_store_root() {
        let store = FileStore::new("/data/root");
        assert_eq!(store.root(), Path::new("/data/root"));
    }
}
