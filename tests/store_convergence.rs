//! Service/store convergence integration tests (ARCH-001, Prompt 15).
//!
//! One canonical persistence boundary means MCP, the Control API, the TUI
//! backends, and the CLI all read and write the *same* authoritative files
//! through the *same* canonical types — never interface-local duplicates.
//! These tests pin that from the outside: state written through one plane's
//! entry point must be observable through every other plane and survive a
//! process-style "restart" (a fresh store instance over the same root).

use agent_workspace_hub::core::memory::{MemoryEntry, MemoryScope, MemoryStore};
use agent_workspace_hub::core::tasks::{Task, TaskPriority, TaskStatus, TaskStore};
use agent_workspace_hub::mcp::{
    DispatchResult, McpDispatcher, McpPermissions, MemoryMcp, PersistentTrustStore,
    SessionLifecycle, TasksMcp, TrustLevel, TrustStore, WorkspaceMcp, BUILTIN_TOOL_TRUST_ID,
};
use agent_workspace_hub::services::files::FilesService;
use agent_workspace_hub::services::init::initialize_workspace;
use serde_json::{json, Value};
use std::path::Path;

/// The full-grant `awh.builtin` trust record so the coarse built-in gate
/// (High-risk default-deny) does not shadow the convergence behavior this
/// file isolates. The default-deny itself is pinned in
/// tests/mcp_builtin_tool_gate.rs.
fn full_grant_builtin_trust() -> PersistentTrustStore {
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

fn request(id: Value, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

async fn dispatch(dispatcher: &McpDispatcher, input: &str, lifecycle: &SessionLifecycle) -> Value {
    match dispatcher.dispatch_with_lifecycle(input, lifecycle).await {
        DispatchResult::Response(resp) => serde_json::to_value(&resp).expect("serialize"),
        DispatchResult::NoResponse => Value::Null,
    }
}

/// Calls one tool via a freshly initialized dispatcher session and returns
/// the raw JSON-RPC response value (asserting the call produced a result).
async fn ready_call(root: &Path, tool: &str, args: Value) -> Value {
    let dispatcher = McpDispatcher::new_async(root.to_path_buf())
        .await
        .expect("dispatcher")
        .with_trust_store(full_grant_builtin_trust());
    let lifecycle = SessionLifecycle::default();

    let init = dispatch(
        &dispatcher,
        &request(
            json!(1),
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "store-convergence-test", "version": "0.0.1"}
            }),
        ),
        &lifecycle,
    )
    .await;
    assert!(init["result"].is_object(), "initialize failed: {init}");

    let input = request(
        json!(91),
        "tools/call",
        json!({"name": tool, "arguments": args}),
    );
    let response = dispatch(&dispatcher, &input, &lifecycle).await;
    assert!(
        response["result"].is_object(),
        "tools/call {tool} failed: {response}"
    );
    response
}

/// Extracts the single text content payload from a `tools/call` result.
fn text_payload(response: &Value) -> String {
    response["result"]["content"][0]["text"]
        .as_str()
        .expect("content text")
        .to_owned()
}

fn init_workspace(root: &Path) {
    initialize_workspace(root).expect("workspace initializes");
}

#[tokio::test]
async fn memory_written_by_the_store_is_visible_to_mcp_and_survives_restart() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    init_workspace(&root);

    // Write through the canonical store directly (what the CLI/API/TUI do).
    let entry = MemoryStore::for_project(&root)
        .unwrap()
        .append("kernel: the scheduler requeues on EAGAIN")
        .unwrap();

    // Read through the MCP plane: the dispatcher's memory tools must see it.
    let response = ready_call(&root, "memory.get", json!({"id": entry.id})).await;
    let seen: MemoryEntry = serde_json::from_str(&text_payload(&response)).unwrap();
    assert_eq!(seen.id, entry.id);
    assert_eq!(seen.content, "kernel: the scheduler requeues on EAGAIN");

    // Update through MCP, then verify through a brand-new store instance
    // (a "restarted process") reading the same persisted file.
    let response = ready_call(
        &root,
        "memory.update",
        json!({
            "id": entry.id,
            "content": "kernel: updated via MCP",
            "scope": "Project",
            "tags": ["kernel"]
        }),
    )
    .await;
    let updated: MemoryEntry = serde_json::from_str(&text_payload(&response)).unwrap();
    assert_eq!(updated.content, "kernel: updated via MCP");

    let restarted = MemoryStore::for_project(&root).unwrap();
    assert_eq!(
        restarted.get(&entry.id).unwrap().unwrap().content,
        "kernel: updated via MCP"
    );
    assert!(root.join(".agent/memory.json").is_file());
}

