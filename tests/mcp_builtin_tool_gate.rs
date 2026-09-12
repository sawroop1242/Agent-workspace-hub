//! Phase 2: the built-in (static) tool execution gate.
//!
//! These tests pin the four mandatory behavioral properties of the
//! [`BUILTIN_TOOL_TRUST_ID`] gate (`awh.builtin`):
//!
//! * **Backward compatible by default** — a workspace with no trust record
//!   for `awh.builtin` (every pre-existing workspace and test fixture) sees
//!   zero behavior change: all Medium/High static tools still succeed.
//! * **Now audited, always** — every gated call produces an audit event on
//!   both the allow and deny path, including `terminal.run` and
//!   `git.commit` which produced none before this phase.
//! * **Enforceable once configured** — a restrictive `awh.builtin` record
//!   rejects the tool call BEFORE the underlying service runs, so no
//!   filesystem/process/git side effect occurs.
//! * **Fail closed on error** — an unloadable trust store denies rather
//!   than falling back to allow.
//!
//! Plus a registry-coverage regression guard: every Medium/High-risk static
//! tool must be behind the gate, so a future high-risk tool can never ship
//! ungated the way `terminal.run` and `git.commit` did.

use agent_workspace_hub::mcp::tool_registry::{self, ToolRisk};
use agent_workspace_hub::mcp::{
    authorize_builtin_tool, BuiltinToolAuthorizationError, McpDispatcher, McpPermissions,
    Permission, PersistentTrustStore, SessionLifecycle, TrustLevel, TrustStore,
    BUILTIN_TOOL_TRUST_ID,
};
use agent_workspace_hub::services::audit::global as audit_log;
use agent_workspace_hub::services::git::GitService;
use serde_json::{json, Value};
use tempfile::tempdir;

/// Initializes a real git repository at `root` with a committed identity
/// (the `git.commit` tests need one; the service layer intentionally has no
/// repo-creation API, and git refuses to commit without user.name/email).
fn git_init(root: &std::path::Path) {
    let commands: [&[&str]; 3] = [
        &["init", "--quiet"],
        &["config", "user.email", "gate-test@example.invalid"],
        &["config", "user.name", "Gate Test"],
    ];
    for args in commands {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("spawn git")
            .status
            .success();
        assert!(ok, "git {args:?} must succeed in {}", root.display());
    }
}

/// The permission set the `awh mcp trust awh.builtin --...` CLI command
/// writes for the reserved identity: the capability flags map to booleans,
/// and a granted list-valued capability carries the reserved scope label.
fn builtin_permissions(network: bool, process: bool, filesystem: bool) -> McpPermissions {
    McpPermissions {
        network,
        process,
        filesystem: if filesystem {
            vec![BUILTIN_TOOL_TRUST_ID.to_string()]
        } else {
            Vec::new()
        },
        ..McpPermissions::default()
    }
}

/// Builds a persistent trust store holding one `awh.builtin` record.
fn builtin_trust_store(level: TrustLevel, permissions: McpPermissions) -> PersistentTrustStore {
    let mut store = TrustStore::default();
    store
        .approve(
            BUILTIN_TOOL_TRUST_ID,
            level,
            permissions,
            "local".to_string(),
        )
        .expect("valid approval");
    PersistentTrustStore::from_store(&store)
}

/// An empty persistent store: present, but with no `awh.builtin` record —
/// the state every pre-existing workspace is in.
fn empty_trust_store() -> PersistentTrustStore {
    PersistentTrustStore::from_store(&TrustStore::default())
}

/// A dispatcher whose built-in gate reads `trust`, mirroring the
/// `new_dispatcher()` harness of `tests/mcp_protocol.rs`.
async fn gated_dispatcher(trust: PersistentTrustStore) -> (McpDispatcher, tempfile::TempDir) {
    let dir = tempdir().expect("tempdir");
    let dispatcher = McpDispatcher::new_async(dir.path().to_path_buf())
        .await
        .expect("dispatcher")
        .with_trust_store(trust);
    (dispatcher, dir)
}

