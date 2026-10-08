//! TP05 (§4-§29): the capability & policy authorization boundary — the
//! gaps the per-layer suites leave open, proven at real enforcement
//! boundaries with independently observable resource state.
//!
//! Sibling suites already pin the layers in isolation:
//! * `mcp_builtin_tool_gate.rs` — the SEC-001 `awh.builtin` trust floor;
//! * `mcp_policy_gate.rs` — workspace deny-rule matching + CLI round-trip;
//! * `mcp_agent_routes.rs` — route resolution, session binding, discovery
//!   filtering, scoped/expired grants, concurrent agents;
//! * `acceptance_edit_gates.rs` — the edit-plane capability gate;
//! * `trust_wedge_e2e.rs` — the composed wedge + durable audit.
//!
//! This suite pins the CROSS-SURFACE and EDGE properties none of those
//! cover:
//!
//! * **CLI↔enforcement coherence (§8, §18)** — a grant written by the real
//!   `awh agent grant` binary is the same grant the MCP capability gate
//!   reads; `awh agent revoke` removes live authority across a fresh
//!   dispatcher (the restart path) with zero side effects. `agent_cli.rs`
//!   proves the CLI persists records; `mcp_agent_routes.rs` proves the
//!   gate enforces records — only this suite proves they are the SAME
//!   records.
//! * **Least privilege at the gate (§7)** — a Process grant never
//!   satisfies a Filesystem tool and vice versa; capability categories
//!   never imply one another.
//! * **Fail-closed shapes at enforcement (§13, §27)** — a malformed
//!   `expires_at` and a corrupt grant store deny (never silently allow)
//!   at `tools/call`, and a corrupt store collapses `tools/list` to the
//!   empty catalog.
//! * **Policy precision on the wire (§11)** — `src/foo` denies
//!   `src/foo/x` but never `src/foobar`; an empty pattern denies nothing;
//!   a traversal-shaped pattern never matches a normalized escape.
//! * **Concurrency (§19)** — revoking one agent's grant never disturbs a
//!   concurrent other-agent decision.
//!
//! Every denial asserts the resource oracle independently (marker file,
//! file bytes, request counter) — a deny plus an unchanged resource is
//! the strongest authorization evidence (§28).

use agent_workspace_hub::core::agents::AgentStore;
use agent_workspace_hub::core::capability_grants::CapabilityGrantStore;
use agent_workspace_hub::core::identity::SessionId;
use agent_workspace_hub::core::policy::PolicyStore;
use agent_workspace_hub::mcp::dispatcher::CAPABILITY_DENIED_CODE;
use agent_workspace_hub::mcp::permissions::Permission;
use agent_workspace_hub::mcp::{McpDispatcher, SessionLifecycle, POLICY_DENIED_CODE};
use agent_workspace_hub::models::agent::{Agent, AgentStatus};
use agent_workspace_hub::models::policy_rule::PolicyRule;
use agent_workspace_hub::services::init::{initialize_workspace, load_workspace_manifest};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

#[path = "common/mod.rs"]
mod common;

/// The full built-in trust record `awh mcp trust awh.builtin --network
/// --process --filesystem` writes — every test here needs the coarse
/// SEC-001 floor open so the *capability/policy* layers under test are
/// the deciding layers (mirrors `tests/trust_wedge_e2e.rs::builtin_trust`).
fn builtin_trust() -> agent_workspace_hub::mcp::PersistentTrustStore {
    use agent_workspace_hub::mcp::{
        McpPermissions, PersistentTrustStore, TrustLevel, TrustStore, BUILTIN_TOOL_TRUST_ID,
    };
    let mut store = TrustStore::default();
    store
        .approve(
            BUILTIN_TOOL_TRUST_ID,
            TrustLevel::Reviewed,
            McpPermissions {
                network: true,
                process: true,
                filesystem: vec![BUILTIN_TOOL_TRUST_ID.to_string()],
                ..McpPermissions::default()
            },
            "local".to_string(),
        )
        .expect("valid approval");
    PersistentTrustStore::from_store(&store)
}

/// A real initialized workspace (manifest on disk — the route/identity
/// resolution requires it) with the given agents registered Active.
struct Ws {
    root: PathBuf,
    _dir: std::mem::ManuallyDrop<TempDir>,
}

