//! TP03 — Agent-Grade Filesystem Editing: basic-operation boundary suite.
//!
//! Scope (the *increment* over the existing edit-family suites): the
//! basic read/write/list/search/stat/hash surface at its REAL boundaries —
//! the MCP stdio tool plane (`workspace.read_file` / `write_file` /
//! `list_files`, in-process [`StdioMcpServer`] with a real temp workspace,
//! the same boundary `tests/mcp_server.rs` exercises), the canonical
//! [`FilesService`] (read_bytes/write_atomic/meta/search), the Control API
//! write route (`PUT /api/v1/files/content` through the real router), and
//! the CLI `fs` expected-hash guard proven with INDEPENDENT published
//! SHA-256 vectors (FIPS 180-4 test vectors, never the production hash
//! helper).
//!
//! The controlled-edit family (replace/insert/delete-range/patch/
//! apply-diff/history/verify/rollback, stale-state, atomicity,
//! concurrency, traversal, parity) is already covered by
//! `tests/cli_fs_edit.rs`, `tests/acceptance_editing.rs`,
//! `tests/acceptance_edit_gates.rs`, and `tests/fs_coordination.rs`;
//! this suite does not duplicate them.
//!
//! Every mutation is proven by reading the actual filesystem bytes
//! back with `std::fs` — never by trusting the API's success response
//! (§25/§34). Every rejected mutation is proven to leave the tree
//! byte-for-byte unchanged.

use agent_workspace_hub::api::control::{build_router, ControlState};
use agent_workspace_hub::mcp::StdioMcpServer;
use agent_workspace_hub::services::files::FilesService;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// One MCP tool call against the real in-process stdio server, with the
/// session initialized exactly like a real client would.
fn tool(server: &StdioMcpServer, name: &str, args: Value) -> Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": name, "arguments": args},
    });
    let raw = server.handle_response(&request.to_string());
    let response: Value = serde_json::from_str(&raw).expect("json-rpc response");
    if let Some(error) = response.get("error") {
        panic!("tool {name} failed: {error}");
    }
    response["result"].clone()
}

/// The payload the `filesystem`/`workspace` tools wrap results in. Scalar
/// results are unwrapped from their JSON encoding (the dispatcher
/// serializes a `String` result to a JSON string inside the content
/// envelope); structured results keep their JSON form.
fn content_text(result: &Value) -> String {
    let text = result["content"][0]["text"]
        .as_str()
        .expect("content text")
        .to_owned();
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::String(unwrapped)) => unwrapped,
        _ => text,
    }
}

/// A tool call that must fail; returns the JSON-RPC error object.
fn tool_error(server: &StdioMcpServer, name: &str, args: Value) -> Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": name, "arguments": args},
    });
    let raw = server.handle_response(&request.to_string());
    let response: Value = serde_json::from_str(&raw).expect("json-rpc response");
    response
        .get("error")
        .cloned()
        .unwrap_or_else(|| panic!("expected error, got result: {response}"))
}

fn server_over(root: &Path) -> StdioMcpServer {
    let server = StdioMcpServer::new(root.to_path_buf()).expect("stdio server");
    // Complete the real MCP initialization handshake exactly like every
    // real client — the lifecycle gate rejects pre-init tool calls
    // (-32002) by design, and these tests exercise tools, not the gate.
    let request = json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "tp03-suite", "version": "0.0.0"}
        }
    });
    let response: Value =
        serde_json::from_str(&server.handle_response(&request.to_string())).expect("init json");
    assert!(
        response.get("error").is_none(),
        "initialize failed: {response}"
    );
    server
}

/// A parsed result for a tool whose handler returns structured JSON.
fn result_text(result: &Value) -> Value {
    let text = content_text(result);
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("non-json result text: {text}"))
}

fn real_file_bytes(root: &Path, relative: &str) -> Vec<u8> {
    std::fs::read(root.join(relative))
        .unwrap_or_else(|error| panic!("read back {relative}: {error}"))
}

fn assert_tree_absent(root: &Path, relative: &str) {
    assert!(
        !root.join(relative).exists(),
        "{relative} must not exist after the rejected write"
    );
}

// ---------------------------------------------------------------------------
// §6 basic read — exact-byte matrix through the MCP tool plane
// ---------------------------------------------------------------------------

