use super::permissions::McpPermissions;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// The trust level assigned to an MCP server. `Unknown` (untrusted) and
/// `Blocked` deny execution; `Review` and `Trusted` permit it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// Explicitly trusted.
    Trusted,
    /// Reviewed and approved.
    Reviewed,
    /// Untrusted (fail closed).
    Unknown,
    /// Explicitly blocked.
    Blocked,
}

/// A persisted approval for a specific MCP server id, version, and permission set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpApproval {
    /// MCP server id being approved.
    pub id: String,
    /// Assigned trust level.
    #[serde(default)]
    pub level: TrustLevel,
    /// The permission set explicitly approved for the server.
    #[serde(default)]
    pub approved_permissions: McpPermissions,
    /// The server version this approval applies to (empty means any).
    #[serde(default)]
    pub approved_version: String,
}
/// `Unknown` is deliberately not the first variant, so `Default` cannot be
/// derived: a new approval must start untrusted (fail closed) rather than
/// implicitly trusted.
#[allow(clippy::derivable_impls)]
impl Default for TrustLevel {
    fn default() -> Self {
        Self::Unknown
    }
}
/// In-memory collection of MCP approvals.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TrustStore {
    /// The stored approvals.
    pub approvals: Vec<McpApproval>,
}
impl TrustStore {
    /// Returns the approval for `id`, if any.
    pub fn get(&self, id: &str) -> Option<&McpApproval> {
        self.approvals.iter().find(|x| x.id == id)
    }
    /// Records (or replaces) an approval after validating the permission set.
    pub fn approve(
        &mut self,
        id: impl Into<String>,
        level: TrustLevel,
        permissions: McpPermissions,
        version: impl Into<String>,
    ) -> Result<()> {
        let id = id.into();
        if id.trim().is_empty() {
            bail!("MCP id is required")
        }
        permissions.validate()?;
        self.approvals.retain(|x| x.id != id);
        self.approvals.push(McpApproval {
            id,
            level,
            approved_permissions: permissions,
            approved_version: version.into(),
        });
        Ok(())
    }
    /// Removes an approval, returning whether one existed for `id`.
    pub fn revoke(&mut self, id: &str) -> bool {
        let before = self.approvals.len();
        self.approvals.retain(|x| x.id != id);
        before != self.approvals.len()
    }
}
/// Whether an approval authorizes the requested permissions and version,
/// denying blocked/unknown levels and over-broad requests.
pub fn can_enable(
    approval: Option<&McpApproval>,
    requested: &McpPermissions,
    version: &str,
) -> bool {
    let Some(a) = approval else { return false };
    if matches!(a.level, TrustLevel::Blocked | TrustLevel::Unknown) {
        return false;
    }
    if !a.approved_version.is_empty() && a.approved_version != version {
        return false;
    }
    requested.network <= a.approved_permissions.network
        && requested.process <= a.approved_permissions.process
        && requested
            .filesystem
            .iter()
            .all(|p| a.approved_permissions.filesystem.contains(p))
        && requested
            .environment
            .iter()
            .all(|v| a.approved_permissions.environment.contains(v))
        && requested
            .secrets
            .iter()
            .all(|v| a.approved_permissions.secrets.contains(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perms(network: bool, process: bool, fs: &[&str]) -> McpPermissions {
        McpPermissions {
            network,
            process,
            filesystem: fs.iter().map(|s| s.to_string()).collect(),
            environment: Vec::new(),
            secrets: Vec::new(),
        }
    }

    fn approval(level: TrustLevel, permissions: McpPermissions, version: &str) -> McpApproval {
        McpApproval {
            id: "server-a".into(),
            level,
            approved_permissions: permissions,
            approved_version: version.into(),
        }
    }

    #[test]
    fn trust_level_default_is_unknown_fail_closed() {
        assert_eq!(TrustLevel::default(), TrustLevel::Unknown);
    }

    #[test]
    fn trust_level_serde_round_trips_lowercase() {
        for level in [
            TrustLevel::Trusted,
            TrustLevel::Reviewed,
            TrustLevel::Unknown,
            TrustLevel::Blocked,
        ] {
            let back: TrustLevel =
                serde_json::from_str(&serde_json::to_string(&level).unwrap()).unwrap();
            assert_eq!(back, level);
        }
        assert_eq!(
            serde_json::to_string(&TrustLevel::Reviewed).unwrap(),
            "\"reviewed\""
        );
        assert_eq!(
            serde_json::to_string(&TrustLevel::Blocked).unwrap(),
            "\"blocked\""
        );
    }

    #[test]
    fn approve_rejects_empty_and_whitespace_only_id() {
        let mut store = TrustStore::default();
        assert!(store
            .approve("", TrustLevel::Trusted, perms(false, false, &[]), "1.0")
            .is_err());
        assert!(store
            .approve("   ", TrustLevel::Trusted, perms(false, false, &[]), "1.0")
            .is_err());
        assert!(store.approvals.is_empty());
    }

    #[test]
    fn approve_replaces_existing_record_for_same_id() {
        let mut store = TrustStore::default();
        store
            .approve(
                "server-a",
                TrustLevel::Trusted,
                perms(false, false, &[]),
                "1.0",
            )
            .unwrap();
        store
            .approve(
                "server-a",
                TrustLevel::Blocked,
                perms(true, true, &["/"]),
                "2.0",
            )
            .unwrap();
        assert_eq!(store.approvals.len(), 1);
        let record = store.get("server-a").unwrap();
        assert_eq!(record.level, TrustLevel::Blocked);
        assert_eq!(record.approved_version, "2.0");
    }

    #[test]
    fn get_returns_none_for_unknown_id() {
        let store = TrustStore::default();
        assert!(store.get("missing").is_none());
    }

    #[test]
    fn revoke_reports_whether_a_record_existed() {
        let mut store = TrustStore::default();
        assert!(!store.revoke("server-a"));
        store
            .approve(
                "server-a",
                TrustLevel::Trusted,
                perms(false, false, &[]),
                "1.0",
            )
            .unwrap();
        assert!(store.revoke("server-a"));
        assert!(store.get("server-a").is_none());
        assert!(!store.revoke("server-a"));
    }

    #[test]
    fn can_enable_denies_without_approval() {
        assert!(!can_enable(None, &perms(false, false, &[]), "1.0"));
    }

    #[test]
    fn can_enable_denies_blocked_and_unknown_levels() {
        for level in [TrustLevel::Blocked, TrustLevel::Unknown] {
            let a = approval(level, perms(true, true, &["/"]), "");
            assert!(!can_enable(Some(&a), &perms(false, false, &[]), "1.0"));
        }
    }

    #[test]
    fn can_enable_empty_version_means_any_version() {
        let a = approval(TrustLevel::Trusted, perms(false, false, &[]), "");
        assert!(can_enable(Some(&a), &perms(false, false, &[]), "9.9"));
    }

    #[test]
    fn can_enable_version_mismatch_denies() {
        let a = approval(TrustLevel::Trusted, perms(false, false, &[]), "1.0");
        assert!(!can_enable(Some(&a), &perms(false, false, &[]), "2.0"));
        assert!(can_enable(Some(&a), &perms(false, false, &[]), "1.0"));
    }

    #[test]
    fn can_enable_subset_permissions_allowed_superset_denied() {
        let approved = perms(true, true, &["/data", "/tmp"]);
        let a = approval(TrustLevel::Trusted, approved, "");
        // requesting exactly what was approved is fine
        assert!(can_enable(
            Some(&a),
            &perms(true, true, &["/data", "/tmp"]),
            "1.0"
        ));
        // requesting strictly less is fine
        assert!(can_enable(Some(&a), &perms(false, false, &["/tmp"]), "1.0"));
        // requesting a filesystem path not covered is denied
        assert!(!can_enable(
            Some(&a),
            &perms(false, false, &["/etc"]),
            "1.0"
        ));
    }

    #[test]
    fn can_enable_denies_unapproved_process() {
        let approved = perms(false, false, &[]);
        let a = approval(TrustLevel::Reviewed, approved, "");
        assert!(!can_enable(Some(&a), &perms(false, true, &[]), "1.0"));
    }

    #[test]
    fn can_enable_environment_and_secret_subsets() {
        let approved = McpPermissions {
            network: false,
            process: false,
            filesystem: Vec::new(),
            environment: vec!["API_KEY".into(), "REGION".into()],
            secrets: vec!["API_KEY".into()],
        };
        let a = approval(TrustLevel::Reviewed, approved, "");
        // approved env subset is allowed
        assert!(can_enable(
            Some(&a),
            &McpPermissions {
                environment: vec!["API_KEY".into()],
                ..Default::default()
            },
            "1.0"
        ));
        // unapproved env name is denied
        assert!(!can_enable(
            Some(&a),
            &McpPermissions {
                environment: vec!["OTHER".into()],
                ..Default::default()
            },
            "1.0"
        ));
        // secret outside the approved secret set is denied
        assert!(!can_enable(
            Some(&a),
            &McpPermissions {
                secrets: vec!["REGION".into()],
                environment: vec!["REGION".into()],
                ..Default::default()
            },
            "1.0"
        ));
        // approved secret is allowed
        assert!(can_enable(
            Some(&a),
            &McpPermissions {
                secrets: vec!["API_KEY".into()],
                environment: vec!["API_KEY".into()],
                ..Default::default()
            },
            "1.0"
        ));
    }
}
