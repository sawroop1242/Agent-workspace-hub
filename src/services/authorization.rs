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

/// How the caller's authority is established. The distinction is
/// structural: an operator context must be *explicitly claimed* by the
/// in-process/CLI caller that already represents a trusted human or
/// service, and an agent context must carry both agent and session
/// identity. Absence of an agent id is never itself authority (§ caller
/// semantics: an ambiguous caller must not become Allow by omission).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrincipalKind {
    /// A trusted in-process/CLI operator invocation: no agent acts, the
    /// process itself was started by a human operator or a trusted
    /// service. Only reachable by construction inside the process —
    /// no network transport may create this variant.
    Operator,
    /// An agent acting under a runtime session. Requires `agent_id` and
    /// `session_id` to be present; both are supplied by trusted runtime
    /// state (session/route binding), never by request fields.
    Agent,
}

/// The explicit principal/caller identity attached to an authorization
/// request. This is never derived from a URL, route, or transport detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizingPrincipal {
    /// Whether this is an explicitly trusted operator invocation or an
    /// agent-scoped request. See [`PrincipalKind`].
    pub kind: PrincipalKind,
    /// The agent the request acts on behalf of. `Some` iff
    /// [`PrincipalKind::Agent`].
    pub agent_id: Option<String>,
    /// A session identifier, required when agent identity is claimed.
    pub session_id: Option<String>,
    /// Workspace identifier (the workspace root path).
    pub workspace_id: String,
}

impl AuthorizingPrincipal {
    /// The explicit operator context. Callers are trusted in-process
    /// surfaces (CLI, application services, tests) — never a transport.
    pub fn operator(workspace_id: impl Into<String>) -> Self {
        Self {
            kind: PrincipalKind::Operator,
            agent_id: None,
            session_id: None,
            workspace_id: workspace_id.into(),
        }
    }

    /// An agent-scoped principal from *trusted runtime state* (the
    /// session/route binding), never from request fields.
    pub fn agent(
        agent_id: impl Into<String>,
        session_id: impl Into<String>,
        workspace_id: impl Into<String>,
    ) -> Self {
        Self {
            kind: PrincipalKind::Agent,
            agent_id: Some(agent_id.into()),
            session_id: Some(session_id.into()),
            workspace_id: workspace_id.into(),
        }
    }
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

impl DenialReason {
    /// The stable snake_case reason code recorded in the durable audit
    /// store (AWE-013): machine-readable, never a secret, identical for
    /// MCP/CLI/API consumers.
    pub fn to_audit_code(&self) -> &'static str {
        match self {
            DenialReason::MissingAgentIdentity => "missing_agent_identity",
            DenialReason::UnknownAgent => "unknown_agent",
            DenialReason::MissingCapability => "missing_capability",
            DenialReason::ExpiredGrant => "expired_grant",
            DenialReason::OutOfScope => "out_of_scope",
            DenialReason::PolicyDenied => "policy_denied",
            DenialReason::InfrastructureUnavailable => "infrastructure_unavailable",
            DenialReason::UnsupportedOperation => "unsupported_operation",
        }
    }
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
    root: PathBuf,
    capability_store: CapabilityGrantStore,
    policy_store: PolicyStore,
    agent_store: crate::core::agents::AgentStore,
}

impl EditAuthorizer {
    pub fn new(workspace_root: PathBuf) -> Self {
        let agent_store = crate::core::agents::AgentStore::new(&workspace_root);
        Self {
            root: workspace_root.clone(),
            capability_store: CapabilityGrantStore::new(&workspace_root),
            policy_store: PolicyStore::new(&workspace_root),
            agent_store,
        }
    }

    /// Whether the agent is registered in this workspace (read-only
    /// consumption of the existing registry — never a second identity
    /// store). An unreadable store, or a structurally unsafe id, fails
    /// closed: the agent is reported as not existing rather than guessed.
    fn agent_exists(&self, agent_id: &str) -> bool {
        crate::core::agents::is_safe_agent_id(agent_id)
            && matches!(self.agent_store.get(agent_id), Ok(Some(_)))
    }

