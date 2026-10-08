//! AWE-015 / AWE-017 / TW-008 acceptance suite (Prompt 16).
//!
//! These tests pin the *acceptance* contracts of the agent-grade editing
//! plane against real production boundaries — the compiled `awh` binary
//! for the CLI and MCP planes, and the canonical `EditService` for the
//! service plane — never against a re-implementation of the edit logic.
//!
//! What lives here (complementary to, not duplicating, the existing
//! suites):
//!
//! * **Cross-interface parity** (§17): the same Replace succeeds and
//!   byte-identically mutates through the service, the CLI binary, and
//!   the MCP dispatcher; a stale ExpectedState conflicts everywhere it
//!   is expressible with bytes preserved.
//! * **Exact-byte matrix** (§22): emoji, Devanagari, CRLF, mixed
//!   newline, no-final-newline, empty, zero-byte, and binary-like bytes
//!   through the real binary with byte-level readback AND rollback to
//!   the exact original bytes; oversized input rejection.
//! * **MCP restart** (§24): provenance/snapshot durability across
//!   three separate server processes (commit, rollback, idempotent
//!   re-rollback).
//! * **Symlink containment through the CLI edit plane** (§12 §26).
//! * **Corrupted recovery material fails closed** (§14 §28) through the
//!   CLI, with the live bytes preserved.
//! * **Cross-workspace recovery rejection** (§18 §26): copied
//!   `.agent` recovery material cannot be replayed in another
//!   workspace.
//! * **Durable correlated audit for CLI edits** (§16 §35): `awh fs`
//!   edits land in `.agent/audit/audit.log` with edit/workspace
//!   correlation and no file-content leakage.
//!
//! Concurrency of the edit plane is owned by `tests/fs_coordination.rs`
//! (FS-001), worktree lifecycle by `tests/worktree_cli.rs` (GIT-001),
//! trust-gate semantics by `tests/mcp_builtin_tool_gate.rs`, and agent
//! routes by `tests/mcp_agent_routes.rs` — this suite only adds the
//! edit-plane acceptance evidence those files do not carry.

use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use tempfile::tempdir;

// ---------------------------------------------------------------------------
// CLI workspace harness (real binary, isolated temp workspace)
// ---------------------------------------------------------------------------

struct CliWs {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

impl CliWs {
    /// An initialized AWH workspace: `awh init` has run and the manifest
    /// exists on disk, exactly like a real operator environment.
    fn new() -> Self {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        run_in(&root, &["init", "--path", "."]);
        Self { root, _dir: dir }
    }

    fn write(&self, path: &str, content: &str) {
        std::fs::write(self.root.join(path), content).expect("write fixture");
    }

    fn write_bytes(&self, path: &str, content: &[u8]) {
        std::fs::write(self.root.join(path), content).expect("write binary fixture");
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.join(path)).expect("read result")
    }

    fn read_bytes(&self, path: &str) -> Vec<u8> {
        std::fs::read(self.root.join(path)).expect("read result bytes")
    }

    /// Runs the command and asserts + returns its stdout text.
    fn ok(&self, args: &[&str]) -> String {
        let output = run_in(&self.root, args);
        assert!(
            output.status.success(),
            "expected success, exit {:?}: stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("utf-8 stdout")
    }

    /// Extracts the edit id from a JSON-mode result line.
    fn edit_id_of(&self, stdout: &str) -> String {
        let value: Value = serde_json::from_str(stdout.trim()).expect("json line");
        value["edit_id"].as_str().expect("edit_id field").to_owned()
    }

    fn agent_dir(&self, sub: &str) -> PathBuf {
        self.root.join(".agent").join(sub)
    }
}

fn run_in(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("run awh binary")
}

// ---------------------------------------------------------------------------
// MCP stdio harness (real binary, driven exactly like a real client)
// ---------------------------------------------------------------------------

struct McpProc {
    child: Child,
    stdin: Option<std::process::ChildStdin>,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

impl Drop for McpProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawns the server in an already-initialized workspace — the rollback
/// tool's global-session contract needs the canonical workspace identity.
fn spawn_mcp_in(root: &Path) -> McpProc {
    let mut child = Command::new(env!("CARGO_BIN_EXE_awh"))
        .arg("mcp")
        .arg("serve")
        .arg("--transport")
        .arg("stdio")
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn awh binary");
    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = std::io::BufReader::new(child.stdout.take().expect("stdout piped"));
    McpProc {
        child,
        stdin: Some(stdin),
        stdout,
    }
}

fn mcp_send(server: &mut McpProc, message: &Value) {
    let stdin = server.stdin.as_mut().expect("stdin still open");
    writeln!(stdin, "{message}").expect("write request");
    stdin.flush().expect("flush request");
}

fn mcp_read(server: &mut McpProc) -> Value {
    let mut line = String::new();
    let read = server.stdout.read_line(&mut line).expect("read response");
    assert!(read > 0, "server closed stdout before responding");
    serde_json::from_str(line.trim()).expect("stdout carries only JSON-RPC")
}

fn mcp_initialize(server: &mut McpProc) {
    mcp_send(
        server,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18",
            "capabilities":{},
            "clientInfo":{"name":"acceptance","version":"0.0"}
        }}),
    );
    let init = mcp_read(server);
    assert!(init["result"].is_object(), "initialize failed: {init}");
    mcp_send(
        server,
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
}