fn request(id: Value, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn init_params() -> Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "builtin-gate-test", "version": "0.0.1"}
    })
}

/// Drives a dispatcher through a full session and returns the parsed JSON
/// payload of a successful `tools/call` (the `content[0].text` envelope the
/// MCP protocol wraps every tool result in).
async fn call_tool(dispatcher: &McpDispatcher, name: &str, arguments: Value) -> Value {
    let response = call_tool_raw(dispatcher, name, arguments).await;
    // tools/call results arrive in an MCP content envelope; unwrap the
    // JSON text blob.
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{name} must return text content, got: {response}"));
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{name} text is not JSON ({e}): {text}"))
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

/// Calls a tool expecting a JSON-RPC error, asserting the denial reached
/// the surface as the dedicated code with a message naming the tool.
async fn call_tool_expect_error(dispatcher: &McpDispatcher, name: &str, arguments: Value) -> Value {
    let response = call_tool_raw(dispatcher, name, arguments).await;
    assert!(
        response["error"].is_object(),
        "{name} must be denied with a JSON-RPC error, got: {response}"
    );
    response["error"].clone()
}

/// Whether the global audit ring holds an entry with this kind, action, and
/// subject.
fn audited(kind: &str, action: &str, subject: &str) -> bool {
    audit_log()
        .recent(1000)
        .iter()
        .any(|e| e.kind == kind && e.action == action && e.subject == subject)
}

// ---------------------------------------------------------------------------
// A. Backward compatible by default (the single most important property)
// ---------------------------------------------------------------------------

/// The audit ring is global and shared across tests in this binary, so a
/// deny assertion cannot use "no deny entry exists". Instead each deny test
/// asserts its distinctive (action, subject) pair fired, and this test pins
/// that the no-record allow path produces its own event.
#[tokio::test]
async fn no_builtin_record_leaves_every_medium_high_static_tool_working() {
    let (dispatcher, dir) = gated_dispatcher(empty_trust_store()).await;
    let root = dir.path().to_path_buf();

    // terminal.run (High, process): executes a real command.
    let result = call_tool(
        &dispatcher,
        "terminal.run",
        json!({"program": "git", "args": ["--version"]}),
    )
    .await;
    assert!(!result.is_null(), "terminal.run must succeed: {result}");

    // workspace.write_file (Medium, filesystem): writes a real file.
    let result = call_tool(
        &dispatcher,
        "workspace.write_file",
        json!({"path": "a.txt", "content": "hello"}),
    )
    .await;
    assert_eq!(result["written"], json!(true), "got: {result}");
    assert!(root.join("a.txt").exists());

    // workspace.delete_file (Medium, filesystem): deletes it again.
    let result = call_tool(
        &dispatcher,
        "workspace.delete_file",
        json!({"path": "a.txt"}),
    )
    .await;
    assert_eq!(result["deleted"], json!(true), "got: {result}");
    assert!(!root.join("a.txt").exists());

    // git.commit (Medium, filesystem): a real repo, real commit.
    git_init(&root);
    std::fs::write(root.join("to-commit.txt"), "content").expect("write file to commit");
    GitService::open(&root)
        .expect("git service")
        .stage(".")
        .await
        .expect("stage");
    let result = call_tool(
        &dispatcher,
        "git.commit",
        json!({"message": "phase2 baseline"}),
    )
    .await;
    assert!(
        result["stdout"].is_string(),
        "git.commit must succeed: {result}"
    );

    // And the remaining representative Medium/High shapes.
    let result = call_tool(
        &dispatcher,
        "memory.store",
        json!({"id": "m1", "content": "c", "scope": "Project"}),
    )
    .await;
    assert!(
        result["id"].is_string(),
        "memory.store must succeed: {result}"
    );
    let result = call_tool(
        &dispatcher,
        "tasks.create",
        json!({"id": "t1", "title": "t", "description": "d"}),
    )
    .await;
    assert!(
        result["id"].is_string(),
        "tasks.create must succeed: {result}"
    );
    // git.stage (Medium, filesystem): real repo, real stage.
    let result = call_tool(&dispatcher, "git.stage", json!({"path": "."})).await;
    assert!(
        result["stdout"].is_string(),
        "git.stage must succeed: {result}"
    );
    // git.branch (Medium): lists the branch the init created.
    let result = call_tool(&dispatcher, "git.branch", json!({})).await;
    assert!(
        result["stdout"].is_string(),
        "git.branch must succeed: {result}"
    );
}

// ---------------------------------------------------------------------------
// B. Now audited, always
// ---------------------------------------------------------------------------

#[tokio::test]
async fn terminal_run_and_git_commit_are_now_audited_on_success() {
    let (dispatcher, dir) = gated_dispatcher(empty_trust_store()).await;
    let root = dir.path().to_path_buf();

    let result = call_tool(
        &dispatcher,
        "terminal.run",
        json!({"program": "git", "args": ["--version"]}),
    )
    .await;
    assert!(!result.is_null());
    assert!(
        audited("allow", "builtin_tool_allowed", "terminal.run"),
        "terminal.run success must produce a builtin_tool_allowed audit event"
    );

    git_init(&root);
    std::fs::write(root.join("audit-commit.txt"), "content").expect("write file to commit");
    GitService::open(&root)
        .expect("git service")
        .stage(".")
        .await
        .expect("stage");
    let result = call_tool(&dispatcher, "git.commit", json!({"message": "audited"})).await;
    assert!(result["stdout"].is_string());
    assert!(
        audited("allow", "builtin_tool_allowed", "git.commit"),
        "git.commit success must produce a builtin_tool_allowed audit event"
    );
}

#[tokio::test]
async fn every_gated_allow_path_is_audited() {
    let (dispatcher, _dir) = gated_dispatcher(empty_trust_store()).await;
    for (tool, arguments) in [
        (
            "workspace.write_file",
            json!({"path": "audit.txt", "content": "x"}),
        ),
        ("workspace.delete_file", json!({"path": "audit.txt"})),
        (
            "memory.store",
            json!({"id": "a1", "content": "x", "scope": "Project"}),
        ),
        (
            "tasks.create",
            json!({"id": "t1", "title": "t", "description": "d"}),
        ),
    ] {
        let result = call_tool(&dispatcher, tool, arguments).await;
        assert!(!result.is_null(), "{tool} must succeed: {result}");
        assert!(
            audited("allow", "builtin_tool_allowed", tool),
            "{tool} allow must be audited"
        );
    }
}

// ---------------------------------------------------------------------------
// C. Enforceable once configured — no side effect on denial
// ---------------------------------------------------------------------------

#[tokio::test]
async fn restrictive_record_denies_write_file_without_touching_the_filesystem() {
    // Reviewed record granting nothing: the least-privilege configuration
    // `awh mcp trust awh.builtin` writes with no capability flags.
    let (dispatcher, dir) = gated_dispatcher(builtin_trust_store(
        TrustLevel::Reviewed,
        builtin_permissions(false, false, false),
    ))
    .await;
    let root = dir.path().to_path_buf();

    let error = call_tool_expect_error(
        &dispatcher,
        "workspace.write_file",
        json!({"path": "must-not-exist.txt", "content": "nope"}),
    )
    .await;
    // The denial is a JSON-RPC server error with the dedicated code, naming
    // the tool and the missing permission.
    assert_eq!(error["code"], json!(-32003), "got: {error}");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("workspace.write_file"),
        "message must name the tool: {message}"
    );
    assert!(
        message.contains("filesystem"),
        "message must name the missing permission: {message}"
    );
    // BEFORE the service ran: the file must not exist.
    assert!(
        !root.join("must-not-exist.txt").exists(),
        "denied write_file must not create the file"
    );
    assert!(
        audited("deny", "builtin_tool_denied", "workspace.write_file"),
        "the deny must be audited"
    );
}