impl Ws {
    fn new(agents: &[&str]) -> Self {
        let dir = tempdir().expect("tempdir");
        // Leak: the process-wide durable audit store keeps writing into
        // whichever root was bound first, so tempdirs holding audited
        // workspaces must outlive the test (same pattern as the http and
        // wedge suites).
        let holder = std::mem::ManuallyDrop::new(dir);
        let root = holder.path().to_path_buf();
        initialize_workspace(&root).expect("initialize workspace");
        let store = AgentStore::new(&root);
        for id in agents {
            store
                .create(&Agent {
                    id: (*id).into(),
                    name: (*id).into(),
                    role: "test".into(),
                    status: AgentStatus::Active,
                    enabled: true,
                    created_at: chrono::Utc::now().to_rfc3339(),
                })
                .expect("create agent");
        }
        Self { root, _dir: holder }
    }

    fn workspace_id(&self) -> String {
        load_workspace_manifest(&self.root)
            .expect("manifest")
            .workspace_id
            .as_str()
            .to_string()
    }

    /// A dispatcher over this workspace with the full built-in trust floor
    /// open, so the capability/policy layers are the deciding layers.
    async fn dispatcher(&self) -> McpDispatcher {
        McpDispatcher::new_async(self.root.clone())
            .await
            .expect("dispatcher")
            .with_trust_store(builtin_trust())
    }

    /// A session bound to `agent` exactly the way `/{agent}/sse` would:
    /// trusted route resolution (active + enabled agent, workspace
    /// manifest) → canonical `SessionIdentity` → lifecycle caller slot
    /// (never a client-supplied name).
    async fn bound_session(&self, dispatcher: &McpDispatcher, agent: &str) -> SessionLifecycle {
        let (caller, record) =
            agent_workspace_hub::mcp::agent_route::resolve_route_agent(&self.root, agent)
                .expect("resolve agent");
        assert_eq!(record.id, agent, "route resolves the requested agent");
        let identity = caller.to_session_identity(SessionId::new());
        assert_eq!(identity.workspace_id.as_str(), self.workspace_id());
        let lifecycle = SessionLifecycle::default();
        lifecycle.set_caller(identity);
        // Initialize the session like a real client would.
        let init = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "tp05-capability", "version": "0.0.0"}
            }
        });
        match dispatcher
            .dispatch_with_lifecycle(&init.to_string(), &lifecycle)
            .await
        {
            agent_workspace_hub::mcp::DispatchResult::Response(_) => {}
            agent_workspace_hub::mcp::DispatchResult::NoResponse => {
                panic!("initialize must respond")
            }
        }
        lifecycle
    }
}

/// Calls a tool on a bound lifecycle, returning the raw JSON-RPC value.
async fn call(
    dispatcher: &McpDispatcher,
    lifecycle: &SessionLifecycle,
    name: &str,
    args: Value,
) -> Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": name, "arguments": args},
    });
    match dispatcher
        .dispatch_with_lifecycle(&request.to_string(), lifecycle)
        .await
    {
        agent_workspace_hub::mcp::DispatchResult::Response(resp) => {
            serde_json::to_value(&resp).expect("serialize response")
        }
        agent_workspace_hub::mcp::DispatchResult::NoResponse => {
            panic!("{name} must produce a response")
        }
    }
}

/// The error object of a denial.
fn error_of(response: &Value) -> Value {
    assert!(
        response["error"].is_object(),
        "expected an error, got: {response}"
    );
    response["error"].clone()
}

/// Writes a grant through the canonical store (in-process).
fn grant(
    root: &Path,
    id: &str,
    agent: &str,
    permission: Permission,
    scope: Option<&str>,
    expires_at: Option<&str>,
) {
    CapabilityGrantStore::new(root)
        .create(&agent_workspace_hub::models::CapabilityGrant {
            id: id.into(),
            agent_id: agent.into(),
            permission,
            scope: scope.map(str::to_string),
            granted_at: chrono::Utc::now().to_rfc3339(),
            expires_at: expires_at.map(str::to_string),
        })
        .expect("create grant");
}

