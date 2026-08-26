use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPermissions {
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub filesystem: Vec<String>,
    #[serde(default)]
    pub environment: Vec<String>,
    #[serde(default)]
    pub process: bool,
    #[serde(default)]
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Network,
    Filesystem,
    Environment,
    Process,
    Secrets,
}

const BLOCKED_ENVIRONMENT: &[&str] = &[
    "PATH",
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FRAMEWORK_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "PYTHONPATH",
    "PYTHONHOME",
    "RUBYLIB",
    "PERL5LIB",
];

pub fn is_valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

pub fn is_blocked_environment(name: &str) -> bool {
    BLOCKED_ENVIRONMENT
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(name))
}

impl McpPermissions {
    pub fn validate(&self) -> Result<()> {
        if self.filesystem.iter().any(|p| p.trim().is_empty()) {
            bail!("filesystem permission path cannot be empty");
        }
        for name in &self.environment {
            if !is_valid_env_name(name) {
                bail!("invalid environment variable name: {name:?}");
            }
            if is_blocked_environment(name) {
                bail!("dangerous environment variable is blocked: {name}");
            }
        }
        for name in &self.secrets {
            if !is_valid_env_name(name) {
                bail!("invalid secret environment variable name: {name:?}");
            }
            if is_blocked_environment(name) {
                bail!("dangerous secret environment variable is blocked: {name}");
            }
        }
        if self
            .secrets
            .iter()
            .any(|name| !self.environment.iter().any(|env| env == name))
        {
            bail!("secret permission must also be present in environment permissions");
        }
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
        requested
            .into_iter()
            .filter(|v| allowed.contains(v.as_str()))
            .collect()
    }

    pub fn allows_secret(&self, name: &str) -> bool {
        self.secrets.iter().any(|allowed| allowed == name)
    }
}

pub fn require(permissions: &McpPermissions, permission: Permission) -> Result<()> {
    if !permissions.allows(permission) {
        bail!("MCP permission denied: {:?}", permission);
    }
    Ok(())
}