#[tokio::test]
async fn restrictive_record_denies_delete_file_without_deleting() {
    let (dispatcher, dir) = gated_dispatcher(empty_trust_store()).await;
    let root = dir.path().to_path_buf();
    // Create a file while unrestricted, then restrict the dispatcher.
    let result = call_tool(
        &dispatcher,
        "workspace.write_file",
        json!({"path": "keep.txt", "content": "x"}),
    )
    .await;
    assert_eq!(result["written"], json!(true));
    drop(dispatcher);

    let (restricted, _dir) = gated_dispatcher(builtin_trust_store(
        TrustLevel::Reviewed,
        builtin_permissions(false, false, false),
    ))
    .await;
    let error = call_tool_expect_error(
        &restricted,
        "workspace.delete_file",
        json!({"path": "keep.txt"}),
    )
    .await;
    assert_eq!(error["code"], json!(-32003), "got: {error}");
    // The file still exists: the delete never ran.
    assert!(
        root.join("keep.txt").exists(),
        "denied delete_file must not delete the file"
    );
    assert!(
        audited("deny", "builtin_tool_denied", "workspace.delete_file"),
        "the deny must be audited"
    );
}

#[tokio::test]
async fn restrictive_record_denies_terminal_run_without_running_the_process() {
    let (dispatcher, dir) = gated_dispatcher(builtin_trust_store(
        TrustLevel::Reviewed,
        builtin_permissions(false, false, false),
    ))
    .await;
    let root = dir.path().to_path_buf();

    // If the process ran, it would create this file.
    let marker = root.join("ran-marker.txt");
    #[cfg(unix)]
    let arguments = json!({"program": "touch", "args": [marker.to_str().unwrap()]});
    #[cfg(windows)]
    let arguments = json!({"program": "cmd", "args": ["/C", "type nul > marker.txt"]});
    let error = call_tool_expect_error(&dispatcher, "terminal.run", arguments).await;
    assert_eq!(error["code"], json!(-32003), "got: {error}");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("terminal.run"),
        "must name the tool: {message}"
    );
    assert!(
        message.contains("process"),
        "must name the permission: {message}"
    );
    // BEFORE the process ran.
    #[cfg(unix)]
    let exists = marker.exists();
    #[cfg(windows)]
    let exists = root.join("ran-marker.txt").exists();
    assert!(!exists, "denied terminal.run must not run the process");
    assert!(audited("deny", "builtin_tool_denied", "terminal.run"));
}