/// Adds a policy deny rule through the canonical store.
fn deny_rule(root: &Path, id: &str, tool: &str, pattern: &str) {
    PolicyStore::new(root)
        .add(&PolicyRule {
            id: id.into(),
            tool: tool.into(),
            pattern: pattern.into(),
            reason: Some("tp05".into()),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .expect("add policy rule");
}

/// A real `awh` child in `dir` (hermetic env), returning (ok, stdout, stderr).
fn awh(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let output = common::sanitized_command(dir, args)
        .output()
        .expect("spawn awh");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

// ---------------------------------------------------------------------------
// §8/§18: CLI ↔ enforcement coherence — the grant the real binary writes
// is the grant the gate reads, and the revoke the binary performs is the
// authority the gate loses (across a fresh dispatcher = the restart path).
// ---------------------------------------------------------------------------

/// A grant created by the REAL `awh agent grant` binary authorizes the
/// MCP plane, with its `--scope` narrowing intact; the same binary's
/// `agent revoke` removes that authority for a freshly-constructed
/// dispatcher (what a restarted process reads), and the revoked call
/// leaves no trace on the resource.
#[tokio::test]
async fn cli_created_grant_authorizes_and_cli_revoke_removes_across_restart() {
    let ws = Ws::new(&["alpha"]);
    std::fs::create_dir_all(ws.root.join("src")).expect("src dir");
    std::fs::write(ws.root.join("src/target.txt"), "before\n").expect("fixture");

    // Grant THROUGH THE REAL BINARY, with a scope.
    let (ok, out, err) = awh(
        &ws.root,
        &[
            "agent",
            "grant",
            "alpha",
            "--permission",
            "filesystem",
            "--scope",
            "src",
        ],
    );
    assert!(ok, "agent grant failed: {err} (stdout: {out})");
    assert!(out.contains("alpha"), "grant output names the agent: {out}");

    // The enforcement boundary honors it: in-scope write succeeds and
    // really mutates the file.
    let dispatcher = ws.dispatcher().await;
    let session = ws.bound_session(&dispatcher, "alpha").await;
    let written = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "src/target.txt", "content": "after\n"}),
    )
    .await;
    assert!(
        written["error"].is_null(),
        "CLI-created grant must authorize the gate: {written}"
    );
    assert_eq!(
        std::fs::read_to_string(ws.root.join("src/target.txt")).unwrap(),
        "after\n"
    );

    // Scope survived the CLI round-trip: out-of-scope is still denied.
    let denied = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "outside.txt", "content": "x"}),
    )
    .await;
    assert_eq!(
        error_of(&denied)["code"],
        json!(CAPABILITY_DENIED_CODE),
        "scope must narrow through the CLI path: {denied}"
    );
    assert!(!ws.root.join("outside.txt").exists());

    // Revoke THROUGH THE REAL BINARY: the derived grant id
    // (`{agent}-{permission}`) the grant command printed, exit 0,
    // durable effect.
    let grant_id = "alpha-filesystem";
    let (ok, out, err) = awh(&ws.root, &["agent", "revoke", grant_id]);
    assert!(ok, "agent revoke failed: {err} (stdout: {out})");
    assert!(out.contains("revoked grant"), "revoke output: {out}");

    // A FRESH dispatcher over the same root (the restart path) loses
    // the authority: the identical call is capability-denied and the
    // file is untouched.
    let fresh = ws.dispatcher().await;
    let fresh_session = ws.bound_session(&fresh, "alpha").await;
    std::fs::write(ws.root.join("src/target.txt"), "revocation-probe\n").expect("reset fixture");
    let denied = call(
        &fresh,
        &fresh_session,
        "workspace.write_file",
        json!({"path": "src/target.txt", "content": "must-not-land\n"}),
    )
    .await;
    assert_eq!(
        error_of(&denied)["code"],
        json!(CAPABILITY_DENIED_CODE),
        "CLI revoke must remove authority for a fresh process: {denied}"
    );
    assert_eq!(
        std::fs::read_to_string(ws.root.join("src/target.txt")).unwrap(),
        "revocation-probe\n",
        "a revoked write must leave the file untouched"
    );
}

/// CLI rejection shapes (§22): granting a nonexistent permission or
/// granting an unknown agent both fail closed at the CLI with a
/// non-zero exit and a stable message — and the failed mutations leave
/// the capability store unchanged. Revoking a grant that never existed
/// is the pinned idempotent shape (`agent_cli.rs`): exit 0 with an
/// explicit "grant not found" report, never a silent success.
#[test]
fn cli_grant_rejects_bad_input_without_mutating_the_store() {
    let ws = Ws::new(&["alpha"]);

    // Invalid permission vocabulary: non-zero exit, nothing persisted.
    let (ok, _out, err) = awh(
        &ws.root,
        &["agent", "grant", "alpha", "--permission", "teleportation"],
    );
    assert!(!ok, "unknown permission must fail");
    assert!(
        err.contains("invalid permission \"teleportation\""),
        "error must name the bad value: {err}"
    );

    // Granting an agent that does not exist: non-zero exit.
    let (ok, _out, err) = awh(
        &ws.root,
        &["agent", "grant", "ghost", "--permission", "filesystem"],
    );
    assert!(!ok, "unknown agent must fail");
    assert!(
        err.contains("agent not found: ghost"),
        "error must name the agent: {err}"
    );

    // No side-effect residue: the store is empty for both agents.
    for agent in ["alpha", "ghost"] {
        let grants = CapabilityGrantStore::new(&ws.root)
            .list_for_agent(agent)
            .expect("list");
        assert!(
            grants.is_empty(),
            "failed grant must persist nothing for {agent}"
        );
    }

    // Revoking a grant that never existed: the pinned idempotent shape —
    // exit 0, explicit "grant not found" on stdout, and the store is
    // still empty (no phantom record created either).
    let (ok, out, _err) = awh(&ws.root, &["agent", "revoke", "alpha-filesystem"]);
    assert!(ok, "revoke-missing is the pinned idempotent shape");
    assert!(
        out.contains("grant not found: alpha-filesystem"),
        "must explicitly report the missing grant: {out}"
    );
    assert!(
        CapabilityGrantStore::new(&ws.root)
            .list_for_agent("alpha")
            .expect("list")
            .is_empty(),
        "a ghost revoke must not create or destroy anything"
    );
}

