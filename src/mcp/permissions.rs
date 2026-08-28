use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The set of capabilities an MCP server may request. Requests are validated
/// against an approved [`McpPermissions`] before a server may execute.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPermissions {
    /// Whether outbound network access is permitted.
    #[serde(default)]
    pub network: bool,
    /// Filesystem paths the server may access.
    #[serde(default)]
    pub filesystem: Vec<String>,
    /// Environment variable names the server may receive.
    #[serde(default)]
    pub environment: Vec<String>,
    /// Whether the server may spawn subprocesses.
    #[serde(default)]
    pub process: bool,
    /// Secret names (referenced via `${secret:NAME}`) the server may resolve.
    #[serde(default)]
    pub secrets: Vec<String>,
}

/// A coarse capability category used for permission checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Outbound network access.
    Network,
    /// Filesystem path access.
    Filesystem,
    /// Environment variable access.
    Environment,
    /// Subprocess spawning.
    Process,
    /// Secret resolution.
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

/// Environment variable names must be a safe identifier: start with an ASCII
/// letter or underscore, followed by ASCII alphanumerics or underscores.
pub fn is_valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// Dangerous loader/interpreter variables (e.g. `LD_PRELOAD`, `PYTHONPATH`)
/// are blocked to prevent code injection via environment.
pub fn is_blocked_environment(name: &str) -> bool {
    BLOCKED_ENVIRONMENT
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(name))
}

impl McpPermissions {
    /// Validates the permission set: rejects empty filesystem paths, invalid or
    /// blocked environment/secret names, and secrets without a matching
    /// environment entry.
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

    /// Whether a coarse capability category is granted.
    pub fn allows(&self, permission: Permission) -> bool {
        match permission {
            Permission::Network => self.network,
            Permission::Filesystem => !self.filesystem.is_empty(),
            Permission::Environment => !self.environment.is_empty(),
            Permission::Process => self.process,
            Permission::Secrets => !self.secrets.is_empty(),
        }
    }

    /// Filters a requested environment list down to the approved names.
    pub fn allowed_environment(&self, requested: impl IntoIterator<Item = String>) -> Vec<String> {
        let allowed: HashSet<&str> = self.environment.iter().map(String::as_str).collect();
        requested
            .into_iter()
            .filter(|v| allowed.contains(v.as_str()))
            .collect()
    }

    /// Whether a particular secret name is approved for resolution.
    pub fn allows_secret(&self, name: &str) -> bool {
        self.secrets.iter().any(|allowed| allowed == name)
    }
}

/// Requires a capability, failing closed if it is not granted.
pub fn require(permissions: &McpPermissions, permission: Permission) -> Result<()> {
    if !permissions.allows(permission) {
        bail!("MCP permission denied: {:?}", permission);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_reports_granted_capabilities() {
        let perms = McpPermissions {
            network: true,
            filesystem: vec!["/tmp".into()],
            environment: vec!["FOO".into()],
            process: false,
            secrets: vec!["TOKEN".into()],
        };
        assert!(perms.allows(Permission::Network));
        assert!(perms.allows(Permission::Filesystem));
        assert!(perms.allows(Permission::Environment));
        assert!(perms.allows(Permission::Secrets));
        assert!(!perms.allows(Permission::Process));

        let empty = McpPermissions::default();
        assert!(!empty.allows(Permission::Network));
        assert!(!empty.allows(Permission::Filesystem));
        assert!(!empty.allows(Permission::Environment));
        assert!(!empty.allows(Permission::Secrets));
    }

    #[test]
    fn allowed_environment_filters_to_approved_names() {
        let perms = McpPermissions {
            environment: vec!["FOO".into(), "BAR".into()],
            ..McpPermissions::default()
        };
        let filtered = perms.allowed_environment(vec!["FOO".into(), "BAR".into(), "BAZ".into()]);
        assert_eq!(filtered, vec!["FOO".to_string(), "BAR".to_string()]);
    }

    #[test]
    fn allows_secret_matches_exact_name() {
        let perms = McpPermissions {
            secrets: vec!["API_TOKEN".into()],
            ..McpPermissions::default()
        };
        assert!(perms.allows_secret("API_TOKEN"));
        assert!(!perms.allows_secret("OTHER"));
    }

    #[test]
    fn require_fails_closed_when_not_granted() {
        let perms = McpPermissions {
            network: true,
            ..McpPermissions::default()
        };
        assert!(require(&perms, Permission::Network).is_ok());
        assert!(require(&perms, Permission::Process).is_err());
    }

    #[test]
    fn is_valid_env_name_accepts_safe_identifiers() {
        assert!(is_valid_env_name("FOO"));
        assert!(is_valid_env_name("_leading"));
        assert!(is_valid_env_name("A1_b2"));
        assert!(!is_valid_env_name("1abc"));
        assert!(!is_valid_env_name("with-dash"));
        assert!(!is_valid_env_name(""));
    }

    #[test]
    fn is_blocked_environment_is_case_insensitive() {
        assert!(is_blocked_environment("PATH"));
        assert!(is_blocked_environment("path"));
        assert!(is_blocked_environment("LD_PRELOAD"));
        assert!(is_blocked_environment("PythonPath"));
        assert!(!is_blocked_environment("HOME"));
        assert!(!is_blocked_environment("API_KEY"));
    }
}
