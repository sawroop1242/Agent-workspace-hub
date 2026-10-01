//! End-to-end CLI tests for the CTX-001 context surface: `awh context
//! show|save|update|clear|search` against the real compiled binary.
//!
//! Every `run` is a fresh process, so item persistence and reload across
//! invocations are exercised implicitly (the engine reloads durable state
//! per construction). Offload preservation, unknown-id fail-closed
//! behavior, and scope validation are pinned here.

use std::process::{Command, Stdio};
use tempfile::tempdir;

/// Runs `awh <args>` inside `dir` with optional stdin, returning
/// (exit-success, stdout, stderr).
fn run_with_stdin(dir: &std::path::Path, args: &[&str], stdin: &str) -> (bool, String, String) {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn awh binary");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait for awh");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    run_with_stdin(dir, args, "")
}

/// Reads the durable audit log (checksummed JSONL envelopes) and
/// returns the inner events as JSON values.
fn audit_events(root: &std::path::Path) -> Vec<serde_json::Value> {
    let log_path = root.join(".agent").join("audit").join("audit.log");
    let log = std::fs::read_to_string(&log_path)
        .unwrap_or_else(|_| panic!("context arm must durably audit: {log_path:?} missing"));
    log.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .map(|envelope| envelope["event"].clone())
        .collect()
}

#[test]
fn context_mutations_audit_identifiers_never_content() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    let secret = "hush-context-secret-hunter2";
    let (ok, _out, err) = run_with_stdin(
        root,
        &[
            "context",
            "save",
            "--id",
            "audit-probe",
            "--source",
            "User",
            "--content",
            secret,
        ],
        "",
    );
    assert!(ok, "context save failed: {err}");

    let (ok, _out, err) = run(root, &["context", "clear", "--id", "audit-probe"]);
    assert!(ok, "context clear failed: {err}");

    let events = audit_events(root);
    let saved = events
        .iter()
        .find(|e| e["action"] == "cli_context_save" && e["subject"] == "audit-probe")
        .expect("context save must land in the durable audit log");
    assert!(
        saved["detail"].as_str().unwrap().starts_with("scope "),
        "save detail should carry scope and tokens only: {saved}"
    );
    assert!(
        events
            .iter()
            .any(|e| e["action"] == "cli_context_clear" && e["subject"] == "audit-probe"),
        "context clear must land in the durable audit log"
    );
    // CTX-001 treats context content as sensitive: the sentinel must
    // appear nowhere in the durable log.
    let log = std::fs::read_to_string(root.join(".agent").join("audit").join("audit.log"))
        .expect("audit log readable");
    assert!(
        !log.contains(secret),
        "context content leaked into audit log"
    );
}

#[test]
fn context_retrieval_never_becomes_memory_mutation() {
    // §9 Context × memory: the context plane writes to
    // `.agent/context.md` — every retrieval-shaped operation (search,
    // status, show) and even the mutations stay entirely on that store;
    // nothing may silently persist into the project memory store
    // (`.agent/memory.json` / legacy `.agent/memory.jsonl`).
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    let (ok, _, err) = run(
        root,
        &[
            "context",
            "save",
            "--id",
            "wire-notes",
            "--content",
            "context items are not memory records",
        ],
    );
    assert!(ok, "context save failed: {err}");

    let (ok, _, err) = run(root, &["context", "search", "--query", "memory"]);
    assert!(ok, "context search failed: {err}");
    let (ok, _, err) = run(root, &["context", "show", "--id", "wire-notes"]);
    assert!(ok, "context show failed: {err}");

    // No operation in the context plane ever materialized a memory store.
    assert!(
        !root.join(".agent/memory.json").exists(),
        "context retrieval must not silently become persistent memory mutation"
    );
    assert!(
        !root.join(".agent/memory.jsonl").exists(),
        "context retrieval must not create legacy memory records either"
    );
    // And the context engine's own store carries the data, proving the
    // operations genuinely executed rather than no-opped.
    assert!(
        root.join(".agent/context-engine").exists(),
        "context items persisted in the context engine's store"
    );
}

#[test]
fn context_save_show_search_clear_lifecycle() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // save (stdin content path)
    let (ok, out, err) = run_with_stdin(
        root,
        &[
            "context",
            "save",
            "--id",
            "spec-summary",
            "--source",
            "Summary",
            "--scope",
            "Project",
        ],
        "the build uses cargo workspaces",
    );
    assert!(ok, "context save failed: {err}");
    assert!(
        out.contains("saved context item spec-summary"),
        "got: {out}"
    );

    // save a second item with explicit --content
    let (ok, out, err) = run(
        root,
        &[
            "context",
            "save",
            "--id",
            "tui-notes",
            "--content",
            "tui screens ride the workspace backend",
            "--relevance",
            "0.9",
        ],
    );
    assert!(ok, "context save 2 failed: {err}");
    assert!(out.contains("saved context item tui-notes"), "got: {out}");

    // show status reports both active items
    let (ok, out, err) = run(root, &["context", "show"]);
    assert!(ok, "context show failed: {err}");
    assert!(out.contains("\"active_items\": 2"), "got: {out}");

    // show --id returns the single item
    let (ok, out, err) = run(root, &["context", "show", "--id", "spec-summary"]);
    assert!(ok, "context show --id failed: {err}");
    assert!(out.contains("\"id\": \"spec-summary\""), "got: {out}");
    assert!(
        out.contains("the build uses cargo workspaces"),
        "got: {out}"
    );

    // show unknown id fails closed
    let (ok, _, err) = run(root, &["context", "show", "--id", "ghost"]);
    assert!(!ok, "show unknown id must fail");
    assert!(err.contains("not found"), "got: {err}");

    // search finds the item and reports an excerpt
    let (ok, out, err) = run(
        root,
        &["context", "search", "--query", "cargo", "--limit", "5"],
    );
    assert!(ok, "context search failed: {err}");
    assert!(out.contains("spec-summary"), "got: {out}");

    // search with no match returns an empty array, not an error
    let (ok, out, err) = run(root, &["context", "search", "--query", "zzz-no-match"]);
    assert!(ok, "empty search must succeed: {err}");
    assert!(out.trim() == "[]", "got: {out}");

    // clear by id, then verify it is gone from status
    let (ok, out, err) = run(root, &["context", "clear", "--id", "tui-notes"]);
    assert!(ok, "context clear failed: {err}");
    assert!(out.contains("cleared context item tui-notes"), "got: {out}");
    let (ok, out, _) = run(root, &["context", "show"]);
    assert!(ok && out.contains("\"active_items\": 1"), "got: {out}");

    // clear an unknown id fails closed
    let (ok, _, err) = run(root, &["context", "clear", "--id", "ghost"]);
    assert!(!ok, "clear unknown id must fail");
    assert!(err.contains("not found"), "got: {err}");

    // clear --all empties the active window
    let (ok, out, err) = run(root, &["context", "clear", "--all"]);
    assert!(ok, "context clear --all failed: {err}");
    assert!(
        out.contains("cleared 1 active context item(s)"),
        "got: {out}"
    );
    let (ok, out, _) = run(root, &["context", "show"]);
    assert!(ok && out.contains("\"active_items\": 0"), "got: {out}");
}