#[test]
fn mcp_read_file_returns_exact_bytes_without_normalization() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("crlf.txt"), "alpha\r\nbeta\r\n").unwrap();
    std::fs::write(root.join("no-trailing.txt"), "no trailing newline").unwrap();
    std::fs::write(root.join("unicode.txt"), "héllo — 日本語 🎉\n").unwrap();
    std::fs::write(root.join("blanks.txt"), "a\n\n\n\nb\n").unwrap();
    std::fs::write(root.join("tabs.txt"), "col1\tcol2\tcol3\n").unwrap();
    let server = server_over(root);

    let read = |path: &str| {
        let result = tool(
            &server,
            "workspace.read_file",
            json!({"path": path}),
        );
        content_text(&result)
    };

    // The documented contract: UTF-8 text served verbatim — CRLF, blank
    // lines, tabs and the absence of a trailing newline must all survive
    // the round trip exactly (§19).
    assert_eq!(read("crlf.txt"), "alpha\r\nbeta\r\n");
    assert_eq!(read("no-trailing.txt"), "no trailing newline");
    assert_eq!(read("unicode.txt"), "héllo — 日本語 🎉\n");
    assert_eq!(read("blanks.txt"), "a\n\n\n\nb\n");
    assert_eq!(read("tabs.txt"), "col1\tcol2\tcol3\n");
}

#[test]
fn mcp_read_file_handles_empty_large_missing_directory_and_overcap() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("empty.txt"), b"").unwrap();
    // 1.5 MiB — large but under the 2 MiB MCP read cap.
    let large = "x".repeat(1536 * 1024);
    std::fs::write(root.join("large.txt"), &large).unwrap();
    // 2 MiB + 1 byte — one byte over the documented read cap.
    std::fs::write(root.join("over.txt"), vec![b'y'; 2 * 1024 * 1024 + 1]).unwrap();
    std::fs::create_dir(root.join("adir")).unwrap();
    let server = server_over(root);

    // Empty file → empty string, not an error and not a null placeholder.
    let result = tool(&server, "workspace.read_file", json!({"path": "empty.txt"}));
    assert_eq!(content_text(&result), "");

    // Large-but-valid → complete content, no silent truncation.
    let result = tool(&server, "workspace.read_file", json!({"path": "large.txt"}));
    assert_eq!(content_text(&result).len(), large.len());
    assert_eq!(content_text(&result), large);

    // Missing file → the documented protocol error, not a panic and not
    // an empty success.
    let error = tool_error(&server, "workspace.read_file", json!({"path": "nope.txt"}));
    let message = error["message"].as_str().unwrap_or_default().to_lowercase();
    assert!(
        message.contains("not found"),
        "missing-file error must identify the cause: {error}"
    );

    // Directory target → error (never a directory listing through read).
    // The service surfaces the failed operation ("read <path>", the
    // canonical context) — assert the error, not a guessed io wording.
    let error = tool_error(&server, "workspace.read_file", json!({"path": "adir"}));
    assert_eq!(error["code"], json!(-32603), "directory read must error: {error}");
    assert!(
        error["message"].as_str().unwrap_or("").contains("read "),
        "directory read error names the failed operation: {error}",
    );

    // Over-cap read → the documented size error, and the response must
    // not smuggle any of the file body back.
    let error = tool_error(&server, "workspace.read_file", json!({"path": "over.txt"}));
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.to_lowercase().contains("2 mib"),
        "over-cap read must name the documented 2 MiB limit: {error}"
    );
    assert!(!message.contains("yyyyy"), "over-cap read must not return file bytes");
}

#[test]
fn mcp_read_is_text_only_while_service_read_bytes_is_binary_safe() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    // Invalid UTF-8 (a lone 0xff byte) — the MCP read contract is UTF-8
    // text; binary must surface as an error, never mojibake.
    std::fs::write(root.join("blob.bin"), [0x00, 0x01, 0xff, 0xfe, 0x00]).unwrap();
    let server = server_over(root);

    let error = tool_error(&server, "workspace.read_file", json!({"path": "blob.bin"}));
    assert!(
        error["code"].as_i64().is_some(),
        "binary read must be a JSON-RPC error: {error}"
    );
    // The error is about the stream, not a silent replacement: assert no
    // replacement character ever appears.
    let text = error.to_string();
    assert!(!text.contains('\u{fffd}'), "no mojibake in error: {text}");

    // The canonical service boundary preserves the exact bytes: the same
    // file read through `read_bytes` round-trips byte-for-byte. This is
    // the documented split (MCP tool = text, service = raw bytes).
    let files = FilesService::new(root);
    let bytes = files.read_bytes("blob.bin").expect("read_bytes");
    assert_eq!(bytes, vec![0x00, 0x01, 0xff, 0xfe, 0x00]);
}

