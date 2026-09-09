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

    // mcp.status via tools/call → healthy snapshot inside the envelope.
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
    assert_eq!(status["status"], "healthy");
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
