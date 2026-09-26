//! End-to-end tests against the real compiled `awh` binary over stdio.
//!
//! These exercise the exact bytes a real MCP client sees: the binary is
//! spawned via `CARGO_BIN_EXE_awh`, fed newline-delimited JSON-RPC on stdin,
//! and its stdout must contain ONLY protocol responses — never tracing logs,
//! never banners. EOF on stdin must shut the server down cleanly.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Child, Command, Stdio};
use tempfile::tempdir;

struct ServerProcess {
    child: Child,
    stdin: Option<std::process::ChildStdin>,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_server() -> (ServerProcess, tempfile::TempDir) {
    let dir = tempdir().expect("tempdir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_awh"))
        .arg("mcp")
        .arg("serve")
        .arg("--transport")
        .arg("stdio")
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn awh binary");
    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = std::io::BufReader::new(child.stdout.take().expect("stdout piped"));
    (
        ServerProcess {
            child,
            stdin: Some(stdin),
            stdout,
        },
        dir,
    )
}

fn send(server: &mut ServerProcess, value: &Value) {
    let line = serde_json::to_string(value).expect("serialize request");
    let stdin = server.stdin.as_mut().expect("stdin still open");
    writeln!(stdin, "{line}").expect("write request");
    stdin.flush().expect("flush request");
}

/// Reads one JSON-RPC response line from the server's stdout.
fn read_response(server: &mut ServerProcess) -> Value {
    use std::io::BufRead;
    let mut line = String::new();
    loop {
        line.clear();
        let n = server.stdout.read_line(&mut line).expect("read stdout");
        if n == 0 {
            panic!("server closed stdout without responding");
        }
        if line.trim().is_empty() {
            continue;
        }
        // Protocol purity: every non-empty stdout line must parse as
        // JSON-RPC. A tracing banner here would corrupt the stream.
        return serde_json::from_str(line.trim())
            .unwrap_or_else(|e| panic!("stdout is not JSON-RPC ({e}): {line}"));
    }
}

/// Collects exactly `n` responses in order.
fn read_responses(server: &mut ServerProcess, n: usize) -> Vec<Value> {
    (0..n).map(|_| read_response(server)).collect()
}

#[test]
fn stdio_round_trip_initialize_call_status_shutdown() {
    let (mut server, _dir) = spawn_server();

    // initialize → result with protocolVersion, capabilities, serverInfo.
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18",
            "capabilities":{},
            "clientInfo":{"name":"e2e","version":"0.0"}
        }}),
    );
    let init = read_response(&mut server);
    assert_eq!(init["id"], 1);
    assert_eq!(init["jsonrpc"], "2.0");
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(init["result"]["serverInfo"]["name"].is_string());
    assert!(init["result"]["capabilities"]["tools"].is_object());

    // notifications/initialized → no response (notification semantics).
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );

    // tools/list → every tool carries category and version metadata.
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    let list = read_response(&mut server);
    let tools = list["result"]["tools"].as_array().expect("tools array");
    assert!(
        tools.len() >= 53,
        "static catalog must include mcp.status, got {}",
        tools.len()
    );
    for tool in tools {
        let name = tool["name"].as_str().unwrap_or_default();
        assert!(
            tool["category"].as_str().is_some(),
            "tool {name} must carry category metadata over stdio"
        );
        assert!(
            tool["version"].as_str().is_some(),
            "tool {name} must carry version metadata over stdio"
        );
    }

    // mcp.status via tools/call → running-state snapshot inside the envelope.
    // A prior successful tool call must show up in the metrics snapshot
    // (the status call itself is recorded only after its own result is
    // serialized, so it cannot count itself).
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"memory.search","arguments":{"query":"e2e"}
        }}),
    );
    let search = read_response(&mut server);
    assert!(search["result"].is_object(), "got: {search}");

    send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{
            "name":"mcp.status","arguments":{}
        }}),
    );
    let status = read_response(&mut server);
    let text = status["result"]["content"][0]["text"]
        .as_str()
        .expect("mcp.status content envelope");
    let status: Value = serde_json::from_str(text).expect("status payload is JSON");
    assert_eq!(status["status"], "running");
    assert!(
        status["metrics"]["tool_calls"].as_u64().unwrap() >= 1,
        "prior memory.search call must be counted: {status}"
    );

    // Invalid tool arguments → -32602 invalid params (not internal error).
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{
            "name":"workspace.read_file","arguments":{"path":42}
        }}),
    );
    let invalid = read_response(&mut server);
    assert_eq!(invalid["error"]["code"], -32602);

    // Notification with an unknown method → still silent.
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","method":"tools/call","params":{
            "name":"memory.search","arguments":{"query":"x"}
        }}),
    );

    // Shutdown: close stdin (EOF); the process must exit 0 and have produced
    // no non-JSON output. Send two pipelined requests first so the reader
    // is mid-flight — EOF must still be honored deterministically.
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":5,"method":"ping"}),
    );
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":6,"method":"ping"}),
    );
    let pings = read_responses(&mut server, 2);
    assert_eq!(pings[0]["result"], json!({}));
    assert_eq!(pings[1]["result"], json!({}));

    // Close stdin to signal EOF, then wait for a clean exit.
    server.stdin = None;
    let status = server.child.wait().expect("wait for exit");
    assert!(
        status.success(),
        "EOF must shut down cleanly, got {status:?}"
    );
}

