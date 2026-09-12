//! Phase 3: workspace-scoped, DENY-only policy rules on top of the Phase 2
//! built-in tool gate.
//!
//! These tests pin the mandatory behavioral properties of the
//! workspace-local policy store (`.agent/policy.json`) for the three tools
//! whose call carries a resource that matters (`workspace.write_file`,
//! `workspace.delete_file`, `terminal.run`):
//!
//! * **Backward compatible by default** — a workspace with no
//!   `.agent/policy.json` (every pre-existing workspace) sees byte-for-byte
//!   the Phase 2 behavior, for all three gated tools.
//! * **Deny narrows further, never widens** — a matching rule rejects the
//!   call *before* the underlying service runs, producing a `policy_denied`
//!   audit event distinguishable from Phase 2's `builtin_tool_denied`.
//! * **Precise matching** — a rule for a different tool, or a non-matching
//!   pattern on the same tool, leaves unrelated calls untouched.
//! * **CLI round-trip** — `policy deny` → `policy list` → denied → `policy
//!   remove` → allowed again; and an unsupported tool is rejected by the CLI
//!   before anything is persisted.

use agent_workspace_hub::core::policy::PolicyStore;
use agent_workspace_hub::mcp::{
    McpDispatcher, McpPermissions, PersistentTrustStore, SessionLifecycle, TrustLevel, TrustStore,
    BUILTIN_TOOL_DENIED_CODE, BUILTIN_TOOL_TRUST_ID, POLICY_DENIED_CODE,
};
use agent_workspace_hub::models::PolicyRule;
use agent_workspace_hub::services::audit::global as audit_log;
use serde_json::{json, Value};
use tempfile::tempdir;

/// An empty persistent trust store: present, but with no `awh.builtin`
/// record — so Phase 2's coarse gate allows every Medium/High tool, letting
/// these tests isolate the *policy* layer's effect.
fn empty_trust_store() -> PersistentTrustStore {
    PersistentTrustStore::from_store(&TrustStore::default())
}

/// A restrictive `awh.builtin` record granting no capabilities: the least
/// privilege configuration the trust CLI writes. Used to prove the coarse
/// category-level gate denies a resource-scoped tool *before* the policy
/// layer is even consulted.
fn restrictive_trust_store() -> PersistentTrustStore {
    let mut store = TrustStore::default();
    store
        .approve(
            BUILTIN_TOOL_TRUST_ID,
            TrustLevel::Reviewed,
            McpPermissions {
                network: false,
                process: false,
                filesystem: Vec::new(),
                ..McpPermissions::default()
            },
            "local".to_string(),
        )
        .expect("valid approval");
    PersistentTrustStore::from_store(&store)
}

/// Builds a dispatcher with an empty (allow-everything) trust store and an
/// injected policy store rooted at `policy_root`. Injecting the store lets a
/// test write rules under a controlled directory before the dispatcher is
/// constructed — and, crucially, proves the dispatcher *reads* the policy
/// store at construction time (the same snapshot-on-construction behavior as
/// the trust store).
async fn dispatcher_with_policy(
    project_root: &std::path::Path,
    policy_root: &std::path::Path,
) -> McpDispatcher {
    McpDispatcher::new_async(project_root.to_path_buf())
        .await
        .expect("dispatcher")
        .with_trust_store(empty_trust_store())
        .with_policy_store(PolicyStore::new(policy_root))
}

fn request(id: Value, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn init_params() -> Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "policy-gate-test", "version": "0.0.1"}
    })
}

/// Initializes a fresh session and calls a tool, returning the raw JSON-RPC
/// response object (whose `error` member carries `code` and `message` on a
/// denial).
async fn call_tool_raw(dispatcher: &McpDispatcher, name: &str, arguments: Value) -> Value {
    use agent_workspace_hub::mcp::DispatchResult;
    let lifecycle = SessionLifecycle::default();
    let init = request(json!(0), "initialize", init_params());
    match dispatcher.dispatch_with_lifecycle(&init, &lifecycle).await {
        DispatchResult::Response(_) => {}
        DispatchResult::NoResponse => panic!("initialize must produce a response"),
    }
    let input = request(
        json!(1),
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    );
    match dispatcher.dispatch_with_lifecycle(&input, &lifecycle).await {
        DispatchResult::Response(resp) => serde_json::to_value(&resp).expect("serialize"),
        DispatchResult::NoResponse => panic!("{name} call with id must produce a response"),
    }
}