fn mcp_call(server: &mut McpProc, id: u64, name: &str, arguments: Value) -> Value {
    mcp_send(
        server,
        &json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{
            "name": name, "arguments": arguments
        }}),
    );
    mcp_read(server)
}

fn mcp_tool_payload(response: &Value) -> Value {
    assert!(
        response["error"].is_null(),
        "expected success, got error: {response}"
    );
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("content envelope");
    serde_json::from_str(text).expect("tool payload is JSON")
}

/// Shuts a server down cleanly (EOF) and asserts a zero exit — the same
/// discipline as `tests/mcp_executable.rs`: a crash or a stray stdout
/// byte would fail the restart contract being tested.
fn mcp_shutdown(server: &mut McpProc) {
    server.stdin = None;
    let status = server.child.wait().expect("wait for exit");
    assert!(
        status.success(),
        "stdio server must exit 0 on EOF, got {status:?}"
    );
}

// ---------------------------------------------------------------------------
// Shared: the canonical edit every interface performs
// ---------------------------------------------------------------------------

/// The fixture and the expected post-edit bytes of the parity edit.
const PARITY_BEFORE: &str = "alpha\nbeta\n";
const PARITY_AFTER: &str = "ALPHA\nbeta\n";

/// Executes the parity Replace through the canonical in-process service
/// (the plane the CLI and MCP adapters must ride). Returns the edit id.
fn parity_service_edit(root: &Path) -> String {
    use agent_workspace_hub::services::authorization::AuthorizingPrincipal;
    use agent_workspace_hub::services::edit::{EditOperation, EditService, EditTransaction};
    std::fs::write(root.join("parity.txt"), PARITY_BEFORE).expect("fixture");

    let manifest =
        agent_workspace_hub::services::init::load_workspace_manifest(root).expect("manifest");
    let principal = AuthorizingPrincipal::operator(manifest.workspace_id.as_str());
    let service = EditService::new(root.to_path_buf());
    let tx = EditTransaction::single(EditOperation::Replace {
        path: "parity.txt".into(),
        old: "alpha".into(),
        new: "ALPHA".into(),
        occurrence: None,
    });
    let result = service.replace_as(&principal, tx).expect("service commit");
    assert_eq!(
        std::fs::read_to_string(root.join("parity.txt")).expect("read"),
        PARITY_AFTER,
        "service mutated exactly as reported"
    );
    result.id.to_string()
}

// ---------------------------------------------------------------------------
// §17 Cross-interface parity: one edit, byte-identical, through every
// interface that exposes it — and a stale ExpectedState conflicts
// wherever it is expressible.
// ---------------------------------------------------------------------------