#[tokio::test]
async fn restrictive_record_denies_git_commit_without_committing() {
    let (dispatcher, dir) = gated_dispatcher(builtin_trust_store(
        TrustLevel::Reviewed,
        builtin_permissions(false, false, false),
    ))
    .await;
    let root = dir.path().to_path_buf();

    // A real repo with a staged change, so a commit would succeed if allowed.
    git_init(&root);
    let git = GitService::open(&root).expect("git service");
    std::fs::write(root.join("staged.txt"), "content").expect("write staged file");
    git.stage(".").await.expect("stage");

    let error = call_tool_expect_error(
        &dispatcher,
        "git.commit",
        json!({"message": "must-not-commit"}),
    )
    .await;
    assert_eq!(error["code"], json!(-32003), "got: {error}");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("git.commit"),
        "must name the tool: {message}"
    );
    assert!(
        message.contains("filesystem"),
        "must name the permission: {message}"
    );
    // The commit never happened: HEAD still has zero entries.
    let log = git.log(10).await.expect("git log");
    assert!(
        log.stdout.trim().is_empty(),
        "denied git.commit must not create a commit, got log: {}",
        log.stdout
    );
    assert!(audited("deny", "builtin_tool_denied", "git.commit"));
}

#[tokio::test]
async fn process_grant_alone_does_not_permit_filesystem_tools() {
    // Least-privilege is enforced per permission: granting only `process`
    // still denies `workspace.write_file` (needs filesystem).
    let (dispatcher, dir) = gated_dispatcher(builtin_trust_store(
        TrustLevel::Reviewed,
        builtin_permissions(false, true, false),
    ))
    .await;
    let error = call_tool_expect_error(
        &dispatcher,
        "workspace.write_file",
        json!({"path": "no.txt", "content": "x"}),
    )
    .await;
    assert_eq!(error["code"], json!(-32003), "got: {error}");
    assert!(
        !dir.path().join("no.txt").exists(),
        "the file must not be written"
    );
}