// ---------------------------------------------------------------------------
// §7: least privilege at the gate — capability categories never imply
// one another.
// ---------------------------------------------------------------------------

/// A Process-only grant satisfies no Filesystem tool, and a
/// Filesystem-only grant satisfies no Process tool. The negative
/// direction matters most: a real grant for ONE capability must never
/// leak authority into another category's tools.
#[tokio::test]
async fn capability_categories_never_imply_one_another() {
    let ws = Ws::new(&["proc-only", "fs-only", "none"]);
    grant(
        &ws.root,
        "g-proc",
        "proc-only",
        Permission::Process,
        None,
        None,
    );
    grant(
        &ws.root,
        "g-fs",
        "fs-only",
        Permission::Filesystem,
        None,
        None,
    );
    std::fs::write(ws.root.join("in-root.txt"), "orig\n").expect("fixture");

    let dispatcher = ws.dispatcher().await;

    // Process grant ≠ Filesystem authority: the write is denied and the
    // file is untouched.
    let proc = ws.bound_session(&dispatcher, "proc-only").await;
    let denied = call(
        &dispatcher,
        &proc,
        "workspace.write_file",
        json!({"path": "in-root.txt", "content": "leaked\n"}),
    )
    .await;
    assert_eq!(
        error_of(&denied)["code"],
        json!(CAPABILITY_DENIED_CODE),
        "Process grant must not enable Filesystem tools: {denied}"
    );
    assert_eq!(
        std::fs::read_to_string(ws.root.join("in-root.txt")).unwrap(),
        "orig\n",
        "cross-category denial leaves bytes untouched"
    );

    // Filesystem grant ≠ Process authority: terminal.run is denied even
    // though the coarse trust floor allows it, and the command never
    // executes (no marker).
    let fs = ws.bound_session(&dispatcher, "fs-only").await;
    #[cfg(unix)]
    let args = json!({"program": "touch", "args": ["process-marker.txt"]});
    #[cfg(windows)]
    let args = json!({"program": "cmd", "args": ["/C", "type nul > process-marker.txt"]});
    let denied = call(&dispatcher, &fs, "terminal.run", args).await;
    assert_eq!(
        error_of(&denied)["code"],
        json!(CAPABILITY_DENIED_CODE),
        "Filesystem grant must not enable Process tools: {denied}"
    );
    assert!(
        !ws.root.join("process-marker.txt").exists(),
        "cross-category denial never executes the command"
    );

    // The positive control: the Process holder CAN run the identical
    // command — proving the denial above was the capability gate, not
    // the environment.
    #[cfg(unix)]
    let args = json!({"program": "touch", "args": ["process-marker.txt"]});
    #[cfg(windows)]
    let args = json!({"program": "cmd", "args": ["/C", "type nul > process-marker.txt"]});
    let allowed = call(&dispatcher, &proc, "terminal.run", args).await;
    assert!(
        allowed["error"].is_null(),
        "Process grant must enable terminal.run: {allowed}"
    );
    assert!(
        ws.root.join("process-marker.txt").exists(),
        "the positive control really executed"
    );

    // tools/list mirrors the same least-privilege truth (§16): the
    // Process holder does not even SEE Filesystem tools, and vice versa.
    let list = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
    });
    let listed = match dispatcher
        .dispatch_with_lifecycle(&list.to_string(), &proc)
        .await
    {
        agent_workspace_hub::mcp::DispatchResult::Response(resp) => {
            serde_json::to_value(&resp).expect("serialize")
        }
        agent_workspace_hub::mcp::DispatchResult::NoResponse => panic!("tools/list must respond"),
    };
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(
        !names.contains(&"workspace.write_file"),
        "Process holder must not see Filesystem tools: {names:?}"
    );
    assert!(
        names.contains(&"terminal.run"),
        "Process holder sees its own tools: {names:?}"
    );
}