#[test]
fn parity_replace_conflict_rollback_across_service_cli_and_mcp() {
    use agent_workspace_hub::services::authorization::AuthorizingPrincipal;
    use agent_workspace_hub::services::edit::{
        EditError, EditOperation, EditRollbackStatus, EditService, EditTransaction, ExpectedState,
    };

    // --- service plane --------------------------------------------------
    let svc_dir = tempdir().expect("tempdir");
    let svc_root = svc_dir.path().to_path_buf();
    run_in(&svc_root, &["init", "--path", "."]);
    let svc_edit_id = parity_service_edit(&svc_root);
    assert!(svc_edit_id.starts_with("edit-"), "canonical EditId shape");

    // --- CLI plane (real binary, separate workspace) ---------------------
    let cli = CliWs::new();
    cli.write("parity.txt", PARITY_BEFORE);
    let stdout = cli.ok(&["fs", "replace", "parity.txt", "alpha", "ALPHA", "--json"]);
    assert!(stdout.contains("committed"), "{stdout}");
    assert_eq!(cli.read("parity.txt"), PARITY_AFTER);
    let cli_edit_id = cli.edit_id_of(&stdout);

    // --- MCP plane (real binary over stdio) ------------------------------
    let mcp_dir = tempdir().expect("tempdir");
    let mcp_root = mcp_dir.path().to_path_buf();
    run_in(&mcp_root, &["init", "--path", "."]);
    std::fs::write(mcp_root.join("parity.txt"), PARITY_BEFORE).expect("fixture");
    let mut server = spawn_mcp_in(&mcp_root);
    mcp_initialize(&mut server);
    let response = mcp_call(
        &mut server,
        2,
        "filesystem.replace",
        json!({"path":"parity.txt","old":"alpha","new":"ALPHA"}),
    );
    let payload = mcp_tool_payload(&response);
    assert_eq!(payload["status"], "committed", "{payload}");
    assert_eq!(
        std::fs::read_to_string(mcp_root.join("parity.txt")).expect("read"),
        PARITY_AFTER,
        "MCP mutated exactly as reported — byte-identical to the other planes"
    );
    mcp_shutdown(&mut server);

    // --- stale ExpectedState conflicts everywhere it is expressible -----
    // The MCP `filesystem.replace` tool schema deliberately does not carry
    // expected-state arguments (documented interface difference: the CLI
    // is the operator surface for stale guards); parity is asserted on
    // the two surfaces that express it.
    let svc_result = {
        let manifest = agent_workspace_hub::services::init::load_workspace_manifest(&svc_root)
            .expect("manifest");
        let principal = AuthorizingPrincipal::operator(manifest.workspace_id.as_str());
        let service = EditService::new(svc_root.clone());
        let mut tx = EditTransaction::single(EditOperation::Replace {
            path: "parity.txt".into(),
            old: "ALPHA".into(),
            new: "STALE".into(),
            occurrence: None,
        });
        tx.expected.push(ExpectedState {
            hash: Some("deadbeef".repeat(8)),
            ..ExpectedState::default()
        });
        service.replace_as(&principal, tx)
    };
    assert!(
        matches!(svc_result, Err(EditError::ExpectedStateConflict(_))),
        "stale hash must conflict, got {svc_result:?}"
    );
    assert_eq!(
        std::fs::read_to_string(svc_root.join("parity.txt")).expect("read"),
        PARITY_AFTER,
        "no stale overwrite through the service"
    );

    // CLI: same stale-guard conflict against the live hash.
    let stale_ws = CliWs::new();
    stale_ws.write("parity.txt", PARITY_BEFORE);
    let output = run_in(
        &stale_ws.root,
        &[
            "fs",
            "replace",
            "parity.txt",
            "alpha",
            "ALPHA",
            "--expected-hash",
            "0000000000000000000000000000000000000000000000000000000000000000",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(4),
        "stale hash is the documented conflict exit: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stale_ws.read("parity.txt"),
        PARITY_BEFORE,
        "no stale overwrite through the CLI"
    );

    // --- rollback parity: every plane restores the exact pre-edit bytes --
    let manifest =
        agent_workspace_hub::services::init::load_workspace_manifest(&svc_root).expect("manifest");
    let principal = AuthorizingPrincipal::operator(manifest.workspace_id.as_str());
    let service = EditService::new(svc_root.clone());
    let status = service
        .rollback_edit(&principal, &svc_edit_id)
        .expect("service rollback");
    assert_eq!(
        status,
        EditRollbackStatus::Restored,
        "service-plane rollback status"
    );
    assert_eq!(
        std::fs::read_to_string(svc_root.join("parity.txt")).expect("read"),
        PARITY_BEFORE,
        "service rollback restored exact bytes"
    );

    cli.ok(&["fs", "rollback", &cli_edit_id, "--json"]);
    assert_eq!(
        cli.read("parity.txt"),
        PARITY_BEFORE,
        "CLI rollback restored exact bytes"
    );

    // MCP rollback rides the same recovery material (fresh process, same
    // workspace as the MCP commit above): a second edit whose pre-edit
    // state is the first edit's post-state, then rolled back.
    let mut server = spawn_mcp_in(&mcp_root);
    mcp_initialize(&mut server);
    let response = mcp_call(
        &mut server,
        3,
        "filesystem.replace",
        json!({"path":"parity.txt","old":"ALPHA","new":"alpha"}),
    );
    let second = mcp_tool_payload(&response);
    let second_id = second["id"].as_str().expect("second edit id").to_owned();
    let response = mcp_call(
        &mut server,
        4,
        "filesystem.rollback",
        json!({"edit_id": second_id}),
    );
    let payload = mcp_tool_payload(&response);
    assert_eq!(payload["status"], "restored", "{payload}");
    assert_eq!(
        std::fs::read_to_string(mcp_root.join("parity.txt")).expect("read"),
        PARITY_AFTER,
        "MCP rollback restored the second edit's exact pre-edit bytes"
    );
    mcp_shutdown(&mut server);
}

// ---------------------------------------------------------------------------
// §22 Exact-byte matrix through the real binary, with rollback round-trips
// ---------------------------------------------------------------------------

#[test]
fn exact_byte_matrix_round_trip_with_rollback() {
    // (file, old, new, expected-after) — every cell is byte-compared, and
    // every edit is rolled back to the exact original bytes afterwards.
    let cases: Vec<(&str, &str, &str, &str)> = vec![
        // CRLF stays CRLF (only the named bytes change).
        ("crlf.txt", "one\r\n", "uno\r\n", "uno\r\ntwo\r\n"),
        // Mixed newline styles are preserved untouched.
        (
            "mixed.txt",
            "lf line\n",
            "LF line\n",
            "crlf line\r\nLF line\nno-eol",
        ),
        // Emoji + Devanagari multi-byte boundaries.
        (
            "uni.txt",
            "first 🌍",
            "first 🌍✅",
            "first 🌍✅\nsecond नमस्ते\n",
        ),
    ];

    for (path, old, new, after) in cases {
        let before = match path {
            "crlf.txt" => "one\r\ntwo\r\n",
            "mixed.txt" => "crlf line\r\nlf line\nno-eol",
            "uni.txt" => "first 🌍\nsecond नमस्ते\n",
            _ => unreachable!(),
        };
        let ws = CliWs::new();
        ws.write(path, before);

        let stdout = ws.ok(&["fs", "replace", path, old, new, "--json"]);
        let edit_id = ws.edit_id_of(&stdout);
        assert_eq!(ws.read(path), after, "exact bytes for {path}");

        // Rollback restores the exact ORIGINAL bytes — including the
        // no-final-newline and CRLF shapes.
        ws.ok(&["fs", "rollback", &edit_id, "--json"]);
        assert_eq!(ws.read(path), before, "rollback is byte-exact for {path}");
    }

    // --- no final newline: replace at the very end ------------------------
    let ws = CliWs::new();
    ws.write("noeol.txt", "trailing");
    let stdout = ws.ok(&[
        "fs",
        "replace",
        "noeol.txt",
        "trailing",
        "TRAILING",
        "--json",
    ]);
    let edit_id = ws.edit_id_of(&stdout);
    assert_eq!(ws.read("noeol.txt"), "TRAILING");
    assert!(
        !ws.read("noeol.txt").ends_with('\n'),
        "no newline invented by the edit"
    );
    ws.ok(&["fs", "rollback", &edit_id, "--json"]);
    assert_eq!(ws.read("noeol.txt"), "trailing");

    // --- empty text: insert creates content in an empty file -------------
    let ws = CliWs::new();
    ws.write("empty.txt", "");
    let stdout = ws.ok(&["fs", "insert", "empty.txt", "0", "seeded line\n", "--json"]);
    let edit_id = ws.edit_id_of(&stdout);
    assert_eq!(ws.read("empty.txt"), "seeded line\n");
    ws.ok(&["fs", "rollback", &edit_id, "--json"]);
    assert_eq!(ws.read("empty.txt"), "");

    // --- binary-like but valid-UTF-8 bytes: byte-level round trip -------
    // The editing plane is a TEXT plane: NUL and other control bytes are
    // in-contract, and replace/rollback stay byte-exact over them.
    let ws = CliWs::new();
    let original: Vec<u8> = vec![0x00, 0x01, 0x02, 0x7f, 0x0a, 0x00, 0x42];
    ws.write_bytes("blob.bin", &original);
    let stdout = ws.ok(&["fs", "replace", "blob.bin", "B", "BB", "--json"]);
    let edit_id = ws.edit_id_of(&stdout);
    let mut edited = original.clone();
    edited.pop();
    edited.extend_from_slice(b"BB");
    assert_eq!(ws.read_bytes("blob.bin"), edited);
    ws.ok(&["fs", "rollback", &edit_id, "--json"]);
    assert_eq!(
        ws.read_bytes("blob.bin"),
        original,
        "rollback restored the NUL-containing bytes exactly"
    );

    // --- invalid UTF-8 is rejected deterministically, bytes preserved ----
    let ws = CliWs::new();
    let invalid: Vec<u8> = vec![0x00, 0xff, 0xfe, 0x0a, 0x42];
    ws.write_bytes("invalid.bin", &invalid);
    let output = run_in(&ws.root, &["fs", "replace", "invalid.bin", "B", "BB"]);
    assert_ne!(
        output.status.code(),
        Some(0),
        "a non-UTF-8 file must be refused, not edited"
    );
    assert_eq!(
        ws.read_bytes("invalid.bin"),
        invalid,
        "the refused edit left the bytes untouched"
    );
    assert!(
        ws.agent_dir("provenance")
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true),
        "no provenance record for the refused edit"
    );

    // --- oversized input is rejected before any mutation ------------------
    let ws = CliWs::new();
    let cap = agent_workspace_hub::services::files::MAX_FILE_BYTES as usize;
    let huge = "x".repeat(cap + 1);
    ws.write("huge.txt", &huge);
    let output = run_in(&ws.root, &["fs", "replace", "huge.txt", "x", "y"]);
    assert_ne!(
        output.status.code(),
        Some(0),
        "over-cap file must fail, not silently edit"
    );
    assert_eq!(
        std::fs::read_to_string(ws.root.join("huge.txt"))
            .expect("read")
            .len(),
        cap + 1,
        "rejected oversized edit must leave the file untouched"
    );
    assert!(
        ws.agent_dir("provenance")
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true),
        "no provenance record for the rejected oversized edit"
    );
}