    /// The single authorization entry point. Every edit, and every rollback,
    /// is authorized exactly here. Denial precedence (§20: a lower-priority
    /// positive condition may never override a higher-priority failure):
    ///
    /// ```text
    /// invalid/untrusted caller context      (missing agent/session identity)
    ///   → workspace binding failure          (claim ≠ manifest workspace)
    ///   → unsafe/ambiguous resource          (syntax rejected before stores)
    ///   → policy infrastructure failure      (unreadable policy store)
    ///   → policy deny                        (matching deny rule)
    ///   → capability infrastructure failure  (unreadable capability store)
    ///   → missing capability / unknown agent
    ///   → expired grant
    ///   → out-of-scope grant
    ///   → Allow
    /// ```
    pub fn authorize(&self, request: &AuthorizationRequest) -> AuthorizationDecision {
        let decision = self.authorize_inner(request);
        // AWE-013/TW-007: authorization decisions are consequential
        // security outcomes — denials (and only denials; allow-side
        // accountability is recorded by the mutating service boundary
        // once it observes the final operation outcome) are recorded in
        // the canonical durable audit store with the stable reason code
        // and safe identity correlation. Audit failure never rewrites
        // the decision.
        if let AuthorizationDecision::Deny { reason, detail } = &decision {
            crate::services::audit::record_outcome(
                "deny",
                &format!("authorization.{}", request.action.tool_name()),
                request
                    .principal
                    .agent_id
                    .as_deref()
                    .unwrap_or("unattributed"),
                detail,
                &crate::services::audit::AuditCorrelation {
                    workspace_id: Some(request.principal.workspace_id.clone()),
                    agent_id: request.principal.agent_id.clone(),
                    session_id: request.principal.session_id.clone(),
                    edit_id: request.transaction_id.clone(),
                    snapshot_id: None,
                    reason: Some(reason.to_audit_code().to_owned()),
                },
            );
        }
        decision
    }