#[tokio::test]
async fn memory_scope_rides_the_canonical_entry_not_a_second_store() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    init_workspace(&root);

    // Pre-seed one entry through the canonical store (what the CLI/API/TUI
    // do), then write a Global-scoped entry through the MCP plane.
    let store = MemoryStore::for_project(&root).unwrap();
    let local = store.append("project-local note").unwrap();

    let response = ready_call(
        &root,
        "memory.store",
        json!({
            "id": "note-global",
            "content": "global note",
            "scope": "Global",
            "tags": ["shared"]
        }),
    )
    .await;
    let stored: MemoryEntry = serde_json::from_str(&text_payload(&response)).unwrap();
    assert_eq!(stored.scope, MemoryScope::Global);
    assert_eq!(stored.tags, vec!["shared".to_string()]);

    // Convergence: the MCP-written entry lives in the SAME canonical file
    // the service plane reads — scope is a field on the canonical entry,
    // not a second parallel store. The core store must see both entries
    // with their scopes intact.
    let project_entries = store.list_all().unwrap();
    assert_eq!(project_entries.len(), 2);
    assert_eq!(project_entries[0].id, local.id);
    assert_eq!(project_entries[0].scope, MemoryScope::Project);
    assert_eq!(project_entries[1].id, "note-global");
    assert_eq!(project_entries[1].scope, MemoryScope::Global);

    // MCP still finds the entry it wrote, and a core-store delete is
    // immediately visible to a subsequent MCP read of the same id.
    let response = ready_call(&root, "memory.get", json!({"id": "note-global"})).await;
    let entry: MemoryEntry = serde_json::from_str(&text_payload(&response)).unwrap();
    assert_eq!(entry.content, "global note");

    assert!(store.delete("note-global").unwrap());
    let response = ready_call(&root, "memory.get", json!({"id": "note-global"})).await;
    assert_eq!(
        text_payload(&response),
        "null",
        "deleted entries must read as null, not an error"
    );
}

#[tokio::test]
async fn tasks_created_by_the_cli_store_are_served_by_mcp_tools() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    init_workspace(&root);

    // Create through the canonical store (what the CLI's `awh tasks` path uses).
    let created = TaskStore::new(&root)
        .unwrap()
        .create(
            "metrics-endpoint".to_string(),
            "wire the metrics endpoint".to_string(),
            "Expose /metrics from the runtime".to_string(),
            TaskPriority::High,
            vec!["infra".to_string()],
        )
        .unwrap();

    // The MCP task tools must serve the same record.
    let response = ready_call(&root, "tasks.get", json!({"id": created.id})).await;
    let task: Task = serde_json::from_str(&text_payload(&response)).unwrap();
    assert_eq!(task.title, "wire the metrics endpoint");
    assert_eq!(task.status, TaskStatus::Todo);
    assert_eq!(task.priority, TaskPriority::High);

    // Update status through MCP, read through a fresh store (restart).
    ready_call(
        &root,
        "tasks.update",
        json!({"id": created.id, "status": "InProgress"}),
    )
    .await;
    let restarted = TaskStore::new(&root).unwrap();
    let reloaded = restarted.get(&created.id).unwrap().unwrap();
    assert_eq!(reloaded.status, TaskStatus::InProgress);
}