/// Calls a tool expecting a JSON-RPC error, returning the `error` member.
async fn call_tool_expect_error(dispatcher: &McpDispatcher, name: &str, arguments: Value) -> Value {
    let response = call_tool_raw(dispatcher, name, arguments).await;
    assert!(
        response["error"].is_object(),
        "{name} must be denied with a JSON-RPC error, got: {response}"
    );
    response["error"].clone()
}

/// Whether the global audit ring holds *any* entry with this action (used to
/// prove a `policy_denied` exists independently of its subject).
fn any_audited_action(action: &str) -> bool {
    audit_log().recent(1000).iter().any(|e| e.action == action)
}

fn rule(id: &str, tool: &str, pattern: &str) -> PolicyRule {
    PolicyRule {
        id: id.into(),
        tool: tool.into(),
        pattern: pattern.into(),
        reason: Some("test denial".to_string()),
        created_at: "2026-09-12T00:00:00+00:00".into(),
    }
}

// ---------------------------------------------------------------------------
// A. Backward compatible by default (the single most important property)
// ---------------------------------------------------------------------------

/// With zero policy rules, all three gated tools behave exactly as they did
/// after PR #19 (Phase 2). This is the most important test in this file: a
/// workspace with no `.agent/policy.json` must see zero behavior change.
#[tokio::test]
async fn no_policy_rules_leaves_all_three_tools_working() {
    let project = tempdir().expect("tempdir");
    let policy_dir = tempdir().expect("policy tempdir");
    let dispatcher = dispatcher_with_policy(project.path(), policy_dir.path()).await;
    let root = project.path().to_path_buf();

    // terminal.run
    let result = call_tool_raw(
        &dispatcher,
        "terminal.run",
        json!({"program": "git", "args": ["--version"]}),
    )
    .await;
    assert!(
        result["result"]["content"][0]["text"].is_string(),
        "terminal.run must succeed with no policy rules: {result}"
    );

    // workspace.write_file
    let result = call_tool_raw(
        &dispatcher,
        "workspace.write_file",
        json!({"path": "a.txt", "content": "hello"}),
    )
    .await;
    let text = result["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap()["written"],
        json!(true)
    );
    assert!(root.join("a.txt").exists());

    // workspace.delete_file
    let result = call_tool_raw(
        &dispatcher,
        "workspace.delete_file",
        json!({"path": "a.txt"}),
    )
    .await;
    let text = result["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap()["deleted"],
        json!(true)
    );
    assert!(!root.join("a.txt").exists());
}

// ---------------------------------------------------------------------------
// B. Deny narrows before the service runs (and audits as policy_denied)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deny_rule_rejects_write_file_before_writing() {
    let project = tempdir().expect("tempdir");
    let policy_dir = tempdir().expect("policy tempdir");
    let store = PolicyStore::new(policy_dir.path());
    store
        .add(&rule("r1", "workspace.write_file", "secrets/"))
        .unwrap();
    let dispatcher = dispatcher_with_policy(project.path(), policy_dir.path()).await;

    let error = call_tool_expect_error(
        &dispatcher,
        "workspace.write_file",
        json!({"path": "secrets/token.txt", "content": "nope"}),
    )
    .await;
    assert_eq!(error["code"], json!(POLICY_DENIED_CODE), "got: {error}");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("workspace.write_file"),
        "must name the tool: {message}"
    );
    assert!(message.contains("r1"), "must name the rule id: {message}");
    assert!(
        message.contains("test denial"),
        "must include the reason: {message}"
    );
    assert!(
        !project.path().join("secrets/token.txt").exists(),
        "denied write_file must not create the file"
    );
    assert!(
        any_audited_action("policy_denied"),
        "the deny must be audited as policy_denied"
    );
}

