use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A workspace project, identified by name and tracked as a directory on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    /// Human-facing project name.
    pub name: String,
    /// Filesystem path of the project directory.
    pub path: PathBuf,
}

impl Project {
    /// Creates a project from a name and directory path.
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
        }
    }
}
