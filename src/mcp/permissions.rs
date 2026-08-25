use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPermissions {
    #[serde(default)] pub network: bool,
    #[serde(default)] pub filesystem: Vec<String>,
    #[serde(default)] pub environment: Vec<String>,
    #[serde(default)] pub process: bool,
    #[serde(default)] pub secrets: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission { Network, Filesystem, Environment, Process, Secrets }

impl McpPermissions {
    pub fn validate(&self) -> Result<()> {
        if self.filesystem.iter().any(|p| p.trim().is_empty()) { bail!("filesystem permission path cannot be empty"); }
        if self.environment.iter().any(|v| v.trim().is_empty()) { bail!("environment permission cannot be empty"); }
        if self.secrets.iter().any(|v| v.trim().is_empty()) { bail!("secret permission cannot be empty"); }
        Ok(())
    }

    pub fn allows(&self, permission: Permission) -> bool {
        match permission {
            Permission::Network => self.network,
            Permission::Filesystem => !self.filesystem.is_empty(),
            Permission::Environment => !self.environment.is_empty(),
            Permission::Process => self.process,
            Permission::Secrets => !self.secrets.is_empty(),
        }
    }

    pub fn allowed_environment(&self, requested: impl IntoIterator<Item = String>) -> Vec<String> {
        let allowed: HashSet<&str> = self.environment.iter().map(String::as_str).collect();
        requested.into_iter().filter(|v| allowed.contains(v.as_str())).collect()
    }
}

pub fn require(permissions: &McpPermissions, permission: Permission) -> Result<()> {
    if !permissions.allows(permission) { bail!("MCP permission denied: {:?}", permission); }
    Ok(())
}