// ---------------------------------------------------------------------------
// §24 MCP restart: recovery material is durable across processes
// ---------------------------------------------------------------------------

#[test]
fn mcp_restart_before_and_after_rollback() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    run_in(&root, &["init", "--path", "."]);
    std::fs::write(root.join("restart.txt"), "before\n").expect("fixture");

    // Server A commits; then the process goes away.
    let mut server_a = spawn_mcp_in(&root);
    mcp_initialize(&mut server_a);
    let response = mcp_call(
        &mut server_a,
        2,
        "filesystem.replace",
        json!({"path":"restart.txt","old":"before","new":"after"}),
    );
    let payload = mcp_tool_payload(&response);
    let edit_id = payload["id"].as_str().expect("edit id").to_owned();
    assert_eq!(payload["status"], "committed", "{payload}");
    mcp_shutdown(&mut server_a);
    assert_eq!(
        std::fs::read_to_string(root.join("restart.txt")).expect("read"),
        "after\n",
        "commit is durable before restart"
    );

    // Server B (a fresh process) rolls the edit back — the provenance and
    // snapshot material survived the restart, and no in-memory state was
    // invented by the new process.
    let mut server_b = spawn_mcp_in(&root);
    mcp_initialize(&mut server_b);
    let response = mcp_call(
        &mut server_b,
        2,
        "filesystem.rollback",
        json!({"edit_id": edit_id}),
    );
    let payload = mcp_tool_payload(&response);
    assert_eq!(payload["status"], "restored", "{payload}");
    assert_eq!(
        std::fs::read_to_string(root.join("restart.txt")).expect("read"),
        "before\n",
        "rollback across processes restored exact bytes"
    );
    mcp_shutdown(&mut server_b);

    // Server C: the provenance's rolled-back outcome is durable too — a
    // third process sees the idempotent no-op, not a fresh restore.
    let mut server_c = spawn_mcp_in(&root);
    mcp_initialize(&mut server_c);
    let response = mcp_call(
        &mut server_c,
        2,
        "filesystem.rollback",
        json!({"edit_id": edit_id}),
    );
    let payload = mcp_tool_payload(&response);
    assert_eq!(payload["status"], "already_rolled_back", "{payload}");
    assert_eq!(
        std::fs::read_to_string(root.join("restart.txt")).expect("read"),
        "before\n",
        "the idempotent no-op changed nothing"
    );
    mcp_shutdown(&mut server_c);
}