#[test]
fn context_update_requires_existing_id_and_save_upserts() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // update on an unknown id fails closed instead of creating one
    let (ok, _, err) = run(
        root,
        &["context", "update", "--id", "ghost", "--content", "nope"],
    );
    assert!(!ok, "update unknown id must fail");
    assert!(err.contains("not found"), "got: {err}");

    // save then update replaces the content
    let (ok, _, err) = run(root, &["context", "save", "--id", "a", "--content", "v1"]);
    assert!(ok, "save failed: {err}");
    let (ok, out, err) = run(root, &["context", "update", "--id", "a", "--content", "v2"]);
    assert!(ok, "update failed: {err}");
    assert!(out.contains("updated context item a"), "got: {out}");
    let (ok, out, _) = run(root, &["context", "show", "--id", "a"]);
    assert!(ok && out.contains("v2"), "got: {out}");

    // save with an existing id is an upsert (replace), matching the engine
    let (ok, out, err) = run(root, &["context", "save", "--id", "a", "--content", "v3"]);
    assert!(ok, "upsert save failed: {err}");
    assert!(out.contains("saved context item a"), "got: {out}");
    let (ok, out, _) = run(root, &["context", "show", "--id", "a"]);
    assert!(ok && out.contains("v3"), "got: {out}");
}

#[test]
fn context_rejects_invalid_scope_and_traversal_ids() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // invalid scope fails closed with the canonical MCP error text
    let (ok, _, err) = run(
        root,
        &[
            "context",
            "save",
            "--id",
            "bad-scope",
            "--content",
            "x",
            "--scope",
            "Universe",
        ],
    );
    assert!(!ok, "invalid scope must fail");
    assert!(err.contains("invalid scope 'Universe'"), "got: {err}");

    // traversal-shaped ids are rejected by the engine's id validation
    let (ok, _, err) = run(
        root,
        &["context", "save", "--id", "../escape", "--content", "x"],
    );
    assert!(!ok, "traversal id must fail");
    assert!(err.contains("invalid context item id"), "got: {err}");

    // clear without --id or --all is a usage error (clap enforces
    // required_unless_present)
    let (ok, _, _) = run(root, &["context", "clear"]);
    assert!(!ok, "bare clear must fail with usage error");
}

#[test]
fn context_clear_all_preserves_offloaded_items() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // save and offload an item so it exists in durable offload storage
    let (ok, _, err) = run(
        root,
        &[
            "context",
            "save",
            "--id",
            "keep-me",
            "--content",
            "recoverable",
        ],
    );
    assert!(ok, "save failed: {err}");

    // Offload through the MCP-facing engine API is not a CLI verb; use the
    // library directly to place the item into the offload store, then verify
    // the CLI's clear --all never destroys recoverable state.
    let engine = agent_workspace_hub::context::ContextEngine::new(
        root,
        agent_workspace_hub::context::ContextEngineConfig::default(),
    )
    .expect("engine");
    engine.offload("keep-me", "test offload").expect("offload");

    let (ok, out, err) = run(root, &["context", "clear", "--all"]);
    assert!(ok, "clear --all failed: {err}");
    assert!(out.contains("offloaded items are preserved"), "got: {out}");

    // the offloaded item is still recoverable through the engine
    assert!(engine.get_item("keep-me").is_some(), "offloaded item lost");

    // search still sees the offloaded item's content
    let (ok, out, err) = run(root, &["context", "search", "--query", "recoverable"]);
    assert!(ok, "search failed: {err}");
    assert!(out.contains("keep-me"), "offloaded item invisible: {out}");
}

#[test]
fn context_status_shape_matches_engine_contract() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    let (ok, out, err) = run(root, &["context", "show"]);
    assert!(ok, "fresh status failed: {err}");
    // The documented status fields are stable contract output.
    for field in [
        "\"enabled\"",
        "\"active_items\"",
        "\"active_tokens\"",
        "\"budget\"",
        "\"offloaded_items\"",
        "\"protected_items\"",
    ] {
        assert!(out.contains(field), "status missing {field}: {out}");
    }
}