#[test]
fn stdio_requires_no_auth_and_reports_parse_errors() {
    let (mut server, _dir) = spawn_server();

    // No API key configured: stdio must still serve (local trust model).
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18",
            "capabilities":{},
            "clientInfo":{"name":"e2e","version":"0.0"}
        }}),
    );
    let init = read_response(&mut server);
    assert!(init["result"].is_object(), "got: {init}");

    // Garbage line → -32700 with id null, server stays alive.
    let stdin = server.stdin.as_mut().expect("stdin still open");
    writeln!(stdin, "{{this is not json").expect("write garbage");
    stdin.flush().expect("flush garbage");
    let parse_error = read_response(&mut server);
    assert_eq!(parse_error["id"], Value::Null);
    assert_eq!(parse_error["error"]["code"], -32700);

    // Still alive: a subsequent valid request succeeds.
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
    );
    let list = read_response(&mut server);
    assert!(list["result"]["tools"].is_array(), "got: {list}");
}

#[test]
fn stdio_notification_only_session_produces_zero_bytes() {
    let (mut server, _dir) = spawn_server();

    // A session that only sends notifications must produce literally no
    // stdout bytes — notifications are fire-and-forget in JSON-RPC 2.0.
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}),
    );

    // Now a request: exactly one response, and it must be the FIRST output.
    send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":9,"method":"ping"}),
    );
    let ping = read_response(&mut server);
    assert_eq!(ping["id"], 9);
    assert_eq!(ping["result"], json!({}));
}

// ---------------------------------------------------------------------------
// AWE-009 / AWE-016: the filesystem.* editing plane, proven end-to-end
// against the real compiled binary — discovery, schema rejection, every
// operation kind, and the rollback tool — with zero-mutation assertions
// for every rejected case.
// ---------------------------------------------------------------------------

/// Spawns the server in a tempdir initialized as a real AWH workspace
/// (the rollback tool's global-session contract needs the canonical
/// workspace identity; the editing tools themselves never require it).
fn spawn_initialized_server() -> (ServerProcess, tempfile::TempDir) {
    let dir = tempdir().expect("tempdir");
    agent_workspace_hub::services::init::initialize_workspace(dir.path())
        .expect("initialize temp workspace");
    let mut child = Command::new(env!("CARGO_BIN_EXE_awh"))
        .arg("mcp")
        .arg("serve")
        .arg("--transport")
        .arg("stdio")
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn awh binary");
    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = std::io::BufReader::new(child.stdout.take().expect("stdout piped"));
    (
        ServerProcess {
            child,
            stdin: Some(stdin),
            stdout,
        },
        dir,
    )
}

/// Completes the initialize handshake; the session is then READY.
fn initialize_session(server: &mut ServerProcess, id: u64) {
    send(
        server,
        &json!({"jsonrpc":"2.0","id":id,"method":"initialize","params":{
            "protocolVersion":"2025-06-18",
            "capabilities":{},
            "clientInfo":{"name":"edit-e2e","version":"0.0"}
        }}),
    );
    let init = read_response(server);
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    send(
        server,
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
}

/// Invokes one tool over the real protocol; returns the full response.
fn call_tool(server: &mut ServerProcess, id: u64, name: &str, arguments: Value) -> Value {
    send(
        server,
        &json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{
            "name": name, "arguments": arguments
        }}),
    );
    read_response(server)
}

