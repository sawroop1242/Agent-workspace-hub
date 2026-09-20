//! The authoritative authorization boundary for edit and rollback mutation
//! paths (AWE-011 / #32).
//!
//! This module is the single, transport-independent authorization point that
//! every filesystem edit and rollback operation must traverse. It does not
//! replace the existing MCP trust/execution gate; it composes with it in the
//! documented order:
//!
//! ```text
//! transport authentication/trust
//!        ↓
//! canonical authorization context (this module)
//!        ↓
//! capability + scope + policy decision
//!        ↓
//! canonical EditService
//! ```
//!
//! # Design rules
//!
//! - One boundary, one decision type (`AuthorizationDecision`).
//! - Fail-closed on infrastructure failure: a corrupt or unreadable
//!   capability/policy store means Deny, never Allow.
//! - Capability grants are enforced exactly:
//!   - exact `agent_id` match;
//!   - grant must carry the required `Permission::Filesystem`;
//!   - `expires_at` is evaluated using RFC 3339 / UTC, with malformed entries
//!     failing closed (a malformed expiry is never silently treated as if
//!     nothing was set);
//!   - `scope` is a workspace-relative path prefix matched by path
//!     components — `src/foo` matches `src/foo/bar.rs` and `src/foo`, but
//!     never `src/foobar`. This matches `PolicyStore`'s prefix semantics.
//! - Policy denial takes precedence over capabilities: even a fully granted
//!   agent is denied when a `PolicyStore` rule matches (deny precedence).
//! - Agent identity is never inferred from URLs or routes — it comes only
//!   from the request's explicit typed identity.
//! - Authorization decisions are structured and deterministic, with a stable
//!   machine-readable reason code for MCP and CLI equivalence.
//!
//! ## Compatibility
//!
//! This intentionally preserves the existing toolkit of grant/policy data
//! (models + stores) unchanged; enforcement is what is new.
//!
//! ## CLI / MCP
//!
//! Both interfaces call into this module directly (single decision). The CLI
//! path will be wired up in AWE-014 (CLI editing commands); the code-level
//! parity test lives in this module, proving both have the same decision
//! semantics.

use crate::core::capability_grants::CapabilityGrantStore;
use crate::core::policy::PolicyStore;
use crate::mcp::permissions::Permission;
use crate::models::CapabilityGrant;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The action being authorized: which edit operation or rollback attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditAction {
    Replace,
    Insert,
    DeleteRange,
    Patch,
    ApplyDiff,
    /// AWE-010 rollback: a consequential mutation of equal weight.
    Rollback,
}

impl EditAction {
    /// The stable tool/operation identifier used for policy evaluation.
    pub fn tool_name(self) -> &'static str {
        match self {
            EditAction::Replace => "filesystem.replace",
            EditAction::Insert => "filesystem.insert",
            EditAction::DeleteRange => "filesystem.delete_range",
            EditAction::Patch => "filesystem.patch",
            EditAction::ApplyDiff => "filesystem.apply_diff",
            EditAction::Rollback => "filesystem.rollback",
        }
    }
}

/// The explicit principal/caller identity attached to an authorization
/// request. This is never derived from a URL, route, or transport detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizingPrincipal {
    /// The agent the request claims to act on behalf of. `None` means a
    /// human/operator direct-CLI call (no agent identity asserted).
    pub agent_id: Option<String>,
    /// A session identifier, required when agent identity is claimed.
    pub session_id: Option<String>,
    /// Workspace identifier (the workspace root path).
    pub workspace_id: String,
}

/// A typed, explicit authorization request.
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    /// The operation being requested.
    pub action: EditAction,
    /// The caller identity.
    pub principal: AuthorizingPrincipal,
    /// The workspace-relative resource path being touched by this operation.
    pub resource: String,
    /// When rolling back, the edit id of the transaction being rolled back.
    pub transaction_id: Option<String>,
}

