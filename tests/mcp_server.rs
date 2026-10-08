//! End-to-end integration tests for the stdio JSON-RPC MCP server.
//!
//! These exercise the real [`StdioMcpServer`] against real backing stores
//! (memory, tasks, connectors, workspace) persisted under a temp project root,
//! verifying full request/response round trips and durable side effects.

use agent_workspace_hub::mcp::StdioMcpServer;
use serde_json::{json, Value};
use tempfile::tempdir;

/// Sends a single JSON-RPC request to the server and decodes the response body.
///
/// The server enforces the MCP initialization lifecycle (spec §8): every
/// session must complete the `initialize` exchange before other requests.
/// This helper mirrors what every real MCP client does — initialize once,
/// then issue the request under test.
fn rpc(server: &StdioMcpServer, method: &str, params: Value) -> Value {
    let response = rpc_raw(server, 1, method, params);
    response
        .get("result")
        .cloned()
        .unwrap_or_else(|| panic!("no result in response: {response}"))
}

/// Sends a single JSON-RPC request with a chosen id and returns the raw
/// response object (so error envelopes can be inspected).
fn rpc_raw(server: &StdioMcpServer, id: i64, method: &str, params: Value) -> Value {
    // `initialize` is the request that opens the session; everything else
    // requires an initialized session per the MCP lifecycle.
    if method != "initialize" && !server.is_initialized() {
        let _ = rpc_once(server, 0, "initialize", json!({}));
    }
    rpc_once(server, id, method, params)
}

/// Completes the `initialize` exchange so subsequent raw requests model an
/// established MCP session.
fn init(server: &StdioMcpServer) {
    let response = rpc_once(server, 0, "initialize", json!({}));
    assert!(
        response.get("error").is_none(),
        "initialize failed: {response}"
    );
}

/// Sends a single JSON-RPC request and decodes the full response envelope
/// (result or error). Uses the resilient handler so protocol failures come
/// back as JSON-RPC error objects, exactly like the serve loop delivers them.
fn rpc_once(server: &StdioMcpServer, id: i64, method: &str, params: Value) -> Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    serde_json::from_str(&server.handle_response(&request.to_string())).unwrap()
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
    init(&server);

    // A name without a dot hits the "unknown tool" fallthrough and now returns
    // a JSON-RPC error (-32602 invalid params) rather than a success envelope
    // containing an error string, in line with MCP conformance.
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "doesnotexist", "arguments": {}}
    });
    let response: Value =
        serde_json::from_str(&server.handle_response(&request.to_string())).unwrap();
    let error = response
        .get("error")
        .expect("unknown tool must yield an error");
    assert_eq!(error["code"], -32602);
    assert!(
        error["message"].as_str().unwrap().contains("unknown tool"),
        "got: {error}"
    );
}

#[test]
fn unknown_qualified_provider_tool_fails_closed() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();
    init(&server);
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

#[test]
fn malformed_json_returns_parse_error_code() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();
    let response: Value = serde_json::from_str(&server.handle_response("{ not json")).unwrap();
    assert_eq!(response["error"]["code"], -32700);
}

#[test]
fn unsupported_jsonrpc_version_returns_invalid_request_code() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();
    let request = json!({"jsonrpc": "1.0", "id": 1, "method": "initialize", "params": {}});
    let response: Value =
        serde_json::from_str(&server.handle_response(&request.to_string())).unwrap();
    assert_eq!(response["error"]["code"], -32600);
}

#[test]
fn unknown_method_returns_method_not_found_code() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();
    init(&server);
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "bogus/method", "params": {}});
    let response: Value =
        serde_json::from_str(&server.handle_response(&request.to_string())).unwrap();
    assert_eq!(response["error"]["code"], -32601);
}

#[test]
fn pre_initialize_request_is_rejected_with_32002() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();

    // tools/call before initialize must be refused (-32002), not dispatched:
    // this is the end-to-end contract of the MCP initialization lifecycle.
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "skills.list", "arguments": {}}
    });
    let response: Value =
        serde_json::from_str(&server.handle_response(&request.to_string())).unwrap();
    assert_eq!(response["error"]["code"], -32002);
    assert!(!server.is_initialized());

    // The connection survives: initialize then retries the same call fine.
    init(&server);
    let ok = rpc(
        &server,
        "tools/call",
        json!({"name": "skills.list", "arguments": {}}),
    );
    assert!(ok["content"].is_array(), "got: {ok}");
}

#[test]
fn duplicate_initialize_is_a_protocol_error() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();
    init(&server);

    let response = rpc_raw(&server, 2, "initialize", json!({}));
    assert_eq!(response["error"]["code"], -32600, "got: {response}");
    // The established session stays usable after the duplicate.
    let ok = rpc_raw(&server, 3, "ping", json!({}));
    assert!(ok.get("error").is_none(), "got: {ok}");
}

#[test]
fn initialize_with_bad_params_is_rejected_and_retryable() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();

    // A non-string protocolVersion is a parameter error, not a version miss.
    let bad = rpc_raw(&server, 1, "initialize", json!({"protocolVersion": 3}));
    assert_eq!(bad["error"]["code"], -32602, "got: {bad}");
    assert!(!server.is_initialized());

    // Retry with correct params opens the session.
    init(&server);
    assert!(server.is_initialized());
}

#[test]
fn version_negotiation_responds_with_supported_version() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();

    // Requesting an (unsupported) protocol version returns the server's
    // latest supported version — the client decides whether to continue.
    let old = rpc_raw(
        &server,
        1,
        "initialize",
        json!({"protocolVersion": "1999-01-01"}),
    );
    assert_eq!(old["result"]["protocolVersion"], "2025-06-18", "got: {old}");

    // Every advertised version is echoed back exactly (negotiation rule).
    for supported in agent_workspace_hub::mcp::SUPPORTED_PROTOCOL_VERSIONS {
        let dir2 = tempdir().unwrap();
        let server2 = StdioMcpServer::new(dir2.path().to_path_buf()).unwrap();
        let exact = rpc_raw(
            &server2,
            1,
            "initialize",
            json!({"protocolVersion": supported}),
        );
        assert_eq!(
            exact["result"]["protocolVersion"], supported,
            "supported version must be echoed, got: {exact}"
        );
        assert!(exact["result"]["capabilities"]["resources"].is_object());
        assert!(exact["result"]["capabilities"]["prompts"].is_object());
    }
}

#[test]
fn prompts_get_is_a_deterministic_unknown_prompt_error() {
    let dir = tempdir().unwrap();
    let server = StdioMcpServer::new(dir.path().to_path_buf()).unwrap();
    init(&server);

    let response = rpc_raw(&server, 1, "prompts/get", json!({"name": "code_review"}));
    assert_eq!(response["error"]["code"], -32602, "got: {response}");
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown prompt"),
        "got: {response}"
    );
}