// ---------------------------------------------------------------------------
// §13/§27: fail-closed shapes at enforcement — malformed expiry and a
// corrupt grant store deny (never silently allow).
// ---------------------------------------------------------------------------

/// A malformed `expires_at` (unparseable timestamp) denies at the
/// enforcement boundary — a broken grant never becomes an eternal one.
#[tokio::test]
async fn malformed_expiry_fails_closed_at_enforcement() {
    let ws = Ws::new(&["alpha"]);
    grant(
        &ws.root,
        "g-bad",
        "alpha",
        Permission::Filesystem,
        None,
        Some("not-a-timestamp"),
    );
    std::fs::write(ws.root.join("probe.txt"), "orig\n").expect("fixture");

    let dispatcher = ws.dispatcher().await;
    let session = ws.bound_session(&dispatcher, "alpha").await;
    let denied = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "probe.txt", "content": "leaked\n"}),
    )
    .await;
    assert_eq!(
        error_of(&denied)["code"],
        json!(CAPABILITY_DENIED_CODE),
        "malformed expiry must deny: {denied}"
    );
    assert_eq!(
        std::fs::read_to_string(ws.root.join("probe.txt")).unwrap(),
        "orig\n",
        "malformed-expiry denial leaves bytes untouched"
    );
}

/// A corrupt grant store (garbage JSON in the capabilities dir) fails
/// closed at BOTH enforcement surfaces: `tools/call` returns an internal
/// error (never a false allow, never a panic), and `tools/list` collapses
/// to the empty static catalog — announce nothing you cannot authorize.
#[tokio::test]
async fn corrupt_grant_store_fails_closed_at_call_and_listing() {
    let ws = Ws::new(&["alpha"]);
    grant(
        &ws.root,
        "g-real",
        "alpha",
        Permission::Filesystem,
        None,
        None,
    );

    // Corrupt the store: one unreadable grant file alongside the valid one.
    let caps = ws.root.join(".agent").join("capabilities");
    std::fs::write(caps.join("corrupt.json"), "{not json").expect("corrupt grant");

    let dispatcher = ws.dispatcher().await;
    let session = ws.bound_session(&dispatcher, "alpha").await;

    // tools/call: denied (fail closed), not allowed, not a panic. The
    // code is the dispatcher's internal error — an unreadable store is
    // an infrastructure failure, which the capability layer surfaces as
    // a denial, never as an allow.
    let denied = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "probe.txt", "content": "leaked\n"}),
    )
    .await;
    assert!(
        denied["error"].is_object(),
        "corrupt store must never allow: {denied}"
    );
    let code = error_of(&denied)["code"].as_i64().expect("error code");
    assert_ne!(
        code, 0,
        "the denial must carry a JSON-RPC error code, got: {denied}"
    );
    assert!(
        !ws.root.join("probe.txt").exists(),
        "corrupt-store denial never wrote the file"
    );

    // tools/list for the same caller: empty static catalog (dynamic
    // provider tools may remain; none are configured here).
    let list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"});
    let listed = match dispatcher
        .dispatch_with_lifecycle(&list.to_string(), &session)
        .await
    {
        agent_workspace_hub::mcp::DispatchResult::Response(resp) => {
            serde_json::to_value(&resp).expect("serialize")
        }
        agent_workspace_hub::mcp::DispatchResult::NoResponse => panic!("tools/list must respond"),
    };
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(
        names
            .iter()
            .all(|n| { agent_workspace_hub::mcp::tool_registry::registry_lookup(n).is_none() }),
        "a corrupt store must collapse the static catalog: {names:?}"
    );
    assert!(
        !names.contains(&"workspace.write_file"),
        "specifically, the Filesystem tool is not advertised: {names:?}"
    );
}

// ---------------------------------------------------------------------------
// §11: policy precision on the wire — component boundaries, not substring
// or prefix-byte matching, over unrelated resources.
// ---------------------------------------------------------------------------