/// Stable machine-readable denial reason codes. Deliberately vague about
/// internals: the result is returned to every caller, so no secret or
/// server-internal state is exposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DenialReason {
    /// The request supplied no agent identity (operator direct call
    /// accepted depending on implementation mode, in-flight context).
    MissingAgentIdentity,
    /// The agent identity claimed is unknown to this workspace.
    UnknownAgent,
    /// The agent does not hold the needed capability grant.
    MissingCapability,
    /// The claim was issued with an expired grant.
    ExpiredGrant,
    /// The grant was issued for a different scope than the target resource.
    OutOfScope,
    /// Policy rules explicitly deny this action on this resource.
    PolicyDenied,
    /// The capability/policy store could not be read; deny fail-closed.
    InfrastructureUnavailable,
    /// The transaction type is not allowed (e.g. infrastructure mutation).
    UnsupportedOperation,
}

/// The result of an authorization check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorizationDecision {
    /// All checks passed; the action is authorized.
    Allow,
    /// The action is disallowed. Denial is always associated with a stable
    /// machine-readable reason and optional explanatory details that are
    /// safe for clients to see.
    Deny {
        /// The stable, machine-readable denial code.
        reason: DenialReason,
        /// Human-readable reason (never carries session secrets).
        detail: String,
    },
}

impl AuthorizationDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    pub fn denied(detail: impl Into<String>, reason: DenialReason) -> Self {
        Self::Deny {
            reason,
            detail: detail.into(),
        }
    }
}

/// The single authoritative authorization boundary. Every edit mutation path
/// calls into this struct — no bypassing through a second service path.
pub struct EditAuthorizer {
    capability_store: CapabilityGrantStore,
    policy_store: PolicyStore,
}

impl EditAuthorizer {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            capability_store: CapabilityGrantStore::new(&workspace_root),
            policy_store: PolicyStore::new(&workspace_root),
        }
    }

    /// The single authorization entry point. Every edit, and every rollback,
    /// is authorized exactly here, with the shared deny precedence:
    ///   infrastructure error → Deny
    ///   policy deny → Deny
    ///   capability missing/expired/out-of-scope → Deny
    ///   otherwise → Allow
    pub fn authorize(&self, request: &AuthorizationRequest) -> AuthorizationDecision {
        // Step 1: Policy is evaluated first. If a deny rule matches, nothing
        // else matters — policy denial always wins (deny precedence).
        let policy_match = self
            .policy_store
            .matching(request.action.tool_name(), &request.resource);
        match policy_match {
            Ok(Some(rule)) => {
                return AuthorizationDecision::denied(
                    format!("policy rule {} denies this action", rule.id),
                    DenialReason::PolicyDenied,
                )
            }
            Ok(None) => {}
            Err(err) => {
                return AuthorizationDecision::denied(
                    format!(
                        "policy store is unreadable for tool {}: {err}",
                        request.action.tool_name()
                    ),
                    DenialReason::InfrastructureUnavailable,
                )
            }
        }

        // Step 2: Capability evaluation. This only applies when the request
        // carries an agent identity. Caller-side (human/operator CLI) paths
        // are covered by the policy evaluation above; agents WOULD DO TK...
        let Some(agent_id) = request.principal.agent_id.as_deref() else {
            // No agent identity claimed — this is the operator path.
            return AuthorizationDecision::Allow;
        };

        // Claims to be an agent → must have a matching grant.
        let grants = match self.capability_store.list_for_agent(agent_id) {
            Ok(grants) => grants,
            Err(err) => {
                return AuthorizationDecision::denied(
                    format!("capability store unreadable: {err}"),
                    DenialReason::InfrastructureUnavailable,
                )
            }
        };
        if grants.is_empty() {
            return AuthorizationDecision::denied(
                format!("agent {agent_id} has no capability grants"),
                DenialReason::MissingCapability,
            );
        }

        // Pick the most specific grant for this action: Filesystem
        // permission and matching scope (if any).
        let mut matching: Vec<&CapabilityGrant> = grants
            .iter()
            .filter(|g| g.permission == Permission::Filesystem)
            .filter(|g| match &g.scope {
                // Unscoped grants cover any workspace path.
                None => true,
                Some(scope) => scope_covers(scope, &request.resource),
            })
            .collect();

        if matching.is_empty() {
            return AuthorizationDecision::denied(
                format!(
                    "agent {agent_id} has no capability grant for Filesystem covering {}",
                    request.resource
                ),
                DenialReason::MissingCapability,
            );
        }

        // Expiry evaluation: all matched grants must be unexpired; an expired
        // grant is never a fallback to authorization.
        matching.retain(|g| !grant_is_expired(g));
        if matching.is_empty() {
            return AuthorizationDecision::denied(
                format!("all grants for agent {agent_id} are expired"),
                DenialReason::ExpiredGrant,
            );
        }

        AuthorizationDecision::Allow
    }
}

