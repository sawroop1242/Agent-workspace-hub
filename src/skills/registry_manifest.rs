use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub skills: Vec<RegistrySkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySkill {
    pub name: String,
    pub description: String,
    pub version: String,
    pub path: String,
    pub sha256: Option<String>,
}
