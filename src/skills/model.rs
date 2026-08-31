use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A validated skill package: name, description, optional version, and its
/// on-disk location.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Skill {
    /// Skill name.
    pub name: String,
    /// What the skill does.
    pub description: String,
    /// Optional version string.
    pub version: Option<String>,
    /// On-disk location of the skill package.
    pub path: PathBuf,
}