#[test]
fn file_writes_converge_on_one_atomic_path_for_mcp_and_the_service() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    init_workspace(&root);

    // Write through the canonical service (Control API / TUI path)…
    FilesService::new(&root)
        .write_atomic("docs/plan.md", "service-written")
        .unwrap();

    // …read through the MCP workspace adapter and verify the delegation
    // produced byte-identical state.
    let mcp = WorkspaceMcp::new(&root).unwrap();
    assert_eq!(mcp.read_file("docs/plan.md").unwrap(), "service-written");

    // Write through MCP, read through the service: same file, same bytes.
    mcp.write_file("docs/plan.md", "mcp-written").unwrap();
    assert_eq!(
        FilesService::new(&root).read("docs/plan.md").unwrap(),
        "mcp-written"
    );

    // MCP delete reports existence and the service sees the removal.
    assert!(mcp.delete_file("docs/plan.md").unwrap());
    assert!(!mcp.delete_file("docs/plan.md").unwrap());
    assert!(FilesService::new(&root).read("docs/plan.md").is_err());
}

#[test]
fn memory_store_migrates_legacy_jsonl_once() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    init_workspace(&root);

    // Seed the legacy layout: one JSON object per line, no ids.
    let legacy = root.join(".agent");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(
        legacy.join("memory.jsonl"),
        concat!(
            "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"content\":\"first legacy note\"}\n",
            "{\"timestamp\":\"2026-01-02T03:05:05Z\",\"content\":\"second legacy note\"}\n"
        ),
    )
    .unwrap();

    let store = MemoryStore::for_project(&root).unwrap();
    let entries = store.list_all().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| !e.id.is_empty()));
    // Migration preserves the legacy file's line order (insertion order,
    // the same ordering `list_all` uses for live appends).
    assert_eq!(entries[0].content, "first legacy note");
    assert_eq!(entries[1].content, "second legacy note");

    // Migration is destructive-once: the legacy file is gone, so a second
    // constructor does not duplicate entries.
    assert!(!legacy.join("memory.jsonl").exists());
    let again = MemoryStore::for_project(&root).unwrap();
    assert_eq!(again.list_all().unwrap().len(), 2);
}

#[tokio::test]
async fn legacy_migrated_notes_are_searchable_over_mcp() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    init_workspace(&root);

    let legacy = root.join(".agent");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(
        legacy.join("memory.jsonl"),
        "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"content\":\"first legacy note\"}\n",
    )
    .unwrap();

    let response = ready_call(&root, "memory.search", json!({"query": "legacy"})).await;
    let hits: Vec<MemoryEntry> = serde_json::from_str(&text_payload(&response)).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].content, "first legacy note");
    assert!(!hits[0].id.is_empty());
}

#[test]
fn mcp_type_aliases_are_the_canonical_core_types() {
    // The MCP re-export shims must stay aliases of the canonical core
    // stores — not parallel implementations. The canonical type cannot
    // name the alias (that would be a backwards import), so this pins the
    // observable behavior instead: an entry stored via the alias type (the
    // dispatcher's field type) is served by the core store over the same
    // file, proving they are one store, not two.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    init_workspace(&root);

    let via_alias: MemoryMcp = MemoryMcp::new(&root).unwrap();
    via_alias
        .append("written through the MCP alias type")
        .unwrap();

    let via_core = MemoryStore::for_project(&root).unwrap();
    let entries = via_core.list_all().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].content, "written through the MCP alias type");

    let tasks_via_alias: TasksMcp = TasksMcp::new(&root).unwrap();
    tasks_via_alias
        .create(
            "alias-task".to_string(),
            "alias task".to_string(),
            String::new(),
            TaskPriority::Normal,
            Vec::new(),
        )
        .unwrap();
    let tasks_via_core = TaskStore::new(&root).unwrap();
    assert_eq!(tasks_via_core.list(None).unwrap().len(), 1);
}