    fn authorize_inner(&self, request: &AuthorizationRequest) -> AuthorizationDecision {
        // Step 1: caller context. An agent-scoped request must carry both
        // an agent identity and a session identity; absence of either is
        // an ambiguous caller and must never become Allow. Operator
        // authority is the explicitly claimed [`PrincipalKind::Operator`]
        // — a request that merely *lacks* an agent id (Agent kind with
        // `None` fields) is denied, not silently upgraded.
        match request.principal.kind {
            PrincipalKind::Agent => {
                let Some(agent_id) = request.principal.agent_id.as_deref() else {
                    return AuthorizationDecision::denied(
                        "agent-scoped request carries no agent identity",
                        DenialReason::MissingAgentIdentity,
                    );
                };
                if request
                    .principal
                    .session_id
                    .as_deref()
                    .is_none_or(str::is_empty)
                {
                    return AuthorizationDecision::denied(
                        format!("agent {agent_id} has no runtime session identity"),
                        DenialReason::MissingAgentIdentity,
                    );
                }
            }
            PrincipalKind::Operator => {}
        }

        // Step 2: workspace binding. The claimed workspace must be this
        // workspace (the authorizer is constructed for one root); a
        // principal from another workspace never authorizes here. The
        // manifest is the canonical workspace identity — an unreadable
        // manifest fails closed like every other store.
        let manifest = match crate::services::init::load_workspace_manifest(&self.root) {
            Ok(manifest) => manifest,
            Err(error) => {
                return AuthorizationDecision::denied(
                    format!("workspace identity unavailable: {error}"),
                    DenialReason::InfrastructureUnavailable,
                )
            }
        };
        if request.principal.workspace_id != manifest.workspace_id.as_str() {
            return AuthorizationDecision::denied(
                "caller is not bound to this workspace",
                DenialReason::OutOfScope,
            );
        }

        // Step 3: resource syntax. Path ambiguity is rejected before any
        // store is consulted: an unsafe resource can never be brought
        // into scope by normalization, an unscoped grant, or a policy
        // gap. Containment/resolution itself stays in the filesystem
        // boundary — this is the authorization-side syntax floor.
        if !is_authorizable_resource(&request.resource) {
            return AuthorizationDecision::denied(
                format!(
                    "resource {:?} is not a safe workspace-relative path",
                    request.resource
                ),
                DenialReason::UnsupportedOperation,
            );
        }

        // Step 4: policy first. If a deny rule matches, nothing else
        // matters — policy denial always wins (deny precedence).
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

        // Step 5: operator requests stop here — policy is the operator's
        // authorization surface. Agent requests continue to capability
        // evaluation. The agent must be registered in this workspace:
        // an unknown agent is denied as unknown, not merely as
        // "no grants", so callers can distinguish the two conditions.
        if matches!(request.principal.kind, PrincipalKind::Operator) {
            return AuthorizationDecision::Allow;
        }
        let agent_id = request
            .principal
            .agent_id
            .as_deref()
            .expect("checked above");
        if !self.agent_exists(agent_id) {
            return AuthorizationDecision::denied(
                format!("agent {agent_id} is unknown to this workspace"),
                DenialReason::UnknownAgent,
            );
        }

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
///
/// Both sides are rejected outright when they contain a `..` component:
/// a traversal-shaped resource is never covered by any scope, and a
/// traversal-shaped scope covers nothing (§9: no empty/ambiguous scope
/// semantics). Normalization only collapses repeated separators — it
/// never widens authority (§30).
pub(crate) fn scope_covers(scope: &str, resource: &str) -> bool {
    if has_parent_component(scope) || has_parent_component(resource) {
        return false;
    }
    let scope = normalize_forward_slash(scope);
    let resource = normalize_forward_slash(resource);
    if scope.is_empty() {
        return false;
    }
    resource == scope || resource.starts_with(&format!("{scope}/"))
}

/// Whether the path contains a `..` path component (a traversal shape).
fn has_parent_component(path: &str) -> bool {
    path.split('/').any(|component| component == "..")
}

/// The authorization-side resource syntax floor: a workspace-relative
/// path with no empty/absolute/traversal/backslash/control-character
/// ambiguity may be authorized; anything else is rejected *before* the
/// capability or policy stores are consulted, so an unsafe resource can
/// never be brought into scope by an unscoped grant or by normalization.
///
/// This mirrors the edit model's `validate_path` syntax rules on purpose
/// (one resource representation across the boundary); filesystem
/// containment and symlink resolution remain the filesystem service's
/// exclusive responsibility.
pub(crate) fn is_authorizable_resource(resource: &str) -> bool {
    use std::path::{Component, Path};
    if resource.is_empty() || resource.chars().any(|c| c.is_control()) {
        return false;
    }
    let path = Path::new(resource);
    if path.is_absolute() || resource.contains('\\') {
        return false;
    }
    !path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
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
pub(crate) fn grant_is_expired(grant: &CapabilityGrant) -> bool {
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

    /// An initialized test workspace: the authorizer needs the canonical
    /// manifest for workspace binding, and agents must exist in the
    /// registry for UnknownAgent to be distinguishable from
    /// MissingCapability.
    fn workspace() -> (tempfile::TempDir, EditAuthorizer) {
        let temp = tempfile::tempdir().unwrap();
        crate::services::init::initialize_workspace(temp.path()).unwrap();
        let auth = EditAuthorizer::new(temp.path().to_path_buf());
        (temp, auth)
    }

    fn register_agent(root: &Path, agent_id: &str) {
        crate::core::agents::AgentStore::new(root)
            .create(&crate::models::Agent {
                id: agent_id.into(),
                name: agent_id.into(),
                role: "test".into(),
                status: crate::models::AgentStatus::Active,
                enabled: true,
                created_at: Utc::now().to_rfc3339(),
            })
            .unwrap();
    }

    fn agent_principal(agent: Option<&str>, workspace_id: &str) -> AuthorizingPrincipal {
        match agent {
            Some(agent) => AuthorizingPrincipal::agent(agent, "session-1", workspace_id),
            None => AuthorizingPrincipal::operator(workspace_id),
        }
    }

    fn workspace_id_of(temp: &tempfile::TempDir) -> String {
        crate::services::init::load_workspace_manifest(temp.path())
            .unwrap()
            .workspace_id
            .as_str()
            .to_owned()
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
        let (temp, auth) = workspace();
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            agent_principal(None, &workspace_id_of(&temp)),
        );
        assert_eq!(auth.authorize(&req), AuthorizationDecision::Allow);
    }

    #[test]
    fn agent_valid_grant_is_allowed() {
        let (temp, auth) = workspace();
        register_agent(temp.path(), "agent-a");
        write_grant_store(temp.path(), grant("g1", "agent-a", None, None));
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            agent_principal(Some("agent-a"), &workspace_id_of(&temp)),
        );
        assert_eq!(auth.authorize(&req), AuthorizationDecision::Allow);
    }

    #[test]
    fn agent_with_wrong_identity_is_denied() {
        let (temp, auth) = workspace();
        register_agent(temp.path(), "agent-a");
        register_agent(temp.path(), "agent-b");
        write_grant_store(temp.path(), grant("g1", "agent-a", None, None));
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            agent_principal(Some("agent-b"), &workspace_id_of(&temp)),
        );
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::MissingCapability,
                ..
            }
        ));
    }

    /// §8: an agent id that is not registered in this workspace is
    /// denied as UnknownAgent — a distinct, stable reason from a
    /// registered agent that merely holds no grants.
    #[test]
    fn unregistered_agent_is_denied_as_unknown_agent() {
        let (temp, auth) = workspace();
        write_grant_store(temp.path(), grant("g1", "ghost", None, None));
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            agent_principal(Some("ghost"), &workspace_id_of(&temp)),
        );
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::UnknownAgent,
                ..
            }
        ));
    }

    /// §6 caller semantics: an ambiguous agent-scoped request — Agent
    /// kind with no session identity — must never become Allow, and an
    /// agent id that is merely absent must never be upgraded to operator
    /// authority.
    #[test]
    fn agent_kind_without_session_identity_is_denied() {
        let (temp, auth) = workspace();
        let ws = workspace_id_of(&temp);
        let mut principal = AuthorizingPrincipal::agent("agent-a", "session-1", &ws);
        principal.session_id = None;
        let req = request(EditAction::Replace, "src/main.rs", principal);
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::MissingAgentIdentity,
                ..
            }
        ));
    }

    #[test]
    fn agent_kind_without_agent_identity_is_denied() {
        let (temp, auth) = workspace();
        let ws = workspace_id_of(&temp);
        let mut principal = AuthorizingPrincipal::agent("agent-a", "session-1", &ws);
        principal.agent_id = None;
        let req = request(EditAction::Replace, "src/main.rs", principal);
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::MissingAgentIdentity,
                ..
            }
        ));
    }

    /// §8: a principal bound to a different workspace never authorizes
    /// in this one.
    #[test]
    fn principal_from_another_workspace_is_denied() {
        let (_temp, auth) = workspace();
        let principal = AuthorizingPrincipal::agent("agent-a", "session-1", "ws-other");
        let req = request(EditAction::Replace, "src/main.rs", principal);
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::OutOfScope,
                ..
            }
        ));
    }

    /// §33 Step 5 / §30: unsafe resources are rejected before any store
    /// is consulted — even for an operator, even with an unscoped grant
    /// in place. Previously `../outside` was Allow under an unscoped
    /// grant, relying on a later layer to reject it.
    #[test]
    fn unsafe_resource_is_denied_before_capability_evaluation() {
        let (temp, auth) = workspace();
        register_agent(temp.path(), "agent-a");
        write_grant_store(temp.path(), grant("g1", "agent-a", None, None));
        let ws = workspace_id_of(&temp);
        for resource in [
            "../outside.txt",
            "/abs/path.txt",
            "a\\b.txt",
            "a\u{0000}.txt",
            "",
            "src/foo/../secret.rs",
        ] {
            let agent_req = request(
                EditAction::Replace,
                resource,
                agent_principal(Some("agent-a"), &ws),
            );
            assert!(
                matches!(
                    auth.authorize(&agent_req),
                    AuthorizationDecision::Deny {
                        reason: DenialReason::UnsupportedOperation,
                        ..
                    }
                ),
                "agent resource {resource:?} must be denied as unsafe"
            );
            let operator_req = request(EditAction::Replace, resource, agent_principal(None, &ws));
            assert!(
                matches!(
                    auth.authorize(&operator_req),
                    AuthorizationDecision::Deny {
                        reason: DenialReason::UnsupportedOperation,
                        ..
                    }
                ),
                "operator resource {resource:?} must be denied as unsafe"
            );
        }
        // A well-formed nested resource with repeated separators is
        // normalized, not widened: `dir//nested` authorizes exactly like
        // `dir/nested`, and an unscoped grant still covers it.
        let req = request(
            EditAction::Replace,
            "dir//nested.txt",
            agent_principal(Some("agent-a"), &ws),
        );
        assert_eq!(auth.authorize(&req), AuthorizationDecision::Allow);
    }

    #[test]
    fn expired_grant_is_denied() {
        let (temp, auth) = workspace();
        register_agent(temp.path(), "agent-a");
        let past = (Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        write_grant_store(temp.path(), grant("g1", "agent-a", None, Some(&past)));
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            agent_principal(Some("agent-a"), &workspace_id_of(&temp)),
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
        let (temp, auth) = workspace();
        register_agent(temp.path(), "agent-a");
        write_grant_store(temp.path(), grant("g1", "agent-a", Some("src/foo"), None));
        let req = request(
            EditAction::Replace,
            "src/foobar.rs", // NOT inside the scope; needs prefix boundary
            agent_principal(Some("agent-a"), &workspace_id_of(&temp)),
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
        let (temp, auth) = workspace();
        register_agent(temp.path(), "agent-a");
        write_grant_store(temp.path(), grant("g1", "agent-a", None, None));
        write_policy(temp.path(), "deny-foo", "filesystem.replace", "src/foo");
        let req = request(
            EditAction::Replace,
            "src/foo/x.rs",
            agent_principal(Some("agent-a"), &workspace_id_of(&temp)),
        );
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::PolicyDenied,
                ..
            }
        ));
    }

    /// §12: a corrupt policy store is an infrastructure failure, never
    /// an Allow — even when a valid capability exists.
    #[test]
    fn corrupt_policy_store_fails_closed() {
        let (temp, auth) = workspace();
        register_agent(temp.path(), "agent-a");
        write_grant_store(temp.path(), grant("g1", "agent-a", None, None));
        fs::write(temp.path().join(".agent").join("policy.json"), "{not json").unwrap();
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            agent_principal(Some("agent-a"), &workspace_id_of(&temp)),
        );
        assert!(matches!(
            auth.authorize(&req),
            AuthorizationDecision::Deny {
                reason: DenialReason::InfrastructureUnavailable,
                ..
            }
        ));
    }

    #[test]
    fn malformed_expiry_fails_closed() {
        let (temp, auth) = workspace();
        register_agent(temp.path(), "agent-a");
        write_grant_store(
            temp.path(),
            grant("g1", "agent-a", None, Some("not-a-date")),
        );
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            agent_principal(Some("agent-a"), &workspace_id_of(&temp)),
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
        let (temp, auth) = workspace();
        register_agent(temp.path(), "agent-a");
        write_grant_store(temp.path(), grant("g1", "agent-a", None, None));
        write_policy(temp.path(), "deny-rb", "filesystem.rollback", "src/");
        let ws = workspace_id_of(&temp);
        let denied = request(
            EditAction::Rollback,
            "src/main.rs",
            agent_principal(Some("agent-a"), &ws),
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
            agent_principal(Some("agent-a"), &ws),
        );
        assert_eq!(auth.authorize(&allowed), AuthorizationDecision::Allow);
    }

    #[test]
    fn rollback_tool_name_matches_policy_namespace() {
        // Guards against a future drift between the authorization tool
        // name and the policy store's supported tool table.
        assert_eq!(EditAction::Rollback.tool_name(), "filesystem.rollback");
    }

    /// §10 expiry table: a future expiry is eligible today.
    #[test]
    fn future_expiry_is_eligible() {
        let (temp, auth) = workspace();
        register_agent(temp.path(), "agent-a");
        let future = (Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        write_grant_store(temp.path(), grant("g1", "agent-a", None, Some(&future)));
        let req = request(
            EditAction::Replace,
            "src/main.rs",
            agent_principal(Some("agent-a"), &workspace_id_of(&temp)),
        );
        assert_eq!(auth.authorize(&req), AuthorizationDecision::Allow);
    }

    /// §9 scope semantics: component-aware coverage with no widening.
    #[test]
    fn scope_covers_component_boundaries_without_widening() {
        assert!(scope_covers("src/foo", "src/foo"));
        assert!(scope_covers("src/foo", "src/foo/bar.rs"));
        assert!(!scope_covers("src/foo", "src/foobar.rs"));
        assert!(!scope_covers("src/foo", "other/src/foo"));
        assert!(!scope_covers("", "src/foo"));
        // Normalization is idempotent and never widens.
        assert!(scope_covers("src//foo", "src/foo/x.rs"));
        assert!(scope_covers("src/foo", "src//foo/x.rs"));
        assert!(!scope_covers("src/foo", "src/foo/../secret.rs"));
    }

    /// §25 decision stability: identical inputs produce the identical
    /// decision and reason, repeatedly.
    #[test]
    fn decisions_are_deterministic_for_identical_inputs() {
        let (temp, auth) = workspace();
        register_agent(temp.path(), "agent-a");
        write_grant_store(temp.path(), grant("g1", "agent-a", Some("src"), None));
        let ws = workspace_id_of(&temp);
        let make = || {
            request(
                EditAction::Insert,
                "docs/outside.rs",
                agent_principal(Some("agent-a"), &ws),
            )
        };
        let first = auth.authorize(&make());
        for _ in 0..8 {
            assert_eq!(auth.authorize(&make()), first);
        }
        assert!(matches!(
            first,
            AuthorizationDecision::Deny {
                reason: DenialReason::MissingCapability,
                ..
            }
        ));
    }
}