#[tokio::test]
async fn blocked_record_denies_every_gated_tool() {
    // `awh mcp block awh.builtin` denies regardless of granted capabilities.
    let (dispatcher, _dir) = gated_dispatcher(builtin_trust_store(
        TrustLevel::Blocked,
        builtin_permissions(true, true, true),
    ))
    .await;
    for (tool, arguments) in [
        (
            "terminal.run",
            json!({"program": "git", "args": ["--version"]}),
        ),
        (
            "workspace.write_file",
            json!({"path": "x.txt", "content": "x"}),
        ),
        (
            "memory.store",
            json!({"id": "m", "content": "c", "scope": "Project"}),
        ),
        (
            "tasks.create",
            json!({"id": "t", "title": "t", "description": "d"}),
        ),
    ] {
        let error = call_tool_expect_error(&dispatcher, tool, arguments).await;
        assert_eq!(error["code"], json!(-32003), "{tool}: {error}");
        assert!(
            audited("deny", "builtin_tool_denied", tool),
            "{tool} deny audited"
        );
    }
}

#[tokio::test]
async fn fully_granted_record_keeps_tools_working() {
    // The exact record `awh mcp trust awh.builtin --network --process
    // --filesystem` writes: every gated tool keeps working.
    let (dispatcher, _dir) = gated_dispatcher(builtin_trust_store(
        TrustLevel::Reviewed,
        builtin_permissions(true, true, true),
    ))
    .await;
    let result = call_tool(
        &dispatcher,
        "terminal.run",
        json!({"program": "git", "args": ["--version"]}),
    )
    .await;
    assert!(!result.is_null(), "terminal.run must succeed: {result}");
    let result = call_tool(
        &dispatcher,
        "workspace.write_file",
        json!({"path": "granted.txt", "content": "ok"}),
    )
    .await;
    assert_eq!(result["written"], json!(true), "got: {result}");
}

// ---------------------------------------------------------------------------
// D. Fail closed on error
// ---------------------------------------------------------------------------

#[test]
fn missing_trust_store_fails_closed_for_every_gated_tool() {
    // None = the store could not be loaded (corrupt trust.json): deny.
    for tool in [
        "terminal.run",
        "git.commit",
        "workspace.write_file",
        "workspace.delete_file",
        "memory.store",
        "tasks.create",
        "skills.add",
        "github.pr_create",
        "context.insert",
        "connector.invoke",
    ] {
        assert_eq!(
            authorize_builtin_tool(tool, None),
            Err(BuiltinToolAuthorizationError::StoreUnavailable {
                tool: tool.to_string()
            }),
            "{tool} must fail closed without a loadable trust store"
        );
    }
    assert!(audited("deny", "builtin_tool_denied", "terminal.run"));
}