/// A deny pattern matches whole path COMPONENTS: `src/foo` denies
/// `src/foo/x.txt` but never `src/foobar.txt`. An empty pattern denies
/// nothing (a malformed rule must not deny the world). A
/// traversal-shaped pattern never matches a normalized legitimate path.
#[tokio::test]
async fn policy_path_patterns_match_components_not_substrings() {
    let ws = Ws::new(&["alpha"]);
    grant(
        &ws.root,
        "g-fs",
        "alpha",
        Permission::Filesystem,
        None,
        None,
    );
    deny_rule(&ws.root, "deny-foo", "workspace.write_file", "src/foo");

    let dispatcher = ws.dispatcher().await;
    let session = ws.bound_session(&dispatcher, "alpha").await;

    // Inside the denied component prefix: -32004, nothing written.
    let denied = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "src/foo/x.txt", "content": "x"}),
    )
    .await;
    assert_eq!(
        error_of(&denied)["code"],
        json!(POLICY_DENIED_CODE),
        "in-prefix resource must be denied: {denied}"
    );
    assert!(!ws.root.join("src/foo/x.txt").exists());

    // The sibling whose name merely SHARES the byte prefix: allowed.
    // `src/foobar.txt` is not under `src/foo/` — substring/prefix-byte
    // matching would wrongly deny it.
    let allowed = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "src/foobar.txt", "content": "legitimate"}),
    )
    .await;
    assert!(
        allowed["error"].is_null(),
        "src/foobar.txt must not be denied by the src/foo rule: {allowed}"
    );
    assert_eq!(
        std::fs::read_to_string(ws.root.join("src/foobar.txt")).unwrap(),
        "legitimate"
    );

    // The pattern's exact resource with no trailing component: also
    // denied (resource == pattern is in scope).
    let denied_exact = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "src/foo", "content": "x"}),
    )
    .await;
    assert_eq!(
        error_of(&denied_exact)["code"],
        json!(POLICY_DENIED_CODE),
        "the exact pattern resource is denied: {denied_exact}"
    );

    // An empty pattern is non-matching: adding it changes nothing.
    deny_rule(&ws.root, "deny-empty", "workspace.write_file", "");
    let still_allowed = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "elsewhere.txt", "content": "ok"}),
    )
    .await;
    assert!(
        still_allowed["error"].is_null(),
        "an empty pattern must deny nothing: {still_allowed}"
    );
    assert!(ws.root.join("elsewhere.txt").exists());
}

/// A traversal-shaped pattern (`src/../etc`) never matches a normalized
/// legitimate path — the deny side is as traversal-safe as the
/// capability side, and escape attempts stay allowed-but-confined to
/// the workspace (the FilesService traversal guard owns the rest).
#[tokio::test]
async fn traversal_shaped_policy_pattern_matches_nothing_legitimate() {
    let ws = Ws::new(&["alpha"]);
    grant(
        &ws.root,
        "g-fs",
        "alpha",
        Permission::Filesystem,
        None,
        None,
    );
    // A pattern carrying parent components. Written directly through the
    // store (the CLI accepts arbitrary patterns; the matcher's job is to
    // never let this shape deny a normalized resource).
    PolicyStore::new(&ws.root)
        .add(&PolicyRule {
            id: "deny-traversal".into(),
            tool: "workspace.write_file".into(),
            pattern: "src/../etc".into(),
            reason: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .expect("add rule");

    let dispatcher = ws.dispatcher().await;
    let session = ws.bound_session(&dispatcher, "alpha").await;

    // A legitimate in-workspace path is NOT denied by the
    // traversal-shaped pattern (it is not a policy match; the actual
    // write proceeds through the service's own traversal guards).
    let outcome = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "docs/readme.md", "content": "ok"}),
    )
    .await;
    assert!(
        outcome["error"].is_null() || error_of(&outcome)["code"] != json!(POLICY_DENIED_CODE),
        "traversal-shaped pattern must not policy-deny a legitimate path: {outcome}"
    );
}

// ---------------------------------------------------------------------------
// §19: concurrency — revoking one agent's grant never disturbs a
// concurrent other-agent decision.
// ---------------------------------------------------------------------------

