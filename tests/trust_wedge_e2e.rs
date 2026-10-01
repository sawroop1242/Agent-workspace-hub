//! Core Trust Wedge end-to-end integration (Prompt 18, §22 steps 4–5).
//!
//! One composed pass through the full authorization chain, in a single
//! test process so the process-wide durable audit root is bound exactly
//! once:
//!
//! ```text
//! MCP interface → identity (manifest) + session → per-agent capability
//! → built-in trust gate → workspace policy → canonical service →
//! canonical store → canonical audit (durable, survives restart)
//! ```
//!
//! …then continues into the editing plane over the same bound session:
//! write → edit transaction (with the session's principal) → rollback
//! by exact edit id — the snapshot/provenance tail of the §8 chain,
//! proven through the real filesystem boundary — and closes the chain
//! with worktree isolation: a REAL managed `git worktree` bound to the
//! same session, session-root isolation against foreign sessions, the
//! §10 ownership invariant on removal, and the owning session's own
//! remove.
//!
//! What this pins that the per-layer tests cannot, because they pin the
//! layers in isolation with synthetic identities:
//!
//! * the identity the audit trail attributes is the REAL workspace
//!   identity established by `awh init` — manifest workspace id, agent
//!   store record, session id — not an invented one;
//! * the gates LAYER in the contract order — a capability allow is
//!   overridden by a policy deny; a capability deny never reaches the
//!   policy layer (or the service);
//! * denial provably happens BEFORE the service runs (a command with an
//!   observable side effect leaves no trace when denied);
//! * revocation is live: grants are re-read on every call, never cached
//!   across a session;
//! * the whole story is durable: a fresh `AuditLog::open` (the restart
//!   path) reconstructs the ordered allow/deny sequence with every
//!   identity field intact.

use agent_workspace_hub::core::agents::AgentStore;
use agent_workspace_hub::core::capability_grants::CapabilityGrantStore;
use agent_workspace_hub::core::identity::{AgentId, SessionId, SessionIdentity};
use agent_workspace_hub::core::policy::PolicyStore;
use agent_workspace_hub::mcp::dispatcher::CAPABILITY_DENIED_CODE;
use agent_workspace_hub::mcp::{McpDispatcher, POLICY_DENIED_CODE};
use agent_workspace_hub::models::agent::{Agent, AgentStatus};
use agent_workspace_hub::models::policy_rule::PolicyRule;
use agent_workspace_hub::models::CapabilityGrant;
use agent_workspace_hub::services::audit::{init_global, AuditLog};
use agent_workspace_hub::services::init::{initialize_workspace, load_workspace_manifest};
use serde_json::{json, Value};

/// Approves `awh.builtin` at Reviewed with the process capability and the
/// builtin filesystem scope label — the explicit authorization
/// `terminal.run` (High) and the Medium `filesystem.*`/`workspace.*`
/// mutation surface require. Mirrors the `awh mcp trust` CLI record.
fn builtin_trust() -> agent_workspace_hub::mcp::PersistentTrustStore {
    let mut store = agent_workspace_hub::mcp::TrustStore::default();
    store
        .approve(
            agent_workspace_hub::mcp::BUILTIN_TOOL_TRUST_ID,
            agent_workspace_hub::mcp::TrustLevel::Reviewed,
            agent_workspace_hub::mcp::McpPermissions {
                process: true,
                filesystem: vec![agent_workspace_hub::mcp::BUILTIN_TOOL_TRUST_ID.to_string()],
                ..Default::default()
            },
            "local".to_string(),
        )
        .expect("valid approval");
    agent_workspace_hub::mcp::PersistentTrustStore::from_store(&store)
}