#[test]
fn low_risk_tools_never_hit_the_gate() {
    // Reads are out of scope: even the most restrictive possible record
    // (blocked, nothing granted) leaves them allowed.
    let store = builtin_trust_store(TrustLevel::Blocked, McpPermissions::default());
    for tool in [
        "workspace.read_file",
        "git.status",
        "git.log",
        "memory.get",
        "memory.search",
        "tasks.list",
        "skills.list",
        "context.status",
    ] {
        assert_eq!(
            authorize_builtin_tool(tool, Some(&store)),
            Ok(()),
            "low-risk {tool} must not be gated"
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Registry coverage: no Medium/High static tool may ship ungated
// ---------------------------------------------------------------------------

#[test]
fn every_medium_high_risk_static_tool_is_behind_the_gate() {
    // The literal (non catch-all) dispatcher arms are the static surface;
    // the `_ if name.contains('.')` arm is the dynamic provider path and is
    // out of scope (it is already gated by the custom-server is_authorized).
    let static_arms = static_dispatcher_arms();
    assert!(!static_arms.is_empty(), "static arm list must not be empty");

    let mut gated = Vec::new();
    let mut skipped = Vec::new();
    for definition in tool_registry::registry_entries() {
        let name = definition.name;
        if definition.risk != ToolRisk::Medium && definition.risk != ToolRisk::High {
            continue;
        }
        if static_arms.iter().any(|arm| arm == name) {
            gated.push(name.to_string());
        } else {
            // Not a literal arm, so it dispatches dynamically: already
            // covered by the custom-provider gate.
            skipped.push(name.to_string());
        }
    }
    assert_eq!(
        skipped,
        Vec::<String>::new(),
        "every Medium/High registry entry must either be a static dispatcher \
         arm (gated by awh.builtin) or be explained: {skipped:?}"
    );
    // And each gated arm really is gated: a fully-restrictive record
    // (Blocked) denies every one of them at the gate function level.
    let store = builtin_trust_store(TrustLevel::Blocked, McpPermissions::default());
    for tool in &gated {
        assert!(
            authorize_builtin_tool(tool, Some(&store)).is_err(),
            "{tool} must be denied by a blocked awh.builtin record"
        );
    }
    // The four tools this phase was opened for must be in the gated set.
    for tool in [
        "terminal.run",
        "git.commit",
        "workspace.write_file",
        "workspace.delete_file",
    ] {
        assert!(
            gated.contains(&tool.to_string()),
            "{tool} must be gated (it is the reason this phase exists)"
        );
    }
}

/// Source-level coverage guard, the direct regression test for how these
/// four tools shipped ungated: every literal dispatcher arm for a
/// Medium/High registry tool must contain its own
/// `self.authorize_builtin("<tool>")` call. A future high-risk tool whose
/// arm forgets the gate fails here (and the behavioral tests above prove
/// the gate itself denies when invoked).
#[test]
fn every_medium_high_literal_arm_calls_the_gate() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/mcp/dispatcher.rs"
    ))
    .expect("read dispatcher.rs");

    for definition in tool_registry::registry_entries() {
        let name = definition.name;
        if definition.risk != ToolRisk::Medium && definition.risk != ToolRisk::High {
            continue;
        }
        // Only literal arms; dynamic provider tools are out of scope.
        let literal_arm = format!("\"{name}\" =>");
        if !source.contains(&literal_arm) {
            continue;
        }
        let gate_call = format!("self.authorize_builtin(\"{name}\")");
        assert!(
            source.contains(&gate_call),
            "dispatcher arm for Medium/High tool '{name}' must call the built-in gate"
        );
    }
}

/// Reads dispatcher.rs source and extracts every literal match-arm tool
/// name from the `call_tool` dispatcher, mirroring how the real dispatch
/// routes static tools. A literal arm is `"tool.name" =>` at match-arm
/// position inside `call_tool`.
fn static_dispatcher_arms() -> Vec<String> {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/mcp/dispatcher.rs"
    ))
    .expect("read dispatcher.rs");
    let mut arms = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        // Match arms look like `"tool.name" => {` or `"tool.name" => expr,`
        if let Some(rest) = trimmed.strip_prefix('"') {
            if let Some((name, after)) = rest.split_once('"') {
                if after.trim_start().starts_with("=>") && name.contains('.') {
                    arms.push(name.to_string());
                }
            }
        }
    }
    arms
}

// ---------------------------------------------------------------------------
// 4. CLI round-trip: the `awh mcp trust awh.builtin` surface
// ---------------------------------------------------------------------------

