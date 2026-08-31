//! End-to-end integration tests for the stdio JSON-RPC MCP server.
//!
//! These exercise the real [`StdioMcpServer`] against real backing stores
//! (memory, tasks, connectors, workspace) persisted under a temp project root,
//! verifying full request/response round trips and durable side effects.

use agent_workspace_hub::mcp::StdioMcpServer;
use serde_json::{json, Value};
use tempfile::tempdir;

/// Sends a single JSON-RPC request to the server and decodes the response body.
fn rpc(server: &StdioMcpServer, method: &str, params: Value) -> Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let response: Value =
        serde_json::from_str(&server.handle(&request.to_string()).unwrap()).unwrap();
    response
        .get("result")
        .cloned()
        .unwrap_or_else(|| panic!("no result in response: {response}"))
}

#[test]
fn initialize_handshake_reports_server_capabilities() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();
    let result = rpc(&server, "initialize", json!({}));
    assert_eq!(result["serverInfo"]["name"], "agent-workspace-hub");
    assert_eq!(result["protocolVersion"], "2025-06-18");
    assert!(result["capabilities"]["tools"].is_object());
}

#[test]
fn tools_list_returns_full_catalog() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();
    let result = rpc(&server, "tools/list", json!({}));
    let names: Vec<&str> = result["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    for expected in [
        "skills.list",
        "workspace.list_files",
        "memory.store",
        "tasks.create",
        "connectors.list",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
}

#[test]
fn memory_store_then_get_round_trips_and_persists() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();

    rpc(
        &server,
        "tools/call",
        json!({
            "name": "memory.store",
            "arguments": {
                "id": "mem-1",
                "content": "remember the launch flags",
                "scope": "Project",
                "tags": ["launch"]
            }
        }),
    );

    // Verify in-memory read.
    let get = rpc(
        &server,
        "tools/call",
        json!({"name": "memory.get", "arguments": {"id": "mem-1"}}),
    );
    let text = get["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("remember the launch flags"), "got: {text}");

    // Verify durable persistence on disk.
    let raw = std::fs::read_to_string(dir.path().join(".agent").join("memory.json")).unwrap();
    assert!(raw.contains("mem-1"), "memory not persisted: {raw}");
}

#[test]
fn tasks_create_then_list_and_update() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();

    rpc(
        &server,
        "tools/call",
        json!({
            "name": "tasks.create",
            "arguments": {
                "id": "t-1",
                "title": "Ship the MCP server",
                "description": "integration tests first",
                "priority": "High"
            }
        }),
    );

    let list = rpc(
        &server,
        "tools/call",
        json!({"name": "tasks.list", "arguments": {}}),
    );
    let text = list["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("t-1"), "task not listed: {text}");

    rpc(
        &server,
        "tools/call",
        json!({
            "name": "tasks.update",
            "arguments": {"id": "t-1", "status": "Done"}
        }),
    );

    let done = rpc(
        &server,
        "tools/call",
        json!({"name": "tasks.list", "arguments": {"status": "Done"}}),
    );
    let done_text = done["content"][0]["text"].as_str().unwrap();
    assert!(
        done_text.contains("t-1"),
        "updated task not in Done list: {done_text}"
    );
}

#[test]
fn workspace_read_file_serves_real_file() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "hello from workspace").unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();

    let read = rpc(
        &server,
        "tools/call",
        json!({"name": "workspace.read_file", "arguments": {"path": "README.md"}}),
    );
    let text = read["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("hello from workspace"), "got: {text}");
}

#[test]
fn unknown_tool_returns_error_not_panic() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();

    // A name without a dot hits the "unknown tool" fallthrough and returns a
    // JSON error result rather than panicking.
    let result = rpc(
        &server,
        "tools/call",
        json!({"name": "doesnotexist", "arguments": {}}),
    );
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("unknown tool") || text.contains("error"),
        "got: {text}"
    );
}

#[test]
fn unknown_qualified_provider_tool_fails_closed() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();
    // A dotted, unregistered provider name must fail closed (Err), not panic.
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "missing.provider.tool", "arguments": {}}
    });
    assert!(server.handle(&request.to_string()).is_err());
}

#[test]
fn unsupported_jsonrpc_version_is_rejected() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();
    let request = json!({"jsonrpc": "1.0", "id": 1, "method": "initialize", "params": {}});
    assert!(server.handle(&request.to_string()).is_err());
}
