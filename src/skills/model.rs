use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A validated skill package: name, description, optional version, and its
/// on-disk location.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub path: PathBuf,
}
