use super::error::BuiltinToolAuthorizationError;
use super::permissions::Permission;
use super::tool_registry::{self, ToolRisk};
use super::{
    audit_deny, can_enable, McpAuthorizationError, McpPermissions, PersistentTrustStore, TrustStore,
};

/// A request to execute an MCP server, carrying the identity, version, and
/// requested permissions that must be authorized against a [`TrustStore`].
#[derive(Debug, Clone)]
pub struct McpExecutionRequest<'a> {
    /// MCP server id.
    pub id: &'a str,
    /// Server version requesting execution.
    pub version: &'a str,
    /// Permissions requested by the server.
    pub permissions: &'a McpPermissions,
}

/// Authorizes an MCP execution request against the trust store, failing closed
/// for missing, blocked, mismatched-version, or over-broad permission requests.
pub fn authorize(
    request: &McpExecutionRequest<'_>,
    trust: &TrustStore,
) -> Result<(), McpAuthorizationError> {
    if request.id.trim().is_empty() {
        return Err(McpAuthorizationError::MissingId);
    }
    let approval = trust.get(request.id);
    if approval.is_none() {
        audit_deny("authorize_mcp_execution", "no_approval", request.id);
        return Err(McpAuthorizationError::NoApproval {
            id: request.id.to_string(),
        });
    }
    if !can_enable(approval, request.permissions, request.version) {
        audit_deny("authorize_mcp_execution", "trust_mismatch", request.id);
        return Err(McpAuthorizationError::Mismatch {
            id: request.id.to_string(),
        });
    }
    Ok(())
}

/// Reserved trust identity for this workspace's own built-in (static) tool
/// execution — "awh.builtin".
///
/// This is deliberately NOT an external server id: it names the dispatcher's
/// own static tool surface (`terminal.run`, `workspace.write_file`, `git.*`,
/// `github.*`, and every other built-in tool) as one trustable subject, so a
/// single persistent trust record can restrict built-in tool execution using
/// the exact same [`PersistentTrustStore`]/[`authorize`] machinery that gates
/// custom MCP servers.
///
/// Semantics (Phase 2 — binary trust, per coarse permission):
/// * **No record** for this id (the common case, and every pre-existing
///   workspace) means the workspace has opted into no restriction: every
///   built-in tool call behaves exactly as before.
/// * **A record** means the workspace has opted in: the tool's
///   registry-declared `required_permissions` must be covered by the record's
///   approved permissions, and the record's trust level must permit
///   execution. Anything not covered is denied before the tool service runs.
///
/// The same string doubles as the reserved scope label granted inside the
/// record's list-valued permissions (filesystem/environment/secrets):
/// Phase 2 is binary per permission, so granting e.g. the filesystem
/// capability means the approval's filesystem list contains this label — it
/// is a reserved scope marker, never a real path. Path-scoped policy arrives
/// with the Phase 3 policy engine; per-agent scoping with Phase 5.
pub const BUILTIN_TOOL_TRUST_ID: &str = "awh.builtin";

/// Version marker the built-in gate requests, matching the `--version local`
/// default of the `awh mcp trust` CLI command (the same marker the
/// custom-server gate in `dispatcher::is_authorized` uses).
const BUILTIN_TOOL_TRUST_VERSION: &str = "local";

