use anyhow::{bail, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub struct FileStore {
    root: PathBuf,
}

impl FileStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, relative: impl AsRef<Path>) -> Result<PathBuf> {
        let relative = relative.as_ref();
        if relative.is_absolute() {
            bail!("absolute paths are not allowed")
        }
        for component in relative.components() {
            if matches!(component, Component::ParentDir | Component::Root | Component::Prefix(_)) {
                bail!("path traversal is not allowed")
            }
        }
        Ok(self.root.join(relative))
    }

    pub fn read(&self, relative: impl AsRef<Path>) -> Result<String> {
        Ok(fs::read_to_string(self.resolve(relative)?)?)
    }

    pub fn write(&self, relative: impl AsRef<Path>, content: &str) -> Result<()> {
        let path = self.resolve(relative)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }

    pub fn exists(&self, relative: impl AsRef<Path>) -> Result<bool> {
        Ok(self.resolve(relative)?.exists())
    }
}