// ---------------------------------------------------------------------------
// §12 §26 Symlink containment through the CLI edit plane
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn symlink_escape_rejected_by_cli_edit_plane() {
    use std::os::unix::fs::symlink;

    let ws = CliWs::new();
    ws.write("real.txt", "inside\n");
    let outside = tempdir().expect("outside dir");
    let outside_target = outside.path().join("target.txt");
    std::fs::write(&outside_target, "outside\n").expect("outside fixture");
    symlink(&outside_target, ws.root.join("link.txt")).expect("symlink in root");

    // Replace THROUGH an in-root symlink pointing outside: rejected.
    let output = run_in(
        &ws.root,
        &["fs", "replace", "link.txt", "outside", "ESCAPED"],
    );
    assert_ne!(
        output.status.code(),
        Some(0),
        "edit through an escaping symlink must fail closed"
    );
    assert_eq!(
        std::fs::read_to_string(&outside_target).expect("read"),
        "outside\n",
        "the outside target was never mutated"
    );

    // Symlinked PARENT directory: a traversal-shaped write through a
    // directory symlink is rejected the same way.
    let outside_dir = tempdir().expect("outside dir");
    std::fs::create_dir_all(outside_dir.path().join("sub")).expect("sub dir");
    std::fs::write(outside_dir.path().join("sub").join("file.txt"), "outside\n")
        .expect("outside file");
    symlink(outside_dir.path().join("sub"), ws.root.join("dirlink")).expect("dir symlink");
    let output = run_in(
        &ws.root,
        &["fs", "insert", "dirlink/file.txt", "1", "ESCAPED\n"],
    );
    assert_ne!(
        output.status.code(),
        Some(0),
        "edit through a symlinked parent must fail closed"
    );
    assert_eq!(
        std::fs::read_to_string(outside_dir.path().join("sub").join("file.txt")).expect("read"),
        "outside\n",
        "no write escaped through the symlinked parent"
    );
}

