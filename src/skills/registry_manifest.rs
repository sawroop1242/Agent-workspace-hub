use serde::{Deserialize, Serialize};

/// A skill registry manifest: metadata plus the list of available skills.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub skills: Vec<RegistrySkill>,
}

/// A single skill advertised by a registry, optionally with an integrity digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySkill {
    pub name: String,
    pub description: String,
    pub version: String,
    pub path: String,
    pub sha256: Option<String>,
}