// ---------------------------------------------------------------------------
// §7 basic write — exact resulting bytes at the MCP boundary
// ---------------------------------------------------------------------------

#[test]
fn mcp_write_file_creates_overwrites_and_is_deterministic() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let server = server_over(root);

    // New file, nested missing parent directories auto-created.
    tool(
        &server,
        "workspace.write_file",
        json!({"path": "docs/deep/note.md", "content": "first\n"}),
    );
    assert_eq!(real_file_bytes(root, "docs/deep/note.md"), b"first\n");

    // Overwrite: the old bytes must be REPLACED (no residue, no append).
    tool(
        &server,
        "workspace.write_file",
        json!({"path": "docs/deep/note.md", "content": "short"}),
    );
    assert_eq!(real_file_bytes(root, "docs/deep/note.md"), b"short");

    // Empty content writes a genuinely empty file.
    tool(
        &server,
        "workspace.write_file",
        json!({"path": "empty.txt", "content": ""}),
    );
    assert_eq!(real_file_bytes(root, "empty.txt"), b"");

    // Unicode content round-trips byte-exact.
    let unicode = "héllo — 日本語 🎉\r\n";
    tool(
        &server,
        "workspace.write_file",
        json!({"path": "u.txt", "content": unicode}),
    );
    assert_eq!(real_file_bytes(root, "u.txt"), unicode.as_bytes());

    // Repeated identical writes are deterministic: byte-identical state.
    let before = real_file_bytes(root, "u.txt");
    tool(
        &server,
        "workspace.write_file",
        json!({"path": "u.txt", "content": unicode}),
    );
    assert_eq!(real_file_bytes(root, "u.txt"), before);
}

#[test]
fn mcp_write_file_rejects_traversal_without_touching_the_outside_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    // A sentinel OUTSIDE the workspace: the negative proof for §20.
    let outside = dir.path().parent().unwrap().join("outside-sentinel.txt");
    std::fs::write(&outside, "do not touch\n").unwrap();

    let server = server_over(root);
    for escape in ["../outside-sentinel.txt", "a/../../outside-sentinel.txt"] {
        let error = tool_error(
            &server,
            "workspace.write_file",
            json!({"path": escape, "content": "escaped"}),
        );
        assert!(
            error["code"].as_i64().is_some(),
            "traversal write must fail: {escape} → {error}"
        );
    }
    // Absolute paths are also refused.
    let error = tool_error(
        &server,
        "workspace.write_file",
        json!({"path": outside.to_string_lossy(), "content": "escaped"}),
    );
    assert!(error["code"].as_i64().is_some(), "absolute write refused: {error}");

    // The protected outside file is byte-for-byte untouched, and no user
    // artifact leaked inside the workspace (`.agent/` is the server's own
    // legitimate state directory, created at startup — not write residue).
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "do not touch\n",
        "outside sentinel must remain unchanged"
    );
    let residue: Vec<String> = std::fs::read_dir(root)
        .unwrap()
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().to_string_lossy().into_owned();
            (name != ".agent").then_some(name)
        })
        .collect();
    assert!(
        residue.is_empty(),
        "no residue inside root: {residue:?}"
    );
}

#[test]
fn mcp_write_file_enforces_the_5_mib_cap_with_no_partial_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let server = server_over(root);

    // Exactly at the cap (5 MiB): the contract is `>` cap, so at-cap
    // content is accepted. Proves the boundary, not a far-below smoke test.
    let at_cap = "z".repeat(5 * 1024 * 1024);
    tool(
        &server,
        "workspace.write_file",
        json!({"path": "at.txt", "content": at_cap}),
    );
    assert_eq!(real_file_bytes(root, "at.txt").len(), at_cap.len());

    // One byte over the cap: rejected BEFORE any filesystem mutation —
    // the target must not exist at all, let alone partially written.
    let over = "z".repeat(5 * 1024 * 1024 + 1);
    let error = tool_error(
        &server,
        "workspace.write_file",
        json!({"path": "over.txt", "content": over}),
    );
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.to_lowercase().contains("exceeds"),
        "over-cap write must name the size error: {error}"
    );
    assert_tree_absent(root, "over.txt");

    // The at-cap file from before is untouched by the failed attempt.
    assert_eq!(real_file_bytes(root, "at.txt").len(), at_cap.len());
}