/// Authorization checkpoint for built-in (static) tool invocations whose
/// registry risk is Medium or High.
///
/// This reuses the exact [`authorize`] machinery already gating custom MCP
/// servers — there is no second, parallel trust mechanism. It differs from
/// the custom-server path in one deliberate, documented way: the absence of a
/// trust record for [`BUILTIN_TOOL_TRUST_ID`] means "no restriction
/// configured" (backward compatible) rather than "untrusted" — the workspace's
/// own tools were executing freely before this gate existed, and a missing
/// record must not change that. Every other path fails closed:
///
/// * the name is not in the tool registry → deny (the registry is the source
///   of truth for risk; an ungatable name must not execute),
/// * the store itself is unavailable (e.g. corrupt `trust.json`) → deny,
/// * a record exists with a blocking trust level or version → deny,
/// * a record exists but does not grant a required permission → deny,
///   naming the missing permission.
///
/// Low-risk (read-only) tools are out of the gate's scope by design and pass
/// through, so they are unchanged by this phase. Dynamic (`provider.tool`)
/// tools never reach this function — they are gated by the existing
/// custom-provider path.
pub fn authorize_builtin_tool(
    tool: &str,
    trust: Option<&PersistentTrustStore>,
) -> Result<(), BuiltinToolAuthorizationError> {
    let Some(definition) = tool_registry::registry_lookup(tool) else {
        audit_deny("builtin_tool_denied", "unregistered_tool", tool);
        return Err(BuiltinToolAuthorizationError::Unregistered {
            tool: tool.to_string(),
        });
    };
    // Low-risk static tools (reads) are explicitly out of scope for this gate.
    if definition.risk == ToolRisk::Low {
        return Ok(());
    }
    let Some(store) = trust else {
        audit_deny("builtin_tool_denied", "trust_store_unavailable", tool);
        return Err(BuiltinToolAuthorizationError::StoreUnavailable {
            tool: tool.to_string(),
        });
    };
    let store = store.to_store();
    // Backward-compatible default: no record for the reserved identity means
    // the workspace never opted into built-in tool restrictions, so every
    // Medium/High built-in tool keeps its pre-gate behavior.
    let Some(approval) = store.get(BUILTIN_TOOL_TRUST_ID) else {
        return Ok(());
    };
    let requested = requested_permissions(definition.required_permissions);
    let request = McpExecutionRequest {
        id: BUILTIN_TOOL_TRUST_ID,
        version: BUILTIN_TOOL_TRUST_VERSION,
        permissions: &requested,
    };
    match authorize(&request, &store) {
        Ok(()) => Ok(()),
        // Defensive: the record existed immediately above, so these states
        // cannot arise — but they fail closed rather than panic if they ever do.
        Err(McpAuthorizationError::NoApproval { .. }) | Err(McpAuthorizationError::MissingId) => {
            audit_deny("builtin_tool_denied", "no_approval", tool);
            Err(BuiltinToolAuthorizationError::TrustLevelDenied {
                tool: tool.to_string(),
                id: BUILTIN_TOOL_TRUST_ID.to_string(),
            })
        }
        Err(McpAuthorizationError::Mismatch { .. }) => {
            // `authorize` already audited this denial
            // ("authorize_mcp_execution"/"trust_mismatch"); name the first
            // missing permission precisely so the caller's error is actionable.
            let missing = definition
                .required_permissions
                .iter()
                .copied()
                .find(|permission| !approval.approved_permissions.allows(*permission));
            match missing {
                Some(permission) => {
                    audit_deny("builtin_tool_denied", "permission_not_granted", tool);
                    Err(BuiltinToolAuthorizationError::PermissionDenied {
                        tool: tool.to_string(),
                        permission,
                        id: BUILTIN_TOOL_TRUST_ID.to_string(),
                    })
                }
                None => {
                    // Every required permission is granted individually, so
                    // the denial came from the trust level or version.
                    audit_deny("builtin_tool_denied", "trust_level_or_version", tool);
                    Err(BuiltinToolAuthorizationError::TrustLevelDenied {
                        tool: tool.to_string(),
                        id: BUILTIN_TOOL_TRUST_ID.to_string(),
                    })
                }
            }
        }
    }
}