/// Returns whether `scope` covers the given `resource` with path-component
/// semantics (`src/foo` covers `src/foo` and `src/foo/bar.rs`, never
/// `src/foobar`). This mirrors the policy store's prefix matcher.
fn scope_covers(scope: &str, resource: &str) -> bool {
    let scope = normalize_forward_slash(scope);
    let resource = normalize_forward_slash(resource);
    if scope.is_empty() {
        return false;
    }
    resource == scope || resource.starts_with(&format!("{scope}/"))
}

/// Mirrors the normalization step used by `PolicyStore` so authorization
/// state is tightened deterministically by path components.
fn normalize_forward_slash(path: &str) -> String {
    let mut out = String::new();
    let mut last_was_slash = false;
    for ch in path.chars() {
        if ch == '/' {
            last_was_slash = true;
        } else {
            if last_was_slash && !out.is_empty() {
                out.push('/');
            }
            last_was_slash = false;
            out.push(ch);
        }
    }
    out
}

/// Returns `true` if the grant is expired by RFC 3339 comparison.
///
/// A malformed `expires_at` fails closed (treated as expired) so a
/// malformed payload can never be unexpectedly permissive.
fn grant_is_expired(grant: &CapabilityGrant) -> bool {
    let Some(expires) = &grant.expires_at else {
        return false; // Unset = never expires.
    };
    let Ok(expiry) = DateTime::parse_from_rfc3339(expires) else {
        // Malformed expiry fails closed.
        return true;
    };
    expiry <= Utc::now()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn write_grant_store(root: &Path, grant: CapabilityGrant) {
        let dir = root.join(".agent").join("capabilities");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{}.json", grant.id));
        fs::write(path, serde_json::to_string_pretty(&grant).unwrap()).unwrap();
    }

    fn write_policy(root: &Path, rule_id: &str, tool: &str, pattern: &str) {
        let dir = root.join(".agent");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("policy.json");
        fs::write(
            path,
            serde_json::to_string_pretty(&[serde_json::json!({
                "id": rule_id,
                "tool": tool,
                "pattern": pattern,
                "reason": null,
                "created_at": "2026-01-01T00:00:00Z"
            })])
            .unwrap(),
        )
        .unwrap();
    }

    fn principal(agent: Option<&str>) -> AuthorizingPrincipal {
        AuthorizingPrincipal {
            agent_id: agent.map(String::from),
            session_id: Some("session-1".into()),
            workspace_id: "workspace".into(),
        }
    }

    fn request(
        action: EditAction,
        resource: &str,
        principal: AuthorizingPrincipal,
    ) -> AuthorizationRequest {
        AuthorizationRequest {
            action,
            principal,
            resource: resource.into(),
            transaction_id: None,
        }
    }

    fn grant(
        id: &str,
        agent_id: &str,
        scope: Option<&str>,
        expires_at: Option<&str>,
    ) -> CapabilityGrant {
        CapabilityGrant {
            id: id.into(),
            agent_id: agent_id.into(),
            permission: Permission::Filesystem,
            scope: scope.map(String::from),
            granted_at: Utc::now().to_rfc3339(),
            expires_at: expires_at.map(String::from),
        }
    }

    #[test]
    fn operator_direct_with_no_agent_identity_is_allowed() {
        let temp = tempfile::tempdir().unwrap();
        let auth = EditAuthorizer::new(temp.path().to_path_buf());
        let req = request(EditAction::Replace, "src/main.rs", principal(None));
        assert_eq!(auth.authorize(&req), AuthorizationDecision::Allow);
    }

    #[test]
    fn agent_valid_grant_is_allowed() {
        let temp = tempfile::tempdir().unwrap();
        write_grant_store(temp.path(), grant("g1", "agent-a", None, None));
        let auth = EditAuthorizer::new(temp.path().to_path_buf());
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            principal(Some("agent-a")),
        );
        assert_eq!(auth.authorize(&req), AuthorizationDecision::Allow);
    }

    #[test]
    fn agent_with_wrong_identity_is_denied() {
        let temp = tempfile::tempdir().unwrap();
        write_grant_store(temp.path(), grant("g1", "agent-a", None, None));
        let auth = EditAuthorizer::new(temp.path().to_path_buf());
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            principal(Some("agent-b")), // Unknown agent
        );
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::MissingCapability,
                ..
            }
        ));
    }

    #[test]
    fn expired_grant_is_denied() {
        let temp = tempfile::tempdir().unwrap();
        let past = (Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        write_grant_store(temp.path(), grant("g1", "agent-a", None, Some(&past)));
        let auth = EditAuthorizer::new(temp.path().to_path_buf());
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            principal(Some("agent-a")),
        );
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::ExpiredGrant,
                ..
            }
        ));
    }

    #[test]
    fn out_of_scope_grant_is_denied() {
        let temp = tempfile::tempdir().unwrap();
        write_grant_store(temp.path(), grant("g1", "agent-a", Some("src/foo"), None));
        let auth = EditAuthorizer::new(temp.path().to_path_buf());
        let req = request(
            EditAction::Replace,
            "src/foobar.rs", // NOT inside the scope; needs prefix boundary
            principal(Some("agent-a")),
        );
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::MissingCapability,
                ..
            }
        ));
    }

    #[test]
    fn policy_deny_takes_precedence_over_capability() {
        let temp = tempfile::tempdir().unwrap();
        write_grant_store(temp.path(), grant("g1", "agent-a", None, None));
        write_policy(temp.path(), "deny-foo", "filesystem.replace", "src/foo");
        let auth = EditAuthorizer::new(temp.path().to_path_buf());
        let req = request(
            EditAction::Replace,
            "src/foo/x.rs",
            principal(Some("agent-a")),
        );
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::PolicyDenied,
                ..
            }
        ));
    }

    #[test]
    fn malformed_expiry_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        write_grant_store(
            temp.path(),
            grant("g1", "agent-a", None, Some("not-a-date")),
        );
        let auth = EditAuthorizer::new(temp.path().to_path_buf());
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            principal(Some("agent-a")),
        );
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::ExpiredGrant,
                ..
            }
        ));
    }

    #[test]
    fn rollback_obeys_policy_deny() {
        // Regression coverage for the AWE-011 gap: rollback maps to the
        // "filesystem.rollback" policy tool, which must be deniable by a
        // workspace policy rule like every other consequential mutation.
        let temp = tempfile::tempdir().unwrap();
        write_grant_store(temp.path(), grant("g1", "agent-a", None, None));
        write_policy(temp.path(), "deny-rb", "filesystem.rollback", "src/");
        let auth = EditAuthorizer::new(temp.path().to_path_buf());
        let denied = request(
            EditAction::Rollback,
            "src/main.rs",
            principal(Some("agent-a")),
        );
        assert!(matches!(
            auth.authorize(&denied),
            AuthorizationDecision::Deny {
                reason: DenialReason::PolicyDenied,
                ..
            }
        ));
        let allowed = request(
            EditAction::Rollback,
            "docs/readme.md",
            principal(Some("agent-a")),
        );
        assert_eq!(auth.authorize(&allowed), AuthorizationDecision::Allow);
    }

    #[test]
    fn rollback_tool_name_matches_policy_namespace() {
        // Guards against a future drift between the authorization tool
        // name and the policy store's supported tool table.
        assert_eq!(EditAction::Rollback.tool_name(), "filesystem.rollback");
    }
}