// ---------------------------------------------------------------------------
// §14 §28 Corrupted recovery material fails closed (CLI plane)
// ---------------------------------------------------------------------------

#[test]
fn rollback_fails_closed_on_corrupted_recovery_blob() {
    let ws = CliWs::new();
    ws.write("guard.txt", "original\n");
    let stdout = ws.ok(&["fs", "replace", "guard.txt", "original", "edited", "--json"]);
    let edit_id = ws.edit_id_of(&stdout);
    assert_eq!(ws.read("guard.txt"), "edited\n");

    // Locate this edit's recovery material and tamper with the content
    // blob (the snapshot-contents store, not the manifest).
    let provenance_path = ws.agent_dir("provenance").join(format!("{edit_id}.json"));
    let provenance: Value =
        serde_json::from_str(&std::fs::read_to_string(&provenance_path).expect("provenance"))
            .expect("provenance json");
    let snapshot_id = provenance["snapshot_id"].as_str().expect("snapshot id");
    let blob_dir = ws.agent_dir("snapshot-contents").join(snapshot_id);
    let mut blobs: Vec<PathBuf> = std::fs::read_dir(&blob_dir)
        .expect("snapshot content dir")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "bin"))
        .collect();
    blobs.sort();
    assert!(!blobs.is_empty(), "the edit captured recovery bytes");
    std::fs::write(&blobs[0], b"torn-garbage-bytes").expect("tamper with blob");

    // Rollback must fail closed — exit 5 (recovery), live bytes kept.
    let output = run_in(&ws.root, &["fs", "rollback", &edit_id]);
    assert_eq!(
        output.status.code(),
        Some(5),
        "corrupted recovery material is the documented recovery exit: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        ws.read("guard.txt"),
        "edited\n",
        "fail-closed rollback never restores over the live state from corrupted material"
    );

    // The provenance record still tells the truth about the edit.
    let provenance: Value =
        serde_json::from_str(&std::fs::read_to_string(&provenance_path).expect("provenance"))
            .expect("provenance json");
    assert_eq!(provenance["outcome"], "committed", "outcome unchanged");
}

// ---------------------------------------------------------------------------
// §18 §26 Cross-workspace recovery rejection
// ---------------------------------------------------------------------------