/// Synthesizes the permission request a built-in tool's registry-declared
/// capabilities translate to, so the shared `can_enable` subset check can
/// evaluate them against an approval.
///
/// List-valued permissions (filesystem/environment/secrets) request the
/// reserved [`BUILTIN_TOOL_TRUST_ID`] scope label rather than a real path:
/// Phase 2 is binary per permission, so "the filesystem capability is granted"
/// means the approval's filesystem list contains that label.
fn requested_permissions(required: &[Permission]) -> McpPermissions {
    let mut requested = McpPermissions::default();
    for permission in required {
        match permission {
            Permission::Network => requested.network = true,
            Permission::Process => requested.process = true,
            Permission::Filesystem => {
                requested.filesystem = vec![BUILTIN_TOOL_TRUST_ID.to_string()]
            }
            Permission::Environment => {
                requested.environment = vec![BUILTIN_TOOL_TRUST_ID.to_string()]
            }
            // `can_enable` checks the secrets subset independently, so the
            // environment side of the secrets⊆environment invariant is not
            // needed here (and `validate` is only ever run on approvals).
            Permission::Secrets => requested.secrets = vec![BUILTIN_TOOL_TRUST_ID.to_string()],
        }
    }
    requested
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{McpPermissions, TrustLevel};

    fn request<'a>(id: &'a str, permissions: &'a McpPermissions) -> McpExecutionRequest<'a> {
        McpExecutionRequest {
            id,
            version: "1.0",
            permissions,
        }
    }

    /// A persistent store granting the built-in identity the given coarse
    /// capabilities (matching what `awh mcp trust awh.builtin --...` writes).
    fn builtin_store(
        level: TrustLevel,
        network: bool,
        process: bool,
        filesystem: bool,
    ) -> PersistentTrustStore {
        let mut store = TrustStore::default();
        store
            .approve(
                BUILTIN_TOOL_TRUST_ID,
                level,
                McpPermissions {
                    network,
                    process,
                    filesystem: if filesystem {
                        vec![BUILTIN_TOOL_TRUST_ID.to_string()]
                    } else {
                        Vec::new()
                    },
                    ..McpPermissions::default()
                },
                BUILTIN_TOOL_TRUST_VERSION.to_string(),
            )
            .expect("valid approval");
        PersistentTrustStore::from_store(&store)
    }

    #[test]
    fn empty_id_is_missing_id() {
        let trust = TrustStore::default();
        let perms = McpPermissions::default();
        assert_eq!(
            authorize(&request("  ", &perms), &trust),
            Err(McpAuthorizationError::MissingId)
        );
    }

    #[test]
    fn unknown_id_is_no_approval() {
        let mut trust = TrustStore::default();
        trust
            .approve(
                "known",
                TrustLevel::Trusted,
                McpPermissions::default(),
                "1.0",
            )
            .unwrap();
        let perms = McpPermissions::default();
        assert_eq!(
            authorize(&request("missing", &perms), &trust),
            Err(McpAuthorizationError::NoApproval {
                id: "missing".to_string()
            })
        );
    }

    #[test]
    fn mismatched_permissions_are_mismatch() {
        let mut trust = TrustStore::default();
        trust
            .approve(
                "server",
                TrustLevel::Trusted,
                McpPermissions::default(),
                "1.0",
            )
            .unwrap();
        // Request network access that was not approved.
        let perms = McpPermissions {
            network: true,
            ..McpPermissions::default()
        };
        assert_eq!(
            authorize(&request("server", &perms), &trust),
            Err(McpAuthorizationError::Mismatch {
                id: "server".to_string()
            })
        );
    }

    #[test]
    fn matching_approval_is_allowed() {
        let mut trust = TrustStore::default();
        trust
            .approve(
                "server",
                TrustLevel::Trusted,
                McpPermissions::default(),
                "1.0",
            )
            .unwrap();
        let perms = McpPermissions::default();
        assert_eq!(authorize(&request("server", &perms), &trust), Ok(()));
    }

    // ------------------------------------------------------------------
    // Built-in (static) tool gate
    // ------------------------------------------------------------------

    #[test]
    fn builtin_gate_allows_when_no_record_exists() {
        // Backward compatibility: a workspace with no "awh.builtin" record
        // must not see any behavior change for Medium/High static tools.
        let store = PersistentTrustStore::from_store(&TrustStore::default());
        for tool in [
            "terminal.run",
            "git.commit",
            "workspace.write_file",
            "memory.store",
        ] {
            assert_eq!(
                authorize_builtin_tool(tool, Some(&store)),
                Ok(()),
                "{tool} must stay allowed without an awh.builtin record"
            );
        }
    }

    #[test]
    fn builtin_gate_allows_low_risk_tools_even_when_restricted() {
        // Read-only tools are out of the gate's scope by design: restricting
        // awh.builtin must not break reads.
        let store = builtin_store(TrustLevel::Blocked, false, false, false);
        for tool in [
            "workspace.read_file",
            "git.status",
            "memory.get",
            "skills.list",
        ] {
            assert_eq!(
                authorize_builtin_tool(tool, Some(&store)),
                Ok(()),
                "low-risk {tool} must not be gated"
            );
        }
    }

    #[test]
    fn builtin_gate_denies_missing_store_failing_closed() {
        // A store that could not be loaded (corrupt trust file, or no home
        // directory) must deny rather than fall back to allow.
        assert_eq!(
            authorize_builtin_tool("terminal.run", None),
            Err(BuiltinToolAuthorizationError::StoreUnavailable {
                tool: "terminal.run".to_string()
            })
        );
    }

    #[test]
    fn builtin_gate_denies_unregistered_tools() {
        let store = PersistentTrustStore::from_store(&TrustStore::default());
        assert_eq!(
            authorize_builtin_tool("not.a.tool", Some(&store)),
            Err(BuiltinToolAuthorizationError::Unregistered {
                tool: "not.a.tool".to_string()
            })
        );
    }

    #[test]
    fn builtin_gate_names_the_missing_permission() {
        // A Reviewed record granting nothing must reject a Process-requiring
        // tool, naming the tool and the missing permission.
        let store = builtin_store(TrustLevel::Reviewed, false, false, false);
        assert_eq!(
            authorize_builtin_tool("terminal.run", Some(&store)),
            Err(BuiltinToolAuthorizationError::PermissionDenied {
                tool: "terminal.run".to_string(),
                permission: Permission::Process,
                id: BUILTIN_TOOL_TRUST_ID.to_string(),
            })
        );
        // ...and a Filesystem-requiring tool likewise.
        assert_eq!(
            authorize_builtin_tool("git.commit", Some(&store)),
            Err(BuiltinToolAuthorizationError::PermissionDenied {
                tool: "git.commit".to_string(),
                permission: Permission::Filesystem,
                id: BUILTIN_TOOL_TRUST_ID.to_string(),
            })
        );
    }

    #[test]
    fn builtin_gate_denies_blocked_level_even_with_permissions() {
        // A Blocked record denies regardless of the granted capabilities,
        // and reports the trust-level cause (no single missing permission).
        let store = builtin_store(TrustLevel::Blocked, true, true, true);
        assert_eq!(
            authorize_builtin_tool("terminal.run", Some(&store)),
            Err(BuiltinToolAuthorizationError::TrustLevelDenied {
                tool: "terminal.run".to_string(),
                id: BUILTIN_TOOL_TRUST_ID.to_string(),
            })
        );
    }

    #[test]
    fn builtin_gate_denies_version_mismatch() {
        // A record pinned to a different version marker denies every gated
        // tool, exactly like the custom-server gate.
        let mut trust = TrustStore::default();
        trust
            .approve(
                BUILTIN_TOOL_TRUST_ID,
                TrustLevel::Trusted,
                McpPermissions {
                    network: true,
                    process: true,
                    filesystem: vec![BUILTIN_TOOL_TRUST_ID.to_string()],
                    ..McpPermissions::default()
                },
                "9.9.9".to_string(),
            )
            .unwrap();
        let store = PersistentTrustStore::from_store(&trust);
        assert_eq!(
            authorize_builtin_tool("terminal.run", Some(&store)),
            Err(BuiltinToolAuthorizationError::TrustLevelDenied {
                tool: "terminal.run".to_string(),
                id: BUILTIN_TOOL_TRUST_ID.to_string(),
            })
        );
    }

    #[test]
    fn builtin_gate_allows_fully_granted_record() {
        // The full grant (what `awh mcp trust awh.builtin --network --process
        // --filesystem` writes) permits every permission shape.
        let store = builtin_store(TrustLevel::Reviewed, true, true, true);
        for tool in [
            "terminal.run",
            "git.commit",
            "workspace.write_file",
            "github.pr_create",
            "memory.store",
        ] {
            assert_eq!(
                authorize_builtin_tool(tool, Some(&store)),
                Ok(()),
                "{tool} must be allowed under a full grant"
            );
        }
    }

    #[test]
    fn builtin_gate_allows_empty_permission_tools_under_partial_grant() {
        // Phase 2 is binary per permission: a tool whose registry entry
        // declares no required permissions (memory.store, tasks.create) is
        // governed only by the record's trust level, so a Reviewed record
        // with no capabilities still allows it.
        let store = builtin_store(TrustLevel::Reviewed, false, false, false);
        assert_eq!(authorize_builtin_tool("memory.store", Some(&store)), Ok(()));
        assert_eq!(authorize_builtin_tool("tasks.create", Some(&store)), Ok(()));
    }

    #[test]
    fn requested_permissions_map_each_capability() {
        let requested = requested_permissions(&[
            Permission::Network,
            Permission::Process,
            Permission::Filesystem,
            Permission::Environment,
            Permission::Secrets,
        ]);
        assert!(requested.network);
        assert!(requested.process);
        assert_eq!(requested.filesystem, vec![BUILTIN_TOOL_TRUST_ID]);
        assert_eq!(requested.environment, vec![BUILTIN_TOOL_TRUST_ID]);
        assert_eq!(requested.secrets, vec![BUILTIN_TOOL_TRUST_ID]);
        // The empty declaration requests nothing.
        let empty = requested_permissions(&[]);
        assert!(!empty.network);
        assert!(!empty.process);
        assert!(empty.filesystem.is_empty());
        assert!(empty.environment.is_empty());
        assert!(empty.secrets.is_empty());
    }
}