#[test]
fn trust_wedge_end_to_end_composes_and_survives_restart() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async move {
        // The workspace root is leaked: the process-wide durable audit
        // store keeps writing into it for the lifetime of this test
        // binary, so the tempdir must never be dropped.
        let holder = std::mem::ManuallyDrop::new(tempfile::tempdir().unwrap());
        let root = holder.path().to_path_buf();

        // Identity without authority: init establishes the durable
        // manifest, nothing else. Bind the audit root first so every
        // wedge decision below lands in the durable store.
        initialize_workspace(&root).expect("initialize workspace");
        init_global(&root).expect("bind durable audit root");
        let manifest = load_workspace_manifest(&root).expect("manifest");
        let workspace_id = manifest.workspace_id.clone();

        // The acting identity: a real agent-store record plus a session
        // bound to the canonical workspace identity.
        let agent_id = AgentId::new_checked("wedge-agent").expect("valid agent id");
        AgentStore::new(&root)
            .create(&Agent {
                id: agent_id.as_str().to_string(),
                name: "wedge-agent".into(),
                role: "worker".into(),
                status: AgentStatus::Active,
                enabled: true,
                created_at: chrono::Utc::now().to_rfc3339(),
            })
            .expect("create agent");
        let session_id = SessionId::new();
        let identity = SessionIdentity {
            session_id: session_id.clone(),
            agent_id: agent_id.clone(),
            workspace_id: workspace_id.clone(),
        };

        let grants = CapabilityGrantStore::new(&root);
        grants
            .create(&CapabilityGrant {
                id: "wedge-grant".into(),
                agent_id: agent_id.as_str().into(),
                permission: agent_workspace_hub::mcp::Permission::Process,
                scope: None,
                granted_at: chrono::Utc::now().to_rfc3339(),
                expires_at: None,
            })
            .expect("create grant");

        let dispatcher = McpDispatcher::new_async(root.clone())
            .await
            .expect("dispatcher")
            .with_trust_store(builtin_trust());
        let lifecycle = agent_workspace_hub::mcp::SessionLifecycle::default();
        lifecycle.set_caller(identity.clone());

        // Session establishment: initialize must succeed before any
        // tool call is accepted.
        let init = dispatcher
            .dispatch_with_lifecycle(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2025-06-18",
                        "capabilities": {},
                        "clientInfo": {"name": "trust-wedge-e2e", "version": "0.0.1"}
                    }
                })
                .to_string(),
                &lifecycle,
            )
            .await;
        assert!(
            matches!(init, agent_workspace_hub::mcp::DispatchResult::Response(_)),
            "initialize must respond"
        );

        let call_tool = |name: &str, arguments: Value, id: i64| {
            let dispatcher = &dispatcher;
            let lifecycle = &lifecycle;
            let name = name.to_string();
            async move {
                let result = dispatcher
                    .dispatch_with_lifecycle(
                        &json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "method": "tools/call",
                            "params": {"name": name, "arguments": arguments}
                        })
                        .to_string(),
                        lifecycle,
                    )
                    .await;
                match result {
                    agent_workspace_hub::mcp::DispatchResult::Response(response) => {
                        serde_json::to_value(&response).expect("serialize response")
                    }
                    agent_workspace_hub::mcp::DispatchResult::NoResponse => {
                        panic!("tools/call must produce a response")
                    }
                }
            }
        };

        // ---- Phase A: every layer allows. The canonical terminal
        // service really runs (cross-platform `git --version`).
        let allowed = call_tool(
            "terminal.run",
            json!({"program": "git", "args": ["--version"]}),
            2,
        )
        .await;
        assert!(allowed["error"].is_null(), "allow phase failed: {allowed}");
        let text = allowed["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default();
        assert!(text.contains("git version"), "expected git output: {text}");

        // ---- Phase B: revocation is live. The same session loses the
        // process capability; the next call fails closed at the
        // capability layer, before the trust/policy/service layers.
        grants.revoke("wedge-grant").expect("revoke grant");
        let capability_denied = call_tool(
            "terminal.run",
            json!({"program": "git", "args": ["--version"]}),
            3,
        )
        .await;
        let code = capability_denied["error"]["code"]
            .as_i64()
            .expect("error code");
        assert_eq!(code, CAPABILITY_DENIED_CODE, "got: {capability_denied}");

        // ---- Phase C: layering. With the capability restored, a
        // workspace policy deny rule for the program overrides the
        // capability allow — and the command never runs. `git init`
        // would leave a directory behind; denial must leave no trace.
        grants
            .create(&CapabilityGrant {
                id: "wedge-grant-2".into(),
                agent_id: agent_id.as_str().into(),
                permission: agent_workspace_hub::mcp::Permission::Process,
                scope: None,
                granted_at: chrono::Utc::now().to_rfc3339(),
                expires_at: None,
            })
            .expect("restore grant");
        let policy = PolicyStore::new(&root);
        policy
            .add(&PolicyRule {
                id: "wedge-deny-git".into(),
                tool: "terminal.run".into(),
                pattern: "git".into(),
                reason: Some("no git from this session".into()),
                created_at: chrono::Utc::now().to_rfc3339(),
            })
            .expect("add policy rule");
        let policy_denied = call_tool(
            "terminal.run",
            json!({"program": "git", "args": ["init", "wedge-side-effect-probe"]}),
            4,
        )
        .await;
        let code = policy_denied["error"]["code"].as_i64().expect("error code");
        assert_eq!(code, POLICY_DENIED_CODE, "got: {policy_denied}");
        assert!(
            !root.join("wedge-side-effect-probe").exists(),
            "a policy-denied command must not execute"
        );

        // ---- Phase E: the editing plane over the same bound session.
        // The dispatcher maps the bound caller onto the canonical
        // `AuthorizingPrincipal::agent` for the `*_as` service entry
        // points (TW-004): capability gates the Filesystem surface,
        // the transaction commits through the real filesystem
        // boundary, and the service-boundary audit event carries the
        // exact edit id + the session's identity.
        grants
            .create(&CapabilityGrant {
                id: "wedge-grant-fs".into(),
                agent_id: agent_id.as_str().into(),
                permission: agent_workspace_hub::mcp::Permission::Filesystem,
                scope: None,
                granted_at: chrono::Utc::now().to_rfc3339(),
                expires_at: None,
            })
            .expect("grant filesystem capability");
        let written = call_tool(
            "workspace.write_file",
            json!({
                "path": "wedge-doc.md", "content": "alpha beta gamma"
            }),
            5,
        )
        .await;
        assert!(written["error"].is_null(), "write failed: {written}");

        let replaced = call_tool(
            "filesystem.replace",
            json!({
                "path": "wedge-doc.md", "old": "beta", "new": "BETA"
            }),
            6,
        )
        .await;
        assert!(replaced["error"].is_null(), "replace failed: {replaced}");
        let edit = serde_json::from_str::<Value>(
            replaced["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or_default(),
        )
        .expect("EditResult JSON");
        let edit_id = edit["id"].as_str().expect("edit id").to_string();
        assert!(
            edit_id.starts_with("edit-"),
            "edit id must be the canonical generated shape: {edit_id}"
        );
        assert_eq!(edit["status"], "committed");
        assert_eq!(edit["match_count"], 1);
        assert_eq!(
            std::fs::read_to_string(root.join("wedge-doc.md")).expect("file on disk"),
            "alpha BETA gamma",
            "the edit must have mutated the real file"
        );

        // Rollback by exact id restores the pre-apply snapshot.
        let rolled_back = call_tool(
            "filesystem.rollback",
            json!({
                "edit_id": edit_id
            }),
            7,
        )
        .await;
        assert!(
            rolled_back["error"].is_null(),
            "rollback failed: {rolled_back}"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("wedge-doc.md")).expect("file on disk"),
            "alpha beta gamma",
            "rollback must restore the pre-apply snapshot"
        );

        // ---- Phase F: zero-side-effect denial on the EDIT plane. Revoke
        // the Filesystem grant live and attempt another replace: the
        // capability gate must deny BEFORE the service runs, leaving the
        // file bytes and the durable provenance store untouched. (The
        // isolated gate tests pin the gate's code; only a composed pass
        // can pin that no downstream state was created.)
        grants
            .revoke("wedge-grant-fs")
            .expect("revoke filesystem capability");
        let provenance_dir = root.join(".agent/provenance");
        let provenance_before: Vec<String> = std::fs::read_dir(&provenance_dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let edit_denied = call_tool(
            "filesystem.replace",
            json!({
                "path": "wedge-doc.md", "old": "gamma", "new": "GAMMA"
            }),
            8,
        )
        .await;
        let denied_code = edit_denied["error"]["code"]
            .as_i64()
            .expect("capability denial error code");
        assert_eq!(
            denied_code, CAPABILITY_DENIED_CODE,
            "revoked capability must deny the edit: {edit_denied}"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("wedge-doc.md")).expect("file on disk"),
            "alpha beta gamma",
            "a capability-denied edit must not execute"
        );
        let provenance_after: Vec<String> = std::fs::read_dir(&provenance_dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            provenance_before, provenance_after,
            "a denied edit must leave no provenance residue"
        );

        // ---- Phase D: restart durability. A fresh AuditLog over the
        // same root (what a restarted process sees) must reconstruct the
        // whole wedge story, newest first, with identities intact.
        let entries = AuditLog::open(&root)
            .expect("reopen durable audit")
            .recent(500);

        let find = |action: &str| {
            entries
                .iter()
                .find(|entry| {
                    entry.action == action
                        && entry.agent_id.as_deref() == Some(agent_id.as_str())
                        && entry.session_id.as_deref() == Some(session_id.as_str())
                        && entry.workspace_id.as_deref() == Some(workspace_id.as_str())
                })
                .unwrap_or_else(|| panic!("missing correlated {action} in durable audit"))
                .clone()
        };

        let invoke = entries
            .iter()
            .find(|entry| {
                entry.action == "tool_invoke"
                    && entry.subject == "terminal.run"
                    && entry.agent_id.as_deref() == Some(agent_id.as_str())
                    && entry.session_id.as_deref() == Some(session_id.as_str())
                    && entry.workspace_id.as_deref() == Some(workspace_id.as_str())
            })
            .expect("missing correlated terminal.run tool_invoke in durable audit")
            .clone();
        assert_eq!(invoke.kind, "allow");
        let capability = find("agent_capability_denied");
        assert_eq!(capability.kind, "deny");
        let policy_entry = find("policy_denied");
        assert_eq!(policy_entry.kind, "deny");
        // `audit_deny_as` carries the rule id as the subject and the
        // resource as the detail — the rule that denied is the actor's
        // evidence, the resource names what was attempted.
        assert_eq!(policy_entry.subject, "wedge-deny-git");
        assert_eq!(policy_entry.detail, "git");

        // Newest first. The two denial decisions are strictly ordered by
        // when they happened: the policy deny (phase C) is newer than
        // the capability deny (phase B). Note on `tool_invoke`: it is
        // the dispatcher's pre-execution attempt record, emitted after
        // the capability gate but BEFORE the per-tool resource gates —
        // the authoritative decisions are the capability/builtin/policy
        // allow|deny events, not `tool_invoke`'s presence.
        let position = |action: &str, detail: &str| {
            entries
                .iter()
                .position(|entry| {
                    entry.action == action
                        && entry.session_id.as_deref() == Some(session_id.as_str())
                        && (detail.is_empty() || entry.detail == detail)
                })
                .unwrap_or_else(|| panic!("session-scoped {action}/{detail} entry present"))
        };
        // The OLDEST correlated tool_invoke is the successful allow-phase
        // call (phase C also emits one before its policy denial).
        let oldest_tool_invoke = entries
            .iter()
            .rposition(|entry| {
                entry.action == "tool_invoke"
                    && entry.session_id.as_deref() == Some(session_id.as_str())
            })
            .expect("session-scoped tool_invoke present");
        assert!(
            position("policy_denied", "") < position("agent_capability_denied", "terminal.run")
        );
        // The successful invocation is the oldest of the three attempts:
        // both denials are strictly newer than it.
        assert!(position("agent_capability_denied", "terminal.run") < oldest_tool_invoke);
        // Phase F's edit-plane capability denial is the NEWEST decision of
        // the composed story: newer than the policy deny (phase C) and
        // than the terminal capability deny (phase B).
        assert!(
            position("agent_capability_denied", "filesystem.replace")
                < position("policy_denied", "")
        );

        // The editing plane's service-boundary outcome events correlate
        // to the exact edit id AND to the bound session identity
        // (TW-004/AWE-013): `filesystem.replace` and `filesystem.rollback`
        // each carry the edit id and this session's agent identity.
        let edit_events: Vec<_> = entries
            .iter()
            .filter(|entry| {
                entry.edit_id.as_deref() == Some(edit_id.as_str())
                    && entry.agent_id.as_deref() == Some(agent_id.as_str())
                    && entry.session_id.as_deref() == Some(session_id.as_str())
            })
            .collect();
        assert!(
            edit_events.len() >= 2,
            "replace + rollback must be correlated to the edit id and session: {entries:?}"
        );
        assert!(edit_events.iter().all(|entry| entry.kind == "allow"));
        assert!(
            edit_events
                .iter()
                .any(|entry| entry.action == "filesystem.replace"),
            "replace outcome event present: {edit_events:?}"
        );
        assert!(
            edit_events
                .iter()
                .any(|entry| entry.action == "filesystem.rollback"),
            "rollback outcome event present: {edit_events:?}"
        );

        // Un-attributed decisions stay un-attributed: the coarse
        // builtin gate is workspace-level, not agent-level, and its
        // allow event must never invent an identity.
        let builtin = entries
            .iter()
            .find(|entry| entry.action == "builtin_tool_allowed" && entry.subject == "terminal.run")
            .expect("builtin allow audited");
        assert!(
            builtin.agent_id.is_none(),
            "builtin gate has no agent identity"
        );

        // ---- Phase G: worktree isolation closes the §8 chain.
        // The bound session gets a REAL managed worktree (actual
        // `git worktree` checkout on disk, §6), and the isolation
        // boundary holds: the bound session's effective root is the
        // worktree, a foreign session's is the workspace root, and a
        // non-owning session cannot remove what it does not own.
        let run_git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .expect("spawn git");
            assert!(
                out.status.success(),
                "git {:?}: {}{}",
                args,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run_git(&["init"]);
        run_git(&["config", "user.email", "wedge@invalid"]);
        run_git(&["config", "user.name", "wedge"]);
        std::fs::write(root.join("WEDGE-README.md"), "base\n").unwrap();
        run_git(&["add", "."]);
        run_git(&["commit", "-m", "wedge base"]);

        let worktrees = agent_workspace_hub::services::worktree::WorktreeStore::new(&root);
        let record = worktrees
            .create(
                workspace_id.as_str(),
                agent_id.as_str(),
                session_id.as_str(),
                None,
                None,
            )
            .await
            .expect("worktree for the bound session");
        assert_eq!(record.workspace_id, workspace_id.as_str());
        assert_eq!(record.agent_id, agent_id.as_str());
        assert_eq!(record.session_id, session_id.as_str());
        assert!(matches!(
            record.state,
            agent_workspace_hub::services::worktree::WorktreeState::Active
        ));
        let checkout = root.join(&record.path);
        assert!(
            checkout.join("WEDGE-README.md").exists(),
            "checkout shares the repository content"
        );

        // Session-root isolation (§11): the bound session resolves to
        // its worktree; every other session still resolves to the
        // workspace root — callers construct existing services against
        // this root, so the boundary IS filesystem isolation.
        let bound = worktrees
            .resolve_effective_root(session_id.as_str())
            .expect("bound session root");
        let foreign = worktrees
            .resolve_effective_root("sess-foreign")
            .expect("foreign session root");
        assert_eq!(bound, checkout, "bound session sees its worktree");
        assert_eq!(foreign, root, "foreign session sees the workspace root");

        // Ownership invariant (§10): a non-owning session's removal is
        // denied and the checkout survives untouched.
        match worktrees
            .remove(record.worktree_id.as_str(), "sess-foreign")
            .await
        {
            Err(agent_workspace_hub::services::worktree::WorktreeError::Ownership(id)) => {
                assert_eq!(id, record.worktree_id);
            }
            other => panic!("foreign-session remove must be Ownership-denied, got {other:?}"),
        }
        assert!(checkout.is_dir(), "denied removal left the checkout intact");

        // The owning session removes its own worktree.
        let removed = worktrees
            .remove(record.worktree_id.as_str(), session_id.as_str())
            .await
            .expect("owner removes its worktree");
        assert!(matches!(
            removed.state,
            agent_workspace_hub::services::worktree::WorktreeState::Removed
        ));
        assert!(!checkout.exists(), "checkout gone after owner removal");
    });
}