#[test]
fn copied_recovery_material_is_rejected_across_workspaces() {
    let source = CliWs::new();
    source.write("shared.txt", "base\n");
    let stdout = source.ok(&["fs", "replace", "shared.txt", "base", "changed", "--json"]);
    let edit_id = source.edit_id_of(&stdout);

    // A second, separately-initialized workspace (different workspace id).
    let target = CliWs::new();
    target.write("shared.txt", "base\n");

    // The operator copies the first workspace's entire `.agent` state over
    // it — the classic replay/copied-directory attack.
    for sub in ["provenance", "snapshots", "snapshot-contents"] {
        let from = source.agent_dir(sub);
        let to = target.agent_dir(sub);
        copy_dir_recursive(&from, &to);
    }

    let output = run_in(&target.root, &["fs", "rollback", &edit_id]);
    assert_eq!(
        output.status.code(),
        Some(5),
        "foreign recovery material must fail closed, got stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("different workspace"),
        "the denial names the workspace binding: {stderr}"
    );
    assert_eq!(
        target.read("shared.txt"),
        "base\n",
        "the foreign rollback never mutated the target workspace"
    );
}

fn copy_dir_recursive(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create target dir");
    for entry in std::fs::read_dir(from).expect("read source dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_dir() {
            copy_dir_recursive(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).expect("copy file");
        }
    }
}

// ---------------------------------------------------------------------------
// §16 §27 §35 Durable correlated audit for CLI edits (AWE-013 regression)
// ---------------------------------------------------------------------------

#[test]
fn cli_edit_events_are_durably_audited() {
    let ws = CliWs::new();
    // §27: a synthetic sentinel in the file CONTENT — it must never leak
    // into the audit record (the audit subject is the path, the detail is
    // the outcome — never bytes).
    ws.write("audit.txt", "TEST_FILE_CONTENT_SECRET\n");
    let stdout = ws.ok(&[
        "fs",
        "replace",
        "audit.txt",
        "TEST_FILE_CONTENT_SECRET",
        "edited",
        "--json",
    ]);
    let edit_id = ws.edit_id_of(&stdout);
    assert_eq!(ws.read("audit.txt"), "edited\n");

    // The durable log exists. Each line is a checksummed envelope
    // `{"checksum", "event"}` — the event inside carries the correlation.
    let log_path = ws.agent_dir("audit").join("audit.log");
    let log = std::fs::read_to_string(&log_path)
        .unwrap_or_else(|_| panic!("awh fs must durably audit: {log_path:?} missing"));
    let entries: Vec<Value> = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|envelope| envelope["event"].clone())
        .collect();
    assert!(!entries.is_empty(), "audit.log holds parsed records");

    let workspace_id = agent_workspace_hub::services::init::load_workspace_manifest(&ws.root)
        .expect("manifest")
        .workspace_id
        .as_str()
        .to_string();

    let replace_event = entries
        .iter()
        .find(|entry| {
            entry["action"] == "filesystem.replace"
                && entry["edit_id"].as_str() == Some(edit_id.as_str())
        })
        .expect("correlated filesystem.replace event");
    assert_eq!(replace_event["kind"], "allow");
    assert_eq!(
        replace_event["workspace_id"].as_str(),
        Some(workspace_id.as_str()),
        "the CLI binds the workspace correlation into every durable event"
    );
    assert!(
        replace_event["event_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "durable events carry unique identity"
    );
    let sequences: Vec<u64> = entries
        .iter()
        .filter_map(|entry| entry["sequence"].as_u64())
        .collect();
    let mut sorted = sequences.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sequences, sorted, "audit sequence is strictly monotonic");

    // Rollback: a second correlated event for the same edit id.
    ws.ok(&["fs", "rollback", &edit_id, "--json"]);
    let log = std::fs::read_to_string(&log_path).expect("re-read audit.log");
    let entries: Vec<Value> = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|envelope| envelope["event"].clone())
        .collect();
    let rollback_event = entries
        .iter()
        .find(|entry| {
            entry["action"] == "filesystem.rollback"
                && entry["edit_id"].as_str() == Some(edit_id.as_str())
        })
        .expect("correlated filesystem.rollback event");
    assert_eq!(rollback_event["kind"], "allow");

    // A stale-guard conflict is durably recorded as a `conflict` with the
    // canonical reason code.
    let output = run_in(
        &ws.root,
        &[
            "fs",
            "replace",
            "audit.txt",
            "edited",
            "STALE",
            "--expected-hash",
            "0000000000000000000000000000000000000000000000000000000000000000",
        ],
    );
    assert_eq!(output.status.code(), Some(4));
    let log = std::fs::read_to_string(&log_path).expect("re-read audit.log");
    let entries: Vec<Value> = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|envelope| envelope["event"].clone())
        .collect();
    assert!(
        entries.iter().any(|entry| {
            entry["kind"] == "conflict"
                && entry["action"] == "filesystem.replace"
                && entry["reason"].as_str() == Some("expected_state_conflict")
        }),
        "the stale conflict is durably audited with its reason code"
    );

    // §27: file CONTENTS never appear in the audit record.
    assert!(
        !log.contains("TEST_FILE_CONTENT_SECRET"),
        "audit must not carry file contents"
    );
}