#[test]
fn trust_cli_writes_a_record_the_gate_enforces() {
    use std::process::Command;
    let dir = tempdir().expect("tempdir");
    let trust_dir = dir.path().join("trust");
    std::fs::create_dir_all(&trust_dir).expect("create trust dir");

    // No flags: least-privilege Reviewed record granting nothing. The
    // capability flags must be rejected for ordinary server ids.
    let output = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(["mcp", "trust", "awh.builtin"])
        .env("AWH_TRUST_DIR", &trust_dir)
        .current_dir(dir.path())
        .output()
        .expect("spawn awh");
    assert!(
        output.status.success(),
        "awh mcp trust awh.builtin failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("trusted MCP: awh.builtin"),
        "got: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    // The record the gate reads: persisted under the (isolated) trust dir.
    let store = PersistentTrustStore::new(&trust_dir).expect("load store");
    let gate_store = PersistentTrustStore::from_store(&store.to_store());
    assert_eq!(
        authorize_builtin_tool("terminal.run", Some(&gate_store)),
        Err(BuiltinToolAuthorizationError::PermissionDenied {
            tool: "terminal.run".to_string(),
            permission: Permission::Process,
            id: BUILTIN_TOOL_TRUST_ID.to_string(),
        }),
        "the CLI-written record must restrict the built-in gate"
    );
    // `git --version` (a process tool) is denied but the record exists, so
    // the tool name is named and the cause is the missing process grant.
    assert!(audited("deny", "builtin_tool_denied", "terminal.run"));
}

#[test]
fn trust_cli_capability_flags_rejected_for_other_ids() {
    use std::process::Command;
    let dir = tempdir().expect("tempdir");
    let trust_dir = dir.path().join("trust");
    std::fs::create_dir_all(&trust_dir).expect("create trust dir");

    let output = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(["mcp", "trust", "some-server", "--network"])
        .env("AWH_TRUST_DIR", &trust_dir)
        .current_dir(dir.path())
        .output()
        .expect("spawn awh");
    assert!(
        !output.status.success(),
        "capability flags must be rejected for non-reserved ids"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("only valid for the reserved id"),
        "got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn trust_cli_full_grant_enables_the_whole_surface() {
    use std::process::Command;
    let dir = tempdir().expect("tempdir");
    let trust_dir = dir.path().join("trust");
    std::fs::create_dir_all(&trust_dir).expect("create trust dir");

    let output = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args([
            "mcp",
            "trust",
            "awh.builtin",
            "--network",
            "--process",
            "--filesystem",
        ])
        .env("AWH_TRUST_DIR", &trust_dir)
        .current_dir(dir.path())
        .output()
        .expect("spawn awh");
    assert!(
        output.status.success(),
        "full-grant trust failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let store = PersistentTrustStore::new(&trust_dir).expect("load store");
    let gate_store = PersistentTrustStore::from_store(&store.to_store());
    for tool in [
        "terminal.run",
        "git.commit",
        "workspace.write_file",
        "workspace.delete_file",
        "memory.store",
        "github.pr_create",
    ] {
        assert_eq!(
            authorize_builtin_tool(tool, Some(&gate_store)),
            Ok(()),
            "{tool} must be allowed under the full grant"
        );
    }
}

#[test]
fn block_cli_blocks_the_builtin_surface() {
    use std::process::Command;
    let dir = tempdir().expect("tempdir");
    let trust_dir = dir.path().join("trust");
    std::fs::create_dir_all(&trust_dir).expect("create trust dir");

    let output = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(["mcp", "block", "awh.builtin"])
        .env("AWH_TRUST_DIR", &trust_dir)
        .current_dir(dir.path())
        .output()
        .expect("spawn awh");
    assert!(
        output.status.success(),
        "awh mcp block awh.builtin failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let store = PersistentTrustStore::new(&trust_dir).expect("load store");
    let gate_store = PersistentTrustStore::from_store(&store.to_store());
    // The CLI-written Blocked record grants no capabilities, so the denial
    // names both the tool and the first missing permission.
    assert_eq!(
        authorize_builtin_tool("terminal.run", Some(&gate_store)),
        Err(BuiltinToolAuthorizationError::PermissionDenied {
            tool: "terminal.run".to_string(),
            permission: Permission::Process,
            id: BUILTIN_TOOL_TRUST_ID.to_string(),
        })
    );
}