#[test]
fn mcp_write_file_directory_target_fails_and_leaves_the_tree_unchanged() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("pkg")).unwrap();
    std::fs::write(root.join("pkg/keep.txt"), "keep\n").unwrap();

    let server = server_over(root);
    let error = tool_error(
        &server,
        "workspace.write_file",
        json!({"path": "pkg", "content": "not a file"}),
    );
    assert!(error["code"].as_i64().is_some(), "directory write refused: {error}");
    // The directory still exists, still a directory, contents intact.
    assert!(root.join("pkg").is_dir());
    assert_eq!(real_file_bytes(root, "pkg/keep.txt"), b"keep\n");
}

// ---------------------------------------------------------------------------
// §6/§8 list + stat — kinds, sizes, isolation
// ---------------------------------------------------------------------------

#[test]
fn mcp_list_files_reports_only_files_sorted_with_sizes() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("README.md"), "readme").unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("src/parser.rs"), "x").unwrap();

    let server = server_over(root);
    let result = tool(&server, "workspace.list_files", json!({"path": "src"}));
    let files = result_text(&result);
    let entries: Vec<(String, u64)> = files
        .as_array()
        .expect("list is an array")
        .iter()
        .map(|entry| {
            (
                entry["path"].as_str().expect("path").to_owned(),
                entry["size"].as_u64().expect("size"),
            )
        })
        .collect();

    // Files only (the directory itself is absent), workspace-prefixed
    // paths, sorted ascending, exact sizes.
    assert_eq!(
        entries,
        vec![
            ("src/main.rs".to_owned(), 13),
            ("src/parser.rs".to_owned(), 1),
        ]
    );

    // Missing directory → the canonical error, not an empty success
    // (an empty list would be indistinguishable from an empty dir).
    let error = tool_error(&server, "workspace.list_files", json!({"path": "missing"}));
    assert!(error["code"].as_i64().is_some(), "missing dir lists as error: {error}");
}

#[test]
fn service_meta_classifies_kind_reports_size_and_tracks_mutation() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("adir")).unwrap();
    std::fs::write(root.join("text.txt"), "hello\n").unwrap();
    std::fs::write(root.join("blob.bin"), [0x00, 0xff, 0x00, 0xff]).unwrap();
    std::fs::write(root.join("big.bin"), vec![0u8; 1024]).unwrap();
    let files = FilesService::new(root);

    let text = files.meta("text.txt").expect("text meta");
    assert!(text.kind == agent_workspace_hub::services::files::PathKind::TextFile);
    assert_eq!(text.size, 6);

    let dir_meta = files.meta("adir").expect("dir meta");
    assert!(dir_meta.kind == agent_workspace_hub::services::files::PathKind::Directory);

    let binary = files.meta("blob.bin").expect("binary meta");
    assert!(binary.kind == agent_workspace_hub::services::files::PathKind::BinaryFile);

    // §8 "changed file after mutation": size must track the new state.
    std::fs::write(root.join("text.txt"), "hello world, longer now\n").unwrap();
    let changed = files.meta("text.txt").expect("changed meta");
    assert_eq!(changed.size, 24);

    // Missing path → error (never a default meta payload).
    assert!(files.meta("nope.txt").is_err());

    // Nested path with a space and Unicode works ("café\n" = 6 bytes —
    // the é is two UTF-8 bytes).
    std::fs::create_dir_all(root.join("nested dir")).unwrap();
    std::fs::write(root.join("nested dir/café notes.md"), "café\n").unwrap();
    let unicode = files.meta("nested dir/café notes.md").expect("unicode meta");
    assert_eq!(unicode.size, 6);
}

// ---------------------------------------------------------------------------
// §9 search — the matrix the Control API route also serves
// ---------------------------------------------------------------------------