#[tokio::test]
async fn deny_rule_rejects_delete_file_without_deleting() {
    let project = tempdir().expect("tempdir");
    let policy_dir = tempdir().expect("policy tempdir");
    // Create the file through an unrestricted dispatcher first.
    {
        let dispatcher = dispatcher_with_policy(project.path(), policy_dir.path()).await;
        let result = call_tool_raw(
            &dispatcher,
            "workspace.write_file",
            json!({"path": "keep.txt", "content": "x"}),
        )
        .await;
        assert!(result["result"]["content"][0]["text"].is_string());
    }
    assert!(project.path().join("keep.txt").exists());

    // Now add a delete rule and re-construct the dispatcher so it reads it.
    let store = PolicyStore::new(policy_dir.path());
    store
        .add(&rule("r2", "workspace.delete_file", "keep.txt"))
        .unwrap();
    let dispatcher = dispatcher_with_policy(project.path(), policy_dir.path()).await;

    let error = call_tool_expect_error(
        &dispatcher,
        "workspace.delete_file",
        json!({"path": "keep.txt"}),
    )
    .await;
    assert_eq!(error["code"], json!(POLICY_DENIED_CODE), "got: {error}");
    assert!(
        project.path().join("keep.txt").exists(),
        "denied delete_file must not delete the file"
    );
    assert!(any_audited_action("policy_denied"));
}

#[tokio::test]
async fn deny_rule_rejects_terminal_run_without_running() {
    let project = tempdir().expect("tempdir");
    let policy_dir = tempdir().expect("policy tempdir");
    let store = PolicyStore::new(policy_dir.path());
    // `git` is the exact-match command; CI has git on all platforms.
    store.add(&rule("r3", "terminal.run", "git")).unwrap();
    let dispatcher = dispatcher_with_policy(project.path(), policy_dir.path()).await;

    // A denied `git` must not run: use a side-effecting arg that would be
    // observable if the process ran (git exits 0 on --version; we assert the
    // JSON-RPC error instead of any process side effect, matching the Phase 2
    // tests' approach of asserting before-the-service-ran).
    let error = call_tool_expect_error(
        &dispatcher,
        "terminal.run",
        json!({"program": "git", "args": ["--version"]}),
    )
    .await;
    assert_eq!(error["code"], json!(POLICY_DENIED_CODE), "got: {error}");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("terminal.run"),
        "must name the tool: {message}"
    );
    assert!(message.contains("r3"), "must name the rule id: {message}");
    assert!(any_audited_action("policy_denied"));
}

// ---------------------------------------------------------------------------
// C. Precise matching — a rule must not over-broaden
// ---------------------------------------------------------------------------

/// A rule for a DIFFERENT tool does not affect this tool's call.
#[tokio::test]
async fn rule_for_a_different_tool_does_not_affect_this_call() {
    let project = tempdir().expect("tempdir");
    let policy_dir = tempdir().expect("policy tempdir");
    let store = PolicyStore::new(policy_dir.path());
    // A delete rule must not affect a write (different tool), even with a
    // pattern that would match the write's path.
    store
        .add(&rule("r1", "workspace.delete_file", "files/"))
        .unwrap();
    let dispatcher = dispatcher_with_policy(project.path(), policy_dir.path()).await;

    let result = call_tool_raw(
        &dispatcher,
        "workspace.write_file",
        json!({"path": "files/ok.txt", "content": "x"}),
    )
    .await;
    assert!(
        result["result"]["content"][0]["text"].is_string(),
        "write_file must succeed despite a delete rule with a matching path: {result}"
    );
    assert!(project.path().join("files/ok.txt").exists());
}