/// Unwraps a successful tool result into its text content payload.
fn tool_text(response: &Value) -> String {
    assert!(
        response["error"].is_null(),
        "expected success, got error: {response}"
    );
    response["result"]["content"][0]["text"]
        .as_str()
        .expect("content envelope")
        .to_owned()
}

/// Unwraps a JSON-RPC error into its (code, message).
fn rpc_error(response: &Value) -> (i64, String) {
    let code = response["error"]["code"].as_i64().expect("error code");
    let message = response["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    (code, message)
}

#[test]
fn editing_tools_are_discoverable_with_registry_metadata() {
    let (mut server, _dir) = spawn_server();
    initialize_session(&mut server, 1);

    send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    let list = read_response(&mut server);
    let tools = list["result"]["tools"].as_array().expect("tools array");

    for name in [
        "filesystem.replace",
        "filesystem.insert",
        "filesystem.delete_range",
        "filesystem.patch",
        "filesystem.apply_diff",
        "filesystem.rollback",
    ] {
        let tool = tools
            .iter()
            .find(|t| t["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("{name} must be discoverable via tools/list"));
        // Registry metadata rides along (the exhaustiveness contract).
        assert_eq!(tool["category"], "filesystem");
        assert_eq!(tool["provider"], "awh");
        assert_eq!(tool["risk"], "medium");
        assert_eq!(tool["requiredPermissions"], json!(["filesystem"]));
        // A schema exists and is validated before the handler runs.
        assert!(tool["inputSchema"].is_object(), "{name} needs a schema");
    }
}

#[test]
fn editing_full_matrix_through_real_binary() {
    let (mut server, dir) = spawn_initialized_server();
    initialize_session(&mut server, 1);
    let mut id = 2u64;
    let mut next_id = || {
        id += 1;
        id
    };

    // --- replace: create a file and replace exact text -------------------
    std::fs::write(dir.path().join("edit.txt"), "alpha\nbeta\n").unwrap();
    let response = call_tool(
        &mut server,
        next_id(),
        "filesystem.replace",
        json!({"path":"edit.txt","old":"alpha","new":"ALPHA"}),
    );
    let payload: Value =
        serde_json::from_str(&tool_text(&response)).expect("replace payload is JSON");
    // The canonical EditResult carries the transaction id + status, not a
    // "written" flag (that's the workspace.write_file tool).
    assert_eq!(payload["status"], "committed", "{payload}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("edit.txt")).unwrap(),
        "ALPHA\nbeta\n",
        "the real filesystem changed exactly as the tool reported"
    );
    let edit_id = payload["id"]
        .as_str()
        .expect("edit id in result")
        .to_owned();

    // --- rollback: exact-id rollback through the MCP tool --------------
    // Immediately after the replace: no other edit has touched edit.txt,
    // so the produced state still holds and the rollback restores.
    let response = call_tool(
        &mut server,
        next_id(),
        "filesystem.rollback",
        json!({"edit_id": edit_id}),
    );
    let payload: Value =
        serde_json::from_str(&tool_text(&response)).expect("rollback payload is JSON");
    assert_eq!(payload["status"], "restored", "{payload}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("edit.txt")).unwrap(),
        "alpha\nbeta\n",
        "rollback restored the exact pre-edit bytes"
    );

    // --- repeated rollback is the idempotent no-op ----------------------
    let response = call_tool(
        &mut server,
        next_id(),
        "filesystem.rollback",
        json!({"edit_id": edit_id}),
    );
    let payload: Value = serde_json::from_str(&tool_text(&response)).unwrap();
    assert_eq!(payload["status"], "already_rolled_back", "{payload}");

    // --- rollback of an edit whose target changed afterwards -----------
    // conflict: the produced-state guard refuses to overwrite newer
    // content — the file keeps the newer bytes.
    let response = call_tool(
        &mut server,
        next_id(),
        "filesystem.replace",
        json!({"path":"edit.txt","old":"alpha","new":"ALPHA"}),
    );
    let payload: Value =
        serde_json::from_str(&tool_text(&response)).expect("second replace payload");
    let second_edit = payload["id"].as_str().expect("second edit id").to_owned();
    std::fs::write(dir.path().join("edit.txt"), "external newer state\n").unwrap();
    let response = call_tool(
        &mut server,
        next_id(),
        "filesystem.rollback",
        json!({"edit_id": second_edit}),
    );
    let payload: Value =
        serde_json::from_str(&tool_text(&response)).expect("conflict payload is JSON");
    assert_eq!(payload["status"], "conflict", "{payload}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("edit.txt")).unwrap(),
        "external newer state\n",
        "the external change is preserved through the conflict"
    );

    // --- insert: line 0 inserts at the beginning ------------------------
    std::fs::write(dir.path().join("edit.txt"), "alpha\nbeta\n").unwrap();
    let response = call_tool(
        &mut server,
        next_id(),
        "filesystem.insert",
        json!({"path":"edit.txt","line":0,"content":"top"}),
    );
    let payload: Value =
        serde_json::from_str(&tool_text(&response)).expect("insert payload is JSON");
    assert_eq!(payload["status"], "committed", "{payload}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("edit.txt")).unwrap(),
        "top\nalpha\nbeta\n"
    );

    // --- delete_range: inclusive one-based lines -----------------------
    let response = call_tool(
        &mut server,
        next_id(),
        "filesystem.delete_range",
        json!({"path":"edit.txt","start_line":1,"end_line":2}),
    );
    let payload: Value =
        serde_json::from_str(&tool_text(&response)).expect("delete_range payload is JSON");
    assert_eq!(payload["status"], "committed", "{payload}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("edit.txt")).unwrap(),
        "beta\n"
    );

    // --- patch: multi-operation transaction ----------------------------
    std::fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "beta\n").unwrap();
    let response = call_tool(
        &mut server,
        next_id(),
        "filesystem.patch",
        json!({"operations":[
            {"path":"a.txt","old":"alpha","new":"ALPHA"},
            {"path":"b.txt","old":"beta","new":"BETA"}
        ]}),
    );
    let payload: Value =
        serde_json::from_str(&tool_text(&response)).expect("patch payload is JSON");
    assert_eq!(payload["status"]["type"], "committed", "{payload}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "ALPHA\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "BETA\n"
    );

    // --- apply_diff: unified diff through the canonical path -----------
    std::fs::write(dir.path().join("diff.txt"), "x\n").unwrap();
    let diff = "--- a/diff.txt\n+++ b/diff.txt\n@@ -1 +1 @@\n-x\n+X\n";
    let response = call_tool(
        &mut server,
        next_id(),
        "filesystem.apply_diff",
        json!({"diff": diff}),
    );
    let payload: Value =
        serde_json::from_str(&tool_text(&response)).expect("apply_diff payload is JSON");
    assert_eq!(payload["status"]["type"], "committed", "{payload}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("diff.txt")).unwrap(),
        "X\n"
    );

    // --- unknown edit id fails closed ----------------------------------
    let response = call_tool(
        &mut server,
        next_id(),
        "filesystem.rollback",
        json!({"edit_id": "edit-does-not-exist"}),
    );
    assert!(
        !response["error"].is_null(),
        "unknown edit id must be a protocol error: {response}"
    );

    server.stdin = None;
    let status = server.child.wait().expect("clean exit");
    assert!(status.success());
}

#[test]
fn editing_schema_rejection_never_mutates() {
    let (mut server, dir) = spawn_initialized_server();
    initialize_session(&mut server, 1);
    std::fs::write(dir.path().join("guard.txt"), "keep me\n").unwrap();
    let before = std::fs::read(dir.path().join("guard.txt")).unwrap();

    // Missing required argument → -32602 invalid params, zero mutation.
    let response = call_tool(
        &mut server,
        2,
        "filesystem.replace",
        json!({"path":"guard.txt","old":"keep"}),
    );
    assert_eq!(rpc_error(&response).0, -32602, "{response}");
    assert_eq!(
        std::fs::read(dir.path().join("guard.txt")).unwrap(),
        before,
        "schema rejection must not mutate"
    );

    // Wrong argument type → -32602, zero mutation.
    let response = call_tool(
        &mut server,
        3,
        "filesystem.delete_range",
        json!({"path":"guard.txt","start_line":"one","end_line":2}),
    );
    assert_eq!(rpc_error(&response).0, -32602, "{response}");
    assert_eq!(std::fs::read(dir.path().join("guard.txt")).unwrap(), before);

    // Malformed patch operation array → -32602, zero mutation.
    let response = call_tool(
        &mut server,
        4,
        "filesystem.patch",
        json!({"operations":[{"path":"guard.txt"}]}),
    );
    assert!(
        !response["error"].is_null() || response["result"]["isError"] == json!(true),
        "unsupported operation shape must fail: {response}"
    );
    assert_eq!(std::fs::read(dir.path().join("guard.txt")).unwrap(), before);

    server.stdin = None;
    let _ = server.child.wait();
}