#[test]
fn service_search_matrix_zero_one_many_repeated_case_unicode_limit() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(root.join("README.md"), "Parser usage example\n").unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        "fn main() {\n    let parser = Parser::new();\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/parser.rs"),
        "parser one\nparser two\nPARSER three\nunrelated\n",
    )
    .unwrap();
    std::fs::write(
        root.join("tests/parser_test.rs"),
        "parser test one\nparser test two\n",
    )
    .unwrap();
    // A binary file must be skipped silently, not fail the search.
    std::fs::write(root.join("src/parser.bin"), [0x00, 0xff, 0x00]).unwrap();
    let files = FilesService::new(root);

    // Zero matches → empty result (not an error, not null).
    assert!(files.search("zebra", 100).unwrap().is_empty());

    // Many matches across files, nested dirs included, binary skipped,
    // case-insensitive per the documented substring semantics.
    let hits = files.search("parser", 100).unwrap();
    let paths: Vec<&str> = hits.iter().map(|hit| hit.path.as_str()).collect();
    for expected in ["README.md", "src/main.rs", "src/parser.rs", "tests/parser_test.rs"] {
        assert!(paths.contains(&expected), "missing hit in {expected}: {paths:?}");
    }
    assert!(
        !paths.contains(&"src/parser.bin"),
        "binary file must be skipped"
    );

    // Repeated matches in one file → distinct hits with ascending line
    // numbers and the matching line text.
    let parser_hits: Vec<&agent_workspace_hub::services::files::SearchHit> = hits
        .iter()
        .filter(|hit| hit.path == "src/parser.rs")
        .collect();
    assert_eq!(parser_hits.len(), 3, "three 'parser' lines in parser.rs");
    assert_eq!(parser_hits[0].line_number, 1);
    assert_eq!(parser_hits[1].line_number, 2);
    assert_eq!(parser_hits[2].line_number, 3);
    assert_eq!(parser_hits[2].line, "PARSER three");

    // Special characters follow plain substring semantics (no regex).
    std::fs::write(root.join("special.txt"), "fn(x) fn( fn(x)\n").unwrap();
    let hits = files.search("fn(", 100).unwrap();
    let special: Vec<_> = hits
        .iter()
        .filter(|hit| hit.path == "special.txt")
        .collect();
    assert_eq!(special.len(), 1, "substring semantics: one matching line");
    assert_eq!(special[0].line, "fn(x) fn( fn(x)");

    // Unicode needle.
    std::fs::write(root.join("u.txt"), "café au lait\nCafé central\n").unwrap();
    let hits = files.search("café", 100).unwrap();
    assert_eq!(hits.len(), 2, "unicode matches case-insensitively: {hits:?}");

    // Limit bounding: limit=1 returns exactly one hit.
    let hits = files.search("parser", 1).unwrap();
    assert_eq!(hits.len(), 1, "limit must bound the result");

    // limit=0 is the documented "no results" bound, not an error.
    assert!(files.search("parser", 0).unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// §10 hash — independent published SHA-256 vectors through the real CLI
// ---------------------------------------------------------------------------

/// FIPS 180-4 published test vectors (independent of the production
/// `sha256_hex` helper — sourced from the published spec, §10).
#[test]
fn cli_expected_hash_guard_matches_independent_published_vectors() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(["init", "--path", "."])
        .current_dir(root)
        .output()
        .expect("init workspace");

    // sha256("abc") = ba7816bf... (FIPS 180-4 example). File content is
    // exactly "abc" — no trailing newline, so the hash of the file IS
    // the published vector.
    std::fs::write(root.join("vector.txt"), "abc").unwrap();

    let ok = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args([
            "fs",
            "replace",
            "vector.txt",
            "abc",
            "xyz",
            "--expected-hash",
            // sha256("abc"), lowercase hex — the documented format.
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ])
        .current_dir(root)
        .output()
        .expect("replace with correct vector");
    assert!(
        ok.status.success(),
        "published-vector hash must authorize the edit: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert_eq!(real_file_bytes(root, "vector.txt"), b"xyz");

    // One hex character off → conflict (exit 4), bytes unchanged. This
    // pins that the guard compares the FULL digest, not a prefix.
    std::fs::write(root.join("vector.txt"), "abc").unwrap();
    let conflict = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args([
            "fs",
            "replace",
            "vector.txt",
            "abc",
            "xyz",
            "--expected-hash",
            "aa7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ])
        .current_dir(root)
        .output()
        .expect("replace with corrupted vector");
    assert_eq!(
        conflict.status.code(),
        Some(4),
        "stale hash must exit 4 (conflict): stdout={} stderr={}",
        String::from_utf8_lossy(&conflict.stdout),
        String::from_utf8_lossy(&conflict.stderr)
    );
    assert_eq!(real_file_bytes(root, "vector.txt"), b"abc");

    // The empty-file vector (sha256("") = e3b0c442...) authorizes an
    // insert into a genuinely empty file.
    std::fs::write(root.join("empty.txt"), b"").unwrap();
    let ok = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args([
            "fs",
            "insert",
            "empty.txt",
            "1",
            "seeded",
            "--expected-hash",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ])
        .current_dir(root)
        .output()
        .expect("insert with empty vector");
    assert!(
        ok.status.success(),
        "empty-file vector must authorize the insert: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert_eq!(real_file_bytes(root, "empty.txt"), b"seeded\n");
}

// ---------------------------------------------------------------------------
// §22 CLI/service parity — identical bytes at three real boundaries
// ---------------------------------------------------------------------------

#[test]
fn parity_write_produces_identical_bytes_across_mcp_service_and_control_api() {
    let content = "shared payload — 🎉\r\nsecond line\n";

    // Boundary 1: the MCP tool plane. (Done outside any tokio runtime:
    // StdioMcpServer owns its runtime and blocks internally.)
    let mcp_dir = tempdir().unwrap();
    let mcp_server = server_over(mcp_dir.path());
    tool(
        &mcp_server,
        "workspace.write_file",
        json!({"path": "shared.txt", "content": content}),
    );

    // Boundary 2: the canonical service.
    let service_dir = tempdir().unwrap();
    FilesService::new(service_dir.path())
        .write_atomic("shared.txt", content)
        .expect("service write");

    // Boundary 3: the Control API write route through the real router,
    // driven on a dedicated current-thread runtime.
    let api_dir = tempdir().unwrap();
    let state = std::sync::Arc::new(ControlState::new(api_dir.path(), "tp03-key".to_owned()));
    let router = build_router(state);
    let request = axum::http::Request::builder()
        .method("PUT")
        .uri("/api/v1/files/content")
        .header("Authorization", "Bearer tp03-key")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({"path": "shared.txt", "content": content}).to_string(),
        ))
        .expect("build request");
    let response = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(router.oneshot(request))
        .expect("control api request");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "control api write must succeed"
    );

    let expected = content.as_bytes();
    assert_eq!(
        real_file_bytes(mcp_dir.path(), "shared.txt"),
        expected,
        "MCP boundary bytes"
    );
    assert_eq!(
        real_file_bytes(service_dir.path(), "shared.txt"),
        expected,
        "service boundary bytes"
    );
    assert_eq!(
        real_file_bytes(api_dir.path(), "shared.txt"),
        expected,
        "control api boundary bytes"
    );

    // Read parity across the same boundaries: what one boundary wrote,
    // the others read identically (same canonical service under all).
    let via_mcp = {
        let server = server_over(service_dir.path());
        let result = tool(&server, "workspace.read_file", json!({"path": "shared.txt"}));
        content_text(&result)
    };
    assert_eq!(via_mcp.as_bytes(), expected);
    assert_eq!(
        FilesService::new(mcp_dir.path()).read("shared.txt").unwrap(),
        content
    );
}

// ---------------------------------------------------------------------------
// §29 resource bounds kept CI-practical
// ---------------------------------------------------------------------------

#[test]
fn deep_nesting_and_many_files_stay_bounded_and_correct() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // A 12-level nested write/read round trip through the MCP plane.
    let deep = (0..12).map(|i| format!("level{i}")).collect::<Vec<_>>().join("/");
    let server = server_over(root);
    tool(
        &server,
        "workspace.write_file",
        json!({"path": format!("{deep}/leaf.txt"), "content": "deep"}),
    );
    assert_eq!(real_file_bytes(root, &format!("{deep}/leaf.txt")), b"deep");
    let result = tool(
        &server,
        "workspace.read_file",
        json!({"path": format!("{deep}/leaf.txt")}),
    );
    assert_eq!(content_text(&result), "deep");

    // 40 sibling files: list stays complete, sorted, and bounded.
    std::fs::create_dir_all(root.join("many")).unwrap();
    for i in 0..40 {
        std::fs::write(root.join(format!("many/file{i:02}.txt")), format!("{i}"))
            .unwrap();
    }
    let files = FilesService::new(root);
    let entries = files.list("many").expect("list many");
    assert_eq!(entries.len(), 40);
    // The service layer does not guarantee read_dir order (§9's ordering
    // rule: assert only the guaranteed contract) — sort before asserting.
    let mut names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    names.sort();
    assert_eq!(names[0], "file00.txt");
    assert_eq!(names[39], "file39.txt");
    assert_eq!(names.len(), 40);
}