/// Two agents hold grants; alpha's is revoked before the concurrent
/// dispatch. Both calls run concurrently (tokio::join): alpha is
/// capability-denied, beta succeeds, and beta's success is unaffected by
/// alpha's revocation (independent decisions from independent grants).
#[tokio::test]
async fn revoking_one_agents_grant_never_disturbs_a_concurrent_other_agent() {
    let ws = Ws::new(&["alpha", "beta"]);
    grant(
        &ws.root,
        "g-alpha",
        "alpha",
        Permission::Filesystem,
        None,
        None,
    );
    grant(
        &ws.root,
        "g-beta",
        "beta",
        Permission::Filesystem,
        None,
        None,
    );

    let dispatcher = ws.dispatcher().await;
    let alpha = ws.bound_session(&dispatcher, "alpha").await;
    let beta = ws.bound_session(&dispatcher, "beta").await;

    // Revoke alpha's authority, then dispatch both concurrently.
    CapabilityGrantStore::new(&ws.root)
        .revoke("g-alpha")
        .expect("revoke alpha");

    let alpha_call = call(
        &dispatcher,
        &alpha,
        "workspace.write_file",
        json!({"path": "alpha.txt", "content": "a"}),
    );
    let beta_call = call(
        &dispatcher,
        &beta,
        "workspace.write_file",
        json!({"path": "beta.txt", "content": "b"}),
    );
    let (alpha_outcome, beta_outcome) = tokio::join!(alpha_call, beta_call);

    assert_eq!(
        error_of(&alpha_outcome)["code"],
        json!(CAPABILITY_DENIED_CODE),
        "revoked agent denied: {alpha_outcome}"
    );
    assert!(
        beta_outcome["error"].is_null(),
        "other agent's grant is untouched by the revocation: {beta_outcome}"
    );
    assert!(
        !ws.root.join("alpha.txt").exists(),
        "denied concurrent write left no trace"
    );
    assert_eq!(
        std::fs::read_to_string(ws.root.join("beta.txt")).unwrap(),
        "b",
        "allowed concurrent write really landed"
    );
}

// ---------------------------------------------------------------------------
// §12: precedence — the capability gate runs BEFORE the policy layer, so
// a caller with NO capability is denied with the CAPABILITY code even
// when a workspace deny rule also matches the resource. (The inverse
// composition — capability allow + matching deny rule → POLICY code —
// is pinned by mcp_agent_routes::policy_deny_overrides_capability_
// allowance; this is the missing cell.)
// ---------------------------------------------------------------------------

/// No capability + matching policy deny rule → the call is
/// CAPABILITY-denied (-32005): the deny rule is never even consulted,
/// and the error names the missing capability, not the policy rule.
/// Both gates deny, but the FIRST gate's verdict is the surfaced one.
#[tokio::test]
async fn missing_capability_wins_over_matching_policy_deny_rule() {
    let ws = Ws::new(&["alpha"]);
    // NO grant for alpha.
    deny_rule(&ws.root, "deny-writes", "workspace.write_file", "src");

    let dispatcher = ws.dispatcher().await;
    let session = ws.bound_session(&dispatcher, "alpha").await;
    let denied = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "src/foo.txt", "content": "x"}),
    )
    .await;
    let error = error_of(&denied);
    assert_eq!(
        error["code"],
        json!(CAPABILITY_DENIED_CODE),
        "capability denial must surface, not the policy code: {denied}"
    );
    assert_ne!(
        error["code"],
        json!(POLICY_DENIED_CODE),
        "the policy layer must not be the surfaced verdict for a caller with no capability"
    );
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("alpha"),
        "the denial names the caller: {message}"
    );
    assert!(
        !ws.root.join("src/foo.txt").exists(),
        "doubly-denied call left no trace"
    );
}

/// A CLI-shaped grant whose expiry is in the past never authorizes;
/// the same grant shape with a far-future expiry does. (Exactly-at-now
/// is a wall-clock race by construction — the recent-past/far-future
/// pair is the deterministic statement of the boundary.)
#[tokio::test]
async fn expired_cli_shaped_grant_never_authorizes() {
    let ws = Ws::new(&["stale", "fresh"]);
    // Past expiry: RFC 3339 like the CLI would write.
    grant(
        &ws.root,
        "g-stale",
        "stale",
        Permission::Filesystem,
        None,
        Some("2001-01-01T00:00:00+00:00"),
    );
    // Far future.
    grant(
        &ws.root,
        "g-fresh",
        "fresh",
        Permission::Filesystem,
        None,
        Some("2099-01-01T00:00:00+00:00"),
    );

    let dispatcher = ws.dispatcher().await;

    let stale = ws.bound_session(&dispatcher, "stale").await;
    let denied = call(
        &dispatcher,
        &stale,
        "workspace.write_file",
        json!({"path": "stale.txt", "content": "x"}),
    )
    .await;
    assert_eq!(
        error_of(&denied)["code"],
        json!(CAPABILITY_DENIED_CODE),
        "expired grant denies: {denied}"
    );
    assert!(!ws.root.join("stale.txt").exists());

    let fresh = ws.bound_session(&dispatcher, "fresh").await;
    let allowed = call(
        &dispatcher,
        &fresh,
        "workspace.write_file",
        json!({"path": "fresh.txt", "content": "x"}),
    )
    .await;
    assert!(
        allowed["error"].is_null(),
        "future expiry allows: {allowed}"
    );
    assert!(ws.root.join("fresh.txt").exists());
}

