use serde::{Deserialize, Serialize};

/// A skill registry manifest: metadata plus the list of available skills.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryManifest {
    /// Registry name.
    pub name: String,
    /// Registry manifest version.
    pub version: String,
    /// Optional description of the registry.
    pub description: Option<String>,
    /// Skills advertised by the registry.
    pub skills: Vec<RegistrySkill>,
}

/// A single skill advertised by a registry, optionally with an integrity digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySkill {
    /// Skill name.
    pub name: String,
    /// What the skill does.
    pub description: String,
    /// Skill version.
    pub version: String,
    /// Relative path to the skill package within the registry.
    pub path: String,
    /// Optional SHA-256 digest of the skill package.
    pub sha256: Option<String>,
}