/// A non-matching pattern on the same tool does not affect an unrelated call.
#[tokio::test]
async fn non_matching_pattern_on_same_tool_does_not_affect_other_calls() {
    let project = tempdir().expect("tempdir");
    let policy_dir = tempdir().expect("policy tempdir");
    let store = PolicyStore::new(policy_dir.path());
    store
        .add(&rule("r1", "workspace.write_file", "secrets/"))
        .unwrap();
    let dispatcher = dispatcher_with_policy(project.path(), policy_dir.path()).await;

    // A write outside the denied prefix still succeeds.
    let result = call_tool_raw(
        &dispatcher,
        "workspace.write_file",
        json!({"path": "public.txt", "content": "ok"}),
    )
    .await;
    let text = result["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap()["written"],
        json!(true)
    );
    assert!(project.path().join("public.txt").exists());
}

/// A terminal.run rule matches exactly (not as a substring), and does not
/// leak across programs.
#[tokio::test]
async fn terminal_rule_is_exact_not_substring() {
    let project = tempdir().expect("tempdir");
    let policy_dir = tempdir().expect("policy tempdir");
    let store = PolicyStore::new(policy_dir.path());
    store.add(&rule("r1", "terminal.run", "git")).unwrap();
    let dispatcher = dispatcher_with_policy(project.path(), policy_dir.path()).await;

    // "git" is denied, but a different program is unaffected.
    let denied = call_tool_expect_error(
        &dispatcher,
        "terminal.run",
        json!({"program": "git", "args": ["--version"]}),
    )
    .await;
    assert_eq!(denied["code"], json!(POLICY_DENIED_CODE));

    // A program whose name merely *contains* "git" is not matched.
    // (We don't actually run an imaginary binary; we assert the policy layer
    // does not deny it — the call proceeds past policy and either runs or the
    // spawn fails as a normal tool error, not a policy denial.)
    let shop = dispatcher_with_policy(project.path(), policy_dir.path()).await;
    let response = call_tool_raw(
        &shop,
        "terminal.run",
        json!({"program": "git-annex-not-a-real-bin", "args": []}),
    )
    .await;
    // Not a policy denial: no -32004 error code.
    let code = response["error"]["code"].clone();
    assert_ne!(
        code,
        json!(POLICY_DENIED_CODE),
        "a substring program must not be policy-denied, got: {response}"
    );
}

// ---------------------------------------------------------------------------
// D. CLI round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn policy_cli_round_trip_deny_then_list_then_remove() {
    use std::process::Command;
    let dir = tempdir().expect("tempdir");

    // 1. deny: add a rule.
    let deny = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(["policy", "deny", "workspace.write_file", "sensitive/"])
        .current_dir(dir.path())
        .output()
        .expect("spawn awh");
    assert!(
        deny.status.success(),
        "policy deny failed: {}",
        String::from_utf8_lossy(&deny.stderr)
    );
    assert!(
        String::from_utf8_lossy(&deny.stdout).contains("added policy rule"),
        "got: {}",
        String::from_utf8_lossy(&deny.stdout)
    );

    // 2. list: the rule appears.
    let list = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(["policy", "list"])
        .current_dir(dir.path())
        .output()
        .expect("spawn awh");
    assert!(list.status.success());
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(
        stdout.contains("workspace.write_file") && stdout.contains("sensitive/"),
        "policy list must show the rule, got: {stdout}"
    );

    // The rule id is derivable (tool + pattern hash), so we can read it back
    // from the persisted store for a deterministic remove.
    let store = PolicyStore::new(dir.path());
    let rules = store.list().expect("list rules");
    assert_eq!(rules.len(), 1);
    let id = rules[0].id.clone();

    // 3. the matching call is now denied (end-to-end against the real dir).
    let runtime = dispatcher_with_policy(dir.path(), dir.path()).await;
    let error = call_tool_expect_error(
        &runtime,
        "workspace.write_file",
        json!({"path": "sensitive/x.txt", "content": "x"}),
    )
    .await;
    assert_eq!(error["code"], json!(POLICY_DENIED_CODE));

    // 4. remove: the rule is gone.
    let remove = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(["policy", "remove", &id])
        .current_dir(dir.path())
        .output()
        .expect("spawn awh");
    assert!(remove.status.success());
    assert!(
        String::from_utf8_lossy(&remove.stdout).contains("removed policy rule"),
        "got: {}",
        String::from_utf8_lossy(&remove.stdout)
    );

    // 5. the call succeeds again (a fresh dispatcher reads the now-empty store).
    let again = dispatcher_with_policy(dir.path(), dir.path()).await;
    let result = call_tool_raw(
        &again,
        "workspace.write_file",
        json!({"path": "sensitive/x.txt", "content": "x"}),
    )
    .await;
    assert!(
        result["result"]["content"][0]["text"].is_string(),
        "write_file must succeed again after removing the rule: {result}"
    );
    assert!(dir.path().join("sensitive/x.txt").exists());
}