// ---------------------------------------------------------------------------
// §18 Worktree isolation, end to end: an edit inside one managed worktree
// is invisible to the sibling worktree and the main checkout.
// ---------------------------------------------------------------------------

#[test]
fn worktree_edit_isolation_end_to_end() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    // The git repository and its base commit come FIRST: worktrees branch
    // from HEAD, and the base commit must not embed the parent workspace's
    // `.agent` manifest — each checkout becomes its own workspace root
    // below (a checkout carrying a foreign manifest is, correctly, refused
    // by `awh init`'s canonical-root binding).
    // `core.autocrlf false` makes the fixture hermetic: on windows-latest
    // the runner image ships a system-wide `autocrlf=true`, which smudges
    // LF -> CRLF in every `git worktree add` checkout and breaks the
    // byte-exact content asserts below. All platforms agree with the
    // committed bytes.
    for args in [
        vec!["init"],
        vec!["config", "user.email", "test@example.invalid"],
        vec!["config", "user.name", "test"],
        vec!["config", "core.autocrlf", "false"],
    ] {
        let _ = Command::new("git").args(&args).current_dir(&root).output();
    }
    std::fs::write(root.join("README.md"), "base\n").expect("fixture");
    let _ = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&root)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "base"])
        .current_dir(&root)
        .output();
    // The parent's workspace state at the root is created after the commit.
    run_in(&root, &["init", "--path", "."]);

    let id_of_worktree = |stdout: &str| {
        let value: Value = serde_json::from_str(stdout.trim()).expect("json line");
        value["worktree_id"].as_str().expect("id").to_owned()
    };
    let a_out = run_in(&root, &["worktree", "create", "claude", "sess-a", "--json"]);
    assert!(
        a_out.status.success(),
        "worktree create A failed: {}",
        String::from_utf8_lossy(&a_out.stderr)
    );
    let a_id = id_of_worktree(&String::from_utf8(a_out.stdout).expect("utf-8"));
    let b_out = run_in(&root, &["worktree", "create", "qwen", "sess-b", "--json"]);
    assert!(
        b_out.status.success(),
        "worktree create B failed: {}",
        String::from_utf8_lossy(&b_out.stderr)
    );
    let b_id = id_of_worktree(&String::from_utf8(b_out.stdout).expect("utf-8"));
    let a_root = root.join(".agent").join("worktrees").join(&a_id);
    let b_root = root.join(".agent").join("worktrees").join(&b_id);
    assert!(a_root.is_dir() && b_root.is_dir());

    // Each worktree checkout operates as its own workspace root for the
    // editing plane (the runtime's effective-root contract).
    run_in(&a_root, &["init", "--path", "."]);
    let stdout = run_in(
        &a_root,
        &["fs", "replace", "README.md", "base", "A-EDIT", "--json"],
    );
    assert!(
        stdout.status.success(),
        "edit inside worktree A failed: {}",
        String::from_utf8_lossy(&stdout.stderr)
    );
    let stdout = String::from_utf8(stdout.stdout).expect("utf-8");
    let edit_id = id_of_edit(&stdout);

    assert_eq!(
        std::fs::read_to_string(a_root.join("README.md")).expect("read A"),
        "A-EDIT\n",
        "worktree A carries its own edit"
    );
    assert_eq!(
        std::fs::read_to_string(b_root.join("README.md")).expect("read B"),
        "base\n",
        "sibling worktree B is untouched"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("README.md")).expect("read main"),
        "base\n",
        "the main checkout is untouched"
    );

    // Rollback inside A restores exactly A — again invisible to B.
    run_in(&a_root, &["fs", "rollback", &edit_id, "--json"]);
    assert_eq!(
        std::fs::read_to_string(a_root.join("README.md")).expect("read A"),
        "base\n",
        "rollback restored worktree A exactly"
    );
    assert_eq!(
        std::fs::read_to_string(b_root.join("README.md")).expect("read B"),
        "base\n"
    );
}

fn id_of_edit(stdout: &str) -> String {
    let value: Value = serde_json::from_str(stdout.trim()).expect("json line");
    value["edit_id"].as_str().expect("edit id").to_owned()
}