// ---------------------------------------------------------------------------
// Workflow G, live: a grant expiring seconds ahead authorizes NOW, and
// the SAME grant stops authorizing once the instant passes — the
// enforcement boundary reads wall time on every call, so expiry takes
// effect without restart or re-grant. (The comparison is
// `expiry <= now`, so the instant itself is already expired; the
// before/after pair is the deterministic statement. The bounded wait is
// the prompt-sanctioned way to observe a real temporal transition.)
// ---------------------------------------------------------------------------

/// A grant expiring two seconds ahead allows the first call; 2.5s later
/// the identical call from the SAME session is capability-denied and
/// the resource is untouched.
#[tokio::test]
async fn a_grant_expires_live_between_two_calls() {
    let ws = Ws::new(&["temporal"]);
    grant(
        &ws.root,
        "g-temporal",
        "temporal",
        Permission::Filesystem,
        None,
        // Now + 2s in the same RFC 3339 form the CLI and store write.
        Some(&format!(
            "{}+00:00",
            (chrono::Utc::now() + chrono::Duration::seconds(2)).format("%Y-%m-%dT%H:%M:%S")
        )),
    );

    let dispatcher = ws.dispatcher().await;
    let session = ws.bound_session(&dispatcher, "temporal").await;

    // Before the instant: authorized, and the write really lands.
    let before = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "temporal.txt", "content": "before"}),
    )
    .await;
    assert!(
        before["error"].is_null(),
        "a not-yet-expired grant must authorize: {before}"
    );
    assert_eq!(
        std::fs::read_to_string(ws.root.join("temporal.txt")).unwrap(),
        "before"
    );

    // Let the instant pass (bounded wait; no grant is mutated).
    tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

    // After the instant: the identical call is denied, bytes untouched.
    let after = call(
        &dispatcher,
        &session,
        "workspace.write_file",
        json!({"path": "temporal.txt", "content": "after"}),
    )
    .await;
    assert_eq!(
        error_of(&after)["code"],
        json!(CAPABILITY_DENIED_CODE),
        "the expired grant must deny the same session's next call: {after}"
    );
    assert_eq!(
        std::fs::read_to_string(ws.root.join("temporal.txt")).unwrap(),
        "before",
        "the expired denial left the resource unchanged"
    );
}

// ---------------------------------------------------------------------------
// §8: duplicate grant ids are deterministic — re-granting the same
// agent/permission pair through the CLI overwrites in place (the store
// is keyed by id), never duplicates, and the LAST write wins visibly.
// ---------------------------------------------------------------------------

/// `awh agent grant alpha --permission filesystem` twice (the second
/// time with a different scope): exactly ONE grant record exists
/// afterward, carrying the SECOND scope — no duplicate rows, no stale
/// first-write authority, no error.
#[test]
fn re_granting_the_same_pair_overwrites_deterministically() {
    let ws = Ws::new(&["alpha"]);
    let store = CapabilityGrantStore::new(&ws.root);

    let (ok, _out, err) = awh(
        &ws.root,
        &[
            "agent",
            "grant",
            "alpha",
            "--permission",
            "filesystem",
            "--scope",
            "old",
        ],
    );
    assert!(ok, "first grant: {err}");

    let (ok, _out, err) = awh(
        &ws.root,
        &[
            "agent",
            "grant",
            "alpha",
            "--permission",
            "filesystem",
            "--scope",
            "new",
        ],
    );
    assert!(ok, "second grant must succeed (overwrite): {err}");

    // Exactly one record, and it carries the LATEST scope.
    let grants = store.list_for_agent("alpha").expect("list");
    assert_eq!(grants.len(), 1, "re-grant must not duplicate: {grants:?}");
    assert_eq!(grants[0].id, "alpha-filesystem");
    assert_eq!(grants[0].scope.as_deref(), Some("new"));

    // The same truth via the CLI's own listing surface.
    let (ok, out, err) = awh(&ws.root, &["agent", "inspect", "alpha"]);
    assert!(ok, "inspect: {err}");
    assert!(
        out.contains("alpha-filesystem") && out.contains("new"),
        "inspect renders the latest grant: {out}"
    );
}