#[test]
fn policy_cli_rejects_unsupported_tool_before_persisting() {
    use std::process::Command;
    let dir = tempdir().expect("tempdir");

    let output = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(["policy", "deny", "memory.store", "anything"])
        .current_dir(dir.path())
        .output()
        .expect("spawn awh");
    assert!(
        !output.status.success(),
        "unsupported tool must be rejected by the CLI"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unsupported policy tool"),
        "got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Nothing persisted.
    assert!(
        !dir.path().join(".agent/policy.json").exists(),
        "an unsupported tool must not create .agent/policy.json"
    );
}

// ---------------------------------------------------------------------------
// E. Fail closed on an unreadable store
// ---------------------------------------------------------------------------

/// A corrupt `policy.json` must fail closed (deny) rather than fall back to
/// allow, surfacing as an internal error (not a policy denial with no rule).
#[tokio::test]
async fn corrupt_policy_store_fails_closed() {
    let project = tempdir().expect("tempdir");
    let policy_dir = tempdir().expect("policy tempdir");
    std::fs::create_dir_all(policy_dir.path().join(".agent")).unwrap();
    std::fs::write(
        policy_dir.path().join(".agent/policy.json"),
        "{ not valid json",
    )
    .unwrap();
    let dispatcher = dispatcher_with_policy(project.path(), policy_dir.path()).await;

    let response = call_tool_raw(
        &dispatcher,
        "workspace.write_file",
        json!({"path": "x.txt", "content": "x"}),
    )
    .await;
    assert!(
        response["error"].is_object(),
        "a corrupt policy store must fail closed, got: {response}"
    );
    assert!(
        !project.path().join("x.txt").exists(),
        "file must not be written"
    );
}

// ---------------------------------------------------------------------------
// E2. authorize_tool runs the coarse gate before the policy check
// ---------------------------------------------------------------------------

/// With *both* a restrictive trust store (coarse gate denies) and a matching
/// policy deny rule, the call must be denied by the coarse
/// [`BUILTIN_TOOL_DENIED_CODE`] gate — not by the policy layer — proving
/// `authorize_tool` consults `authorize_builtin` before `authorize_policy`.
#[tokio::test]
async fn write_file_coarse_gate_runs_before_policy() {
    let project = tempdir().expect("tempdir");
    let policy_dir = tempdir().expect("policy tempdir");
    let store = PolicyStore::new(policy_dir.path());
    store
        .add(&rule("r1", "workspace.write_file", "secrets/"))
        .unwrap();
    let dispatcher = McpDispatcher::new_async(project.path().to_path_buf())
        .await
        .expect("dispatcher")
        .with_trust_store(restrictive_trust_store())
        .with_policy_store(PolicyStore::new(policy_dir.path()));

    let error = call_tool_expect_error(
        &dispatcher,
        "workspace.write_file",
        json!({"path": "secrets/token.txt", "content": "nope"}),
    )
    .await;
    // The coarse gate wins: the denial code/message come from the built-in
    // gate, not the policy rule.
    assert_eq!(
        error["code"],
        json!(BUILTIN_TOOL_DENIED_CODE),
        "got: {error}"
    );
    assert!(
        any_audited_action("builtin_tool_denied"),
        "the coarse gate denial must be audited"
    );
}

/// Mirrors [`write_file_coarse_gate_runs_before_policy`] for `delete_file`.
#[tokio::test]
async fn delete_file_coarse_gate_runs_before_policy() {
    let project = tempdir().expect("tempdir");
    let policy_dir = tempdir().expect("policy tempdir");
    let store = PolicyStore::new(policy_dir.path());
    store
        .add(&rule("r2", "workspace.delete_file", "keep.txt"))
        .unwrap();
    let dispatcher = McpDispatcher::new_async(project.path().to_path_buf())
        .await
        .expect("dispatcher")
        .with_trust_store(restrictive_trust_store())
        .with_policy_store(PolicyStore::new(policy_dir.path()));

    let error = call_tool_expect_error(
        &dispatcher,
        "workspace.delete_file",
        json!({"path": "keep.txt"}),
    )
    .await;
    assert_eq!(
        error["code"],
        json!(BUILTIN_TOOL_DENIED_CODE),
        "got: {error}"
    );
    assert!(any_audited_action("builtin_tool_denied"));
}

/// Mirrors [`write_file_coarse_gate_runs_before_policy`] for `terminal.run`.
#[tokio::test]
async fn terminal_run_coarse_gate_runs_before_policy() {
    let project = tempdir().expect("tempdir");
    let policy_dir = tempdir().expect("policy tempdir");
    let store = PolicyStore::new(policy_dir.path());
    store.add(&rule("r3", "terminal.run", "git")).unwrap();
    let dispatcher = McpDispatcher::new_async(project.path().to_path_buf())
        .await
        .expect("dispatcher")
        .with_trust_store(restrictive_trust_store())
        .with_policy_store(PolicyStore::new(policy_dir.path()));

    let error =
        call_tool_expect_error(&dispatcher, "terminal.run", json!({"program": "git"})).await;
    assert_eq!(
        error["code"],
        json!(BUILTIN_TOOL_DENIED_CODE),
        "got: {error}"
    );
    assert!(any_audited_action("builtin_tool_denied"));
}

// ---------------------------------------------------------------------------
// F. Source-level guard: exactly three policy call sites, no others
// ---------------------------------------------------------------------------

/// The acceptance criterion: exactly three tools are resource-scoped.
///
/// The single source of truth for that pairing is the declarative
/// `RESOURCE_SCOPED_TOOLS` table in `tool_broker.rs`. This guard scans that
/// table's source and asserts it holds exactly the three supported tools each
/// mapped to the correct resource argument — so a future resource-scoped tool
/// (or an accidental removal) is caught here rather than by an off-by-one call
/// site that a human or agent would have to remember.
#[test]
fn exactly_three_policy_call_sites() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/mcp/tool_broker.rs"
    ))
    .expect("read tool_broker.rs");
    for (tool, arg) in [
        ("workspace.write_file", "path"),
        ("workspace.delete_file", "path"),
        ("terminal.run", "program"),
    ] {
        let entry = format!("(\"{tool}\", \"{arg}\")");
        assert!(
            source.contains(&entry),
            "RESOURCE_SCOPED_TOOLS must map {tool} -> {arg}"
        );
    }
    // Exactly three entries in the table — no more and no fewer. Isolate the
    // const declaration from the following `#[cfg(test)]` module that also
    // lists the pairs, then assert the const body has exactly three tuple
    // literals (each starts a `("tool", "arg")` line).
    let table = source
        .split("pub(crate) const RESOURCE_SCOPED_TOOLS")
        .nth(1)
        .and_then(|rest| rest.split("];").next())
        .expect("RESOURCE_SCOPED_TOOLS const declaration");
    let entry_count = table.matches("(\"").count();
    assert_eq!(
        entry_count, 3,
        "exactly three RESOURCE_SCOPED_TOOLS entries expected, found {entry_count}"
    );
}
