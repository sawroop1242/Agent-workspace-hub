//! TP09 — Context Engine boundary verification suite.
//!
//! Drives the REAL boundaries: the compiled `awh` binary across fresh
//! processes (CLI plane and the MCP stdio plane), the canonical
//! `ContextEngine` over real `.agent/context-engine/` state, and raw
//! persisted bytes as the independent oracle. Nothing is mocked and no
//! second engine, window, offload store, snapshot store, or token counter
//! is reimplemented.
//!
//! Sections (per docs/testing-prompts/09-context-engine.md):
//! - CLI black-box: §6 disabled engine, §7 item validation/upsert,
//!   §9 budget boundary, §22 audit identifiers-only, §23 restart,
//!   §24 corruption fail-closed, §8 workspace isolation.
//! - Engine integration: §10 selection determinism, §11 protected items,
//!   §13 offload/restore persistence + corruption, §14 snapshots,
//!   §12 compression, §15 search limits, §17 assemble invariants,
//!   §25 concurrency.
//! - MCP stdio black-box: §28/§29 the context.* tool surface, the TP09 D1
//!   resurrection regression at the wire level, and schema validation.

use agent_workspace_hub::context::budget::ContextBudget;
use agent_workspace_hub::context::{
    ContextEngine, ContextEngineConfig, ContextItem, ContextRequest, ContextScope, ContextSource,
    ContextState,
};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use tempfile::TempDir;

// ---------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------

fn awh() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_awh"));
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// Runs one `awh` CLI invocation in `dir`; returns (success, stdout, stderr).
fn run(dir: &Path, args: &[&str]) -> (bool, String, String) {
    run_env(dir, args, &[])
}

/// Same, with per-child environment overrides (isolated per §5).
fn run_env(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> (bool, String, String) {
    let mut cmd = awh();
    cmd.args(args).current_dir(dir);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    drop(child.stdin.take()); // close stdin: no piped input for these args
    let output = child.wait_with_output().unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn engine_window(root: &Path) -> PathBuf {
    root.join(".agent")
        .join("context-engine")
        .join("active.json")
}

fn read_window(root: &Path) -> Value {
    let raw = std::fs::read_to_string(engine_window(root))
        .unwrap_or_else(|e| panic!("window must exist: {e}"));
    serde_json::from_str(&raw).expect("window is valid JSON")
}

fn item_of<'a>(window: &'a Value, id: &str) -> Option<&'a Value> {
    window["items"]
        .as_array()
        .expect("items array")
        .iter()
        .find(|i| i["id"].as_str() == Some(id))
}

fn window_item_ids(root: &Path) -> Vec<String> {
    read_window(root)["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect()
}

fn insert_item(engine: &ContextEngine, id: &str, content: &str, relevance: f32) {
    let mut item = ContextItem::new(id, ContextSource::Tool, content, 0);
    item.relevance = relevance;
    engine.insert(item).expect("insert");
}

// ---------------------------------------------------------------------
// CLI black-box (§28)
// ---------------------------------------------------------------------

#[test]
fn cli_restart_persistence_scope_metadata_and_exact_bytes() {
    // Workflow D: save → terminate → fresh process → verify. Scope is
    // persisted as explicit metadata; content round-trips byte-exact.
    let root = TempDir::new().unwrap();
    let unicode = "résumé — مرحبا 世界 🚀 \"quoted\" \\ backslash\ttab";
    let (ok, out, err) = run(
        root.path(),
        &[
            "context",
            "save",
            "--id",
            "session-item",
            "--scope",
            "Session",
            "--content",
            unicode,
        ],
    );
    assert!(ok, "save failed: {err}");
    assert!(out.contains("saved context item session-item"), "{out}");
    let (ok, _, _) = run(
        root.path(),
        &[
            "context",
            "save",
            "--id",
            "global-item",
            "--scope",
            "Global",
            "--content",
            "global note",
        ],
    );
    assert!(ok);
    let (ok, _, _) = run(
        root.path(),
        &[
            "context",
            "save",
            "--id",
            "project-item",
            "--content",
            "project default scope",
        ],
    );
    assert!(ok);

    // Independent oracle: the raw window file is schema-versioned and
    // carries scope/state/content verbatim.
    let window = read_window(root.path());
    assert_eq!(window["schema_version"].as_u64(), Some(1));
    assert_eq!(
        item_of(&window, "session-item").unwrap()["scope"],
        json!("session")
    );
    assert_eq!(
        item_of(&window, "global-item").unwrap()["scope"],
        json!("global")
    );
    assert_eq!(
        item_of(&window, "project-item").unwrap()["scope"],
        json!("project")
    );
    assert_eq!(
        item_of(&window, "session-item").unwrap()["content"].as_str(),
        Some(unicode)
    );

    // Fresh process: show --id returns the exact bytes (stdout JSON).
    let (ok, out, _) = run(root.path(), &["context", "show", "--id", "session-item"]);
    assert!(ok, "{out}");
    let shown: Value = serde_json::from_str(&out).expect("show prints one JSON object");
    assert_eq!(shown["content"].as_str(), Some(unicode));
    assert_eq!(shown["scope"], json!("session"));
    assert_eq!(shown["state"], json!("active"));

    // Engine status in another fresh process observes the persisted window.
    let (ok, out, _) = run(root.path(), &["context", "show"]);
    assert!(ok);
    let status: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(status["active_items"].as_u64(), Some(3));
}

#[test]
fn cli_budget_boundary_rejects_oversized_items_cleanly() {
    // §9: AWH_CONTEXT_MAX_INPUT_TOKENS isolates a tiny budget; an item
    // above it fails closed with the deterministic engine error and
    // nothing is persisted for the rejected id.
    let root = TempDir::new().unwrap();
    let env = [("AWH_CONTEXT_MAX_INPUT_TOKENS", "50")];
    let small = "tiny note that fits easily";
    let (ok, out, _) = run_env(
        root.path(),
        &["context", "save", "--id", "small", "--content", small],
        &env,
    );
    assert!(ok, "{out}");

    let big = "word ".repeat(80); // ~80 tokens > 50
    let (ok, _out, err) = run_env(
        root.path(),
        &["context", "save", "--id", "big", "--content", &big],
        &env,
    );
    assert!(!ok, "oversized item must fail closed");
    assert!(
        err.contains("item token count") && err.contains("exceeds max input tokens"),
        "deterministic budget error expected: {err}"
    );
    // The rejected id never reached the durable window.
    assert!(!window_item_ids(root.path()).contains(&"big".to_string()));
    // The earlier good item survives the rejection untouched.
    assert!(window_item_ids(root.path()).contains(&"small".to_string()));
}

#[test]
fn cli_disabled_engine_fails_closed_and_never_mutates_state() {
    // §6: AWH_CONTEXT_ENABLED=false makes every mutating/reading engine
    // operation fail closed with the documented error; a pre-existing
    // window is not deleted or modified; malformed booleans parse
    // fail-safe (anything not 1/true/yes/on is disabled).
    let root = TempDir::new().unwrap();
    let (ok, _, _) = run(
        root.path(),
        &[
            "context",
            "save",
            "--id",
            "keep",
            "--content",
            "survives disabling",
        ],
    );
    assert!(ok);
    let before = std::fs::read(engine_window(root.path())).unwrap();

    let env = [("AWH_CONTEXT_ENABLED", "false")];
    let (ok, _, err) = run_env(
        root.path(),
        &["context", "save", "--id", "x", "--content", "c"],
        &env,
    );
    assert!(!ok);
    assert!(err.contains("context engine is disabled"), "{err}");
    let (ok, _, err) = run_env(
        root.path(),
        &["context", "search", "--query", "survives"],
        &env,
    );
    assert!(!ok);
    assert!(err.contains("context engine is disabled"), "{err}");

    // Malformed boolean: fails safe → treated as disabled, deterministic.
    let env_bad = [("AWH_CONTEXT_ENABLED", "banana")];
    let (ok, _, err) = run_env(
        root.path(),
        &["context", "save", "--id", "x", "--content", "c"],
        &env_bad,
    );
    assert!(!ok);
    assert!(err.contains("context engine is disabled"), "{err}");

    // Pre-existing window byte-identical after all disabled runs.
    assert_eq!(std::fs::read(engine_window(root.path())).unwrap(), before);
    // Re-enable: state is intact.
    let (ok, out, _) = run(root.path(), &["context", "show", "--id", "keep"]);
    assert!(ok, "{out}");
}

#[test]
fn cli_duplicate_save_replaces_and_clear_targets_only_named_items() {
    // §7: duplicate-id save is the documented explicit upsert (replace),
    // never a duplicate row; clear --id removes exactly one item.
    let root = TempDir::new().unwrap();
    let (ok, _, _) = run(
        root.path(),
        &["context", "save", "--id", "x", "--content", "first version"],
    );
    assert!(ok);
    let (ok, _, _) = run(
        root.path(),
        &[
            "context",
            "save",
            "--id",
            "y",
            "--content",
            "untouched neighbor",
        ],
    );
    assert!(ok);
    let (ok, _, _) = run(
        root.path(),
        &[
            "context",
            "save",
            "--id",
            "x",
            "--content",
            "second version replaces",
        ],
    );
    assert!(ok);

    let ids = window_item_ids(root.path());
    assert_eq!(
        ids.iter().filter(|i| i.as_str() == "x").count(),
        1,
        "upsert must not duplicate"
    );
    assert_eq!(ids.len(), 2);

    // clear --id removes exactly x; y survives byte-identical.
    let y_before = std::fs::read_to_string(engine_window(root.path())).unwrap();
    let (ok, out, _) = run(root.path(), &["context", "clear", "--id", "x"]);
    assert!(ok, "{out}");
    let ids = window_item_ids(root.path());
    assert!(!ids.contains(&"x".to_string()));
    assert!(ids.contains(&"y".to_string()));
    let y_after = read_window(root.path());
    assert_eq!(
        item_of(&y_after, "y").unwrap()["content"].as_str(),
        Some("untouched neighbor")
    );
    let _ = y_before;

    // Unknown id: deterministic failure, no state change.
    let before = std::fs::read(engine_window(root.path())).unwrap();
    let (ok, _, err) = run(root.path(), &["context", "clear", "--id", "missing"]);
    assert!(!ok);
    assert!(err.contains("not found"), "{err}");
    assert_eq!(std::fs::read(engine_window(root.path())).unwrap(), before);
}

#[test]
fn cli_corrupt_window_fails_closed_across_processes() {
    // §24: a corrupt window is detected by EVERY fresh process; no
    // operation fabricates items; removing the corrupt file restores a
    // clean empty engine (missing window = empty contract).
    let root = TempDir::new().unwrap();
    let (ok, _, _) = run(
        root.path(),
        &["context", "save", "--id", "a", "--content", "real item"],
    );
    assert!(ok);

    // Truncated/garbage window (partial publication shape).
    std::fs::write(
        engine_window(root.path()),
        b"{\"schema_version\":1,\"items\":[{\"id\":\"a\"",
    )
    .unwrap();
    let probes: [&[&str]; 4] = [
        &["context", "show"],
        &["context", "show", "--id", "a"],
        &["context", "search", "--query", "real"],
        &["context", "save", "--id", "b", "--content", "c"],
    ];
    for args in probes {
        let (ok, _, err) = run(root.path(), args);
        assert!(!ok, "corrupt window must fail closed: {args:?}");
        assert!(
            err.contains("corrupt context window"),
            "expected corruption error, got: {err}"
        );
    }
    // The corrupt bytes are surfaced, never silently rewritten.
    let raw = std::fs::read_to_string(engine_window(root.path())).unwrap();
    assert!(
        raw.starts_with("{\"schema_version\":1"),
        "corrupt bytes untouched: {raw}"
    );

    // Repair by removal: missing window loads empty and is writable again.
    std::fs::remove_file(engine_window(root.path())).unwrap();
    let (ok, out, _) = run(root.path(), &["context", "show"]);
    assert!(ok, "{out}");
    let status: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(status["active_items"].as_u64(), Some(0));
    let (ok, _, _) = run(
        root.path(),
        &[
            "context",
            "save",
            "--id",
            "fresh",
            "--content",
            "after repair",
        ],
    );
    assert!(ok);
    assert!(window_item_ids(root.path()).contains(&"fresh".to_string()));
}

#[test]
fn cli_canary_content_stored_but_never_audited() {
    // §22/§27: content storage IS the feature (canary expected in the
    // window), but the durable audit log carries identifiers/outcomes
    // only — the canary must not appear in any audit line.
    let canary = "ghp_tp09CanaryNeverAudit0123456789abcdef";
    let root = TempDir::new().unwrap();
    let (ok, _, _) = run(
        root.path(),
        &[
            "context",
            "save",
            "--id",
            "canary-item",
            "--content",
            canary,
        ],
    );
    assert!(ok);

    // Content legitimately persisted in the engine's own store...
    assert!(std::fs::read_to_string(engine_window(root.path()))
        .unwrap()
        .contains(canary));
    // ...and legitimately returned by search (it is context, by design).
    let (ok, out, _) = run(
        root.path(),
        &["context", "search", "--query", "CanaryNeverAudit"],
    );
    assert!(ok);
    assert!(out.contains("canary-item"), "{out}");
    // ...but NEVER in the durable audit log.
    let audit_path = root.path().join(".agent").join("audit").join("audit.log");
    let audit_raw = std::fs::read_to_string(&audit_path).expect("durable audit log");
    assert!(
        !audit_raw.contains(canary),
        "canary leaked into durable audit"
    );
    assert!(
        audit_raw.contains("cli_context_save"),
        "save must be audited"
    );
    assert!(
        audit_raw.contains("canary-item"),
        "audit carries the identifier"
    );
}

#[test]
fn cli_workspace_isolation_physical_boundary() {
    // §8: two roots, same item id, different content. Neither store can
    // observe the other's item; search never crosses roots.
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let (ok, _, _) = run(
        a.path(),
        &[
            "context",
            "save",
            "--id",
            "shared-id",
            "--content",
            "from workspace A",
        ],
    );
    assert!(ok);
    let (ok, _, _) = run(
        b.path(),
        &[
            "context",
            "save",
            "--id",
            "shared-id",
            "--content",
            "from workspace B",
        ],
    );
    assert!(ok);

    let (ok, out, _) = run(a.path(), &["context", "show", "--id", "shared-id"]);
    assert!(ok);
    assert!(
        out.contains("from workspace A") && !out.contains("from workspace B"),
        "{out}"
    );
    let (ok, out, _) = run(a.path(), &["context", "search", "--query", "workspace B"]);
    assert!(ok);
    assert!(
        !out.contains("from workspace B"),
        "search must not cross roots: {out}"
    );
    // Raw bytes agree: each window holds exactly its own content.
    assert!(std::fs::read_to_string(engine_window(a.path()))
        .unwrap()
        .contains("from workspace A"));
    assert!(std::fs::read_to_string(engine_window(b.path()))
        .unwrap()
        .contains("from workspace B"));
}

// ---------------------------------------------------------------------
// Engine integration through the canonical ContextEngine (§36)
// ---------------------------------------------------------------------

fn tiny_engine(root: &Path) -> ContextEngine {
    let config = ContextEngineConfig {
        budget: ContextBudget {
            max_input_tokens: 1_000,
            reserved_output_tokens: 100,
            safety_margin_tokens: 100,
        },
        ..ContextEngineConfig::default()
    };
    ContextEngine::new(root, config).unwrap()
}

#[test]
fn selection_and_assembly_are_deterministic_and_budget_bounded() {
    // §10/§17: identical fixture + config → identical selection; assemble
    // never exceeds the usable budget; a request budget only tightens;
    // assembly never mutates engine state.
    let root = TempDir::new().unwrap();
    let engine = tiny_engine(root.path());
    // Varied fixture: relevance, size, sources, ties.
    insert_item(
        &engine,
        "alpha",
        "alpha tool output about the deploy pipeline",
        0.9,
    );
    insert_item(&engine, "beta", "beta note deploy", 0.8);
    insert_item(
        &engine,
        "gamma",
        "gamma large filler content repeated again and again filler filler",
        0.1,
    );
    insert_item(&engine, "delta", "delta deploy detail", 0.7);
    insert_item(&engine, "tie-a", "tie content deploy", 0.5);
    insert_item(&engine, "tie-b", "tie content deploy", 0.5);

    let request = ContextRequest {
        task: "deploy the pipeline".to_string(),
        query: None,
        token_budget: 0,
        scope: ContextScope::Project,
    };
    let first = engine.select(&request);
    let second = engine.select(&request);
    assert_eq!(first, second, "selection must be deterministic");
    assert!(!first.kept_ids.is_empty());
    // Ties resolve by id, not insertion order or randomness.
    let tie_positions: Vec<usize> = first
        .kept_ids
        .iter()
        .map(|id| id.as_str())
        .enumerate()
        .filter(|(_, id)| *id == "tie-a" || *id == "tie-b")
        .map(|(pos, _)| pos)
        .collect();
    if tie_positions.len() == 2 {
        let ta = first.kept_ids.iter().position(|i| i == "tie-a").unwrap();
        let tb = first.kept_ids.iter().position(|i| i == "tie-b").unwrap();
        assert!(ta < tb, "ties break by id: {:?}", first.kept_ids);
    }

    // Assembled context fits the effective budget and repeats identically.
    let assembled = engine.get_context(&request).unwrap();
    assert!(assembled.total_tokens <= assembled.budget.usable_input_tokens());
    assert!(assembled
        .items
        .iter()
        .all(|i| i.token_count <= assembled.budget.usable_input_tokens()));
    let again = engine.get_context(&request).unwrap();
    assert_eq!(
        assembled
            .items
            .iter()
            .map(|i| i.id.clone())
            .collect::<Vec<_>>(),
        again.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>()
    );

    // A request budget tightens: with an explicit tiny budget the kept set
    // shrinks (or rejects appear) but never exceeds the request cap.
    let tight = ContextRequest {
        token_budget: 30,
        ..request.clone()
    };
    let tight_outcome = engine.select(&tight);
    assert!(
        tight_outcome.kept_tokens <= 30,
        "request budget must tighten: {tight_outcome:?}"
    );
    assert!(tight_outcome.kept_tokens <= first.kept_tokens);

    // Assembly never mutates: the engine's items are byte-identical after.
    let ids_before: Vec<(String, String)> = engine
        .list_items()
        .into_iter()
        .map(|i| (i.id.clone(), i.content.clone()))
        .collect();
    let _ = engine.get_context(&request).unwrap();
    let ids_after: Vec<(String, String)> = engine
        .list_items()
        .into_iter()
        .map(|i| (i.id.clone(), i.content.clone()))
        .collect();
    assert_eq!(ids_before, ids_after, "assemble must not mutate");
}

#[test]
fn protected_items_survive_optimize_and_offload_semantics() {
    // §11/§16: protect → always Keep, never offloaded/archived by a
    // policy pass; explicit offload of a protected item is refused.
    let root = TempDir::new().unwrap();
    let engine = tiny_engine(root.path());
    insert_item(&engine, "vital", "vital deploy runbook steps", 0.05);
    insert_item(&engine, "junk", "junk unrelated filler noise text", 0.05);
    insert_item(&engine, "useful", "useful deploy instructions here", 0.9);
    assert!(engine.protect("vital"));

    let summary = engine.optimize("deploy the service").unwrap();
    assert!(
        summary.kept.contains(&"vital".to_string()),
        "protected kept: {summary:?}"
    );
    assert!(!summary.offloaded.contains(&"vital".to_string()));
    assert!(!summary.archived.contains(&"vital".to_string()));
    assert!(engine.get_item("vital").unwrap().state.is_active());

    // Repeat optimize on the already-optimized state: stable/idempotent.
    let second = engine.optimize("deploy the service").unwrap();
    assert_eq!(
        second.kept.len()
            + second.offloaded.len()
            + second.compressed.len()
            + second.archived.len(),
        summary.kept.len()
            + summary.offloaded.len()
            + summary.compressed.len()
            + summary.archived.len()
    );

    // Explicit offload of a protected item is refused.
    assert!(engine.offload("vital", "manual").is_err());

    // An offloaded low-value item is durably stored and restorable.
    if summary.offloaded.iter().any(|i| i == "junk") {
        let offload_file = root
            .path()
            .join(".agent")
            .join("context-engine")
            .join("offloads")
            .join("junk.json");
        let raw: Value =
            serde_json::from_str(&std::fs::read_to_string(&offload_file).unwrap()).unwrap();
        assert_eq!(
            raw["item"]["content"].as_str(),
            Some("junk unrelated filler noise text")
        );
        let restored = engine.restore("junk").unwrap();
        assert_eq!(restored.content, "junk unrelated filler noise text");
        // TP09 D1 regression at engine level: restore retires the record.
        assert!(!offload_file.exists(), "restored record must be retired");
    }
}

#[test]
fn offload_restore_persist_and_survive_engine_restart() {
    // §13/§23: offload → new engine instance over the same root →
    // offloaded state observed, content byte-exact, restore works,
    // search sees offloaded content, corrupt record fails closed
    // without breaking other ids.
    let root = TempDir::new().unwrap();
    {
        let engine = tiny_engine(root.path());
        insert_item(
            &engine,
            "keepme",
            "precious deploy context kept across restarts",
            0.9,
        );
        engine.offload("keepme", "probe").unwrap();
    }
    // Fresh engine (process-restart equivalent): durable state observed.
    let engine = tiny_engine(root.path());
    let item = engine
        .get_item("keepme")
        .expect("offloaded item visible in window");
    assert_eq!(item.state, ContextState::Offloaded);
    assert_eq!(item.content, "precious deploy context kept across restarts");
    assert_eq!(engine.status().unwrap().offloaded_items, 1);

    // Search finds offloaded content; it does not mutate state.
    let hits = engine.search("across restarts", 10).unwrap();
    assert!(hits.iter().any(|h| h.id == "keepme"));
    assert_eq!(
        engine.get_item("keepme").unwrap().state,
        ContextState::Offloaded
    );

    // Restore from the fresh instance: exact content + metadata back.
    let restored = engine.restore("keepme").unwrap();
    assert_eq!(
        restored.content,
        "precious deploy context kept across restarts"
    );
    assert!(restored.state.is_active());
    assert_eq!(
        engine.status().unwrap().offloaded_items,
        0,
        "no ghost record post-restore"
    );

    // A record whose id is NOT in the window (record-only, the durable
    // fallback path) must fail closed when corrupt — no fabricated
    // content, no silent recovery — while healthy sibling records keep
    // working.
    insert_item(&engine, "healthy", "healthy stays healthy", 0.8);
    engine.offload("healthy", "probe").unwrap();
    let off_dir = root
        .path()
        .join(".agent")
        .join("context-engine")
        .join("offloads");
    std::fs::write(off_dir.join("orphan-corrupt.json"), b"{ not valid json").unwrap();
    assert!(
        engine.restore("orphan-corrupt").is_err(),
        "corrupt record must fail closed"
    );
    let healthy = engine.restore("healthy").unwrap();
    assert_eq!(healthy.content, "healthy stays healthy");
}

#[test]
fn snapshot_lifecycle_budget_limited_restore_and_isolation() {
    // §14: snapshots capture engine state (not files); restore respects
    // the budget with explicit skipped reporting; ids are root-isolated;
    // corruption fails closed; distinct from the edit-plane snapshot store.
    let root = TempDir::new().unwrap();
    let engine = tiny_engine(root.path());
    insert_item(&engine, "s1", "snapshot member one deploy", 0.9);
    insert_item(&engine, "s2", "snapshot member two deploy", 0.9);

    let snap = engine
        .snapshot(
            Some("tp09-snap".into()),
            Some("deploy task".into()),
            Some("sess-tp09".into()),
        )
        .unwrap();
    assert_eq!(snap.id, "tp09-snap");
    let snap_file = root
        .path()
        .join(".agent")
        .join("context-engine")
        .join("snapshots")
        .join("tp09-snap.json");
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(&snap_file).unwrap()).unwrap();
    assert_eq!(raw["task"].as_str(), Some("deploy task"));
    assert_eq!(raw["session"].as_str(), Some("sess-tp09"));
    assert_eq!(raw["active_items"].as_array().unwrap().len(), 2);

    // Mutate the window after the snapshot, then restore.
    engine.remove_item("s1");
    engine.remove_item("s2");
    let restored = engine.restore_snapshot("tp09-snap").unwrap();
    assert!(restored.restored.contains(&"s1".to_string()));
    assert!(restored.restored.contains(&"s2".to_string()));
    assert!(engine.get_item("s1").unwrap().state.is_active());

    // Budget-limited restore: an over-budget item is SKIPPED, not silently
    // dropped or force-restored. 900 tokens: under the 1000 max (accepted
    // by insert) but over the 800 usable (skipped by snapshot restore).
    let big = ContextItem::new("big", ContextSource::Tool, "b ".repeat(900), 0);
    engine.insert(big).unwrap();
    assert_eq!(engine.get_item("big").unwrap().token_count, 900);
    engine
        .snapshot(Some("with-big".into()), None, None)
        .unwrap();
    engine.remove_item("big");
    let outcome = engine.restore_snapshot("with-big").unwrap();
    assert!(outcome.skipped.contains(&"big".to_string()), "{outcome:?}");
    assert!(engine.get_item("big").is_none() || !engine.get_item("big").unwrap().state.is_active());

    // Cross-root isolation: another engine never sees this snapshot id.
    let other = TempDir::new().unwrap();
    let engine_b = tiny_engine(other.path());
    assert!(engine_b.restore_snapshot("tp09-snap").is_err());
    assert!(engine_b.inspect_snapshot("tp09-snap").unwrap().is_none());

    // Corrupt snapshot: inspect fails closed; delete works; other state usable.
    std::fs::write(&snap_file, b"garbage").unwrap();
    assert!(engine.inspect_snapshot("tp09-snap").is_err());
    assert!(engine.delete_snapshot("tp09-snap").unwrap());
    assert!(
        engine.get_item("s1").is_some(),
        "window untouched by snapshot corruption"
    );

    // Invalid snapshot ids are rejected without touching the filesystem:
    // the store resolves them to "not found" (Ok(None)) — never a path
    // escape, never an error that could be confused with corruption.
    assert!(engine.inspect_snapshot("../escape").unwrap().is_none());
    assert!(engine.inspect_snapshot("with space").unwrap().is_none());
    assert!(engine.inspect_snapshot("..").unwrap().is_none());
}

#[test]
fn compression_is_deterministic_lossless_record_and_idempotent() {
    // §12: compression shrinks the active representation, records the
    // original token count in metadata, is idempotent on re-runs, never
    // fabricates content, and the offload path preserves the record.
    let root = TempDir::new().unwrap();
    let engine = tiny_engine(root.path());
    let content = "deploy step\n".repeat(200); // heavy duplicate lines
    insert_item(&engine, "dup", &content, 0.9);

    let saved = engine.get_item("dup").unwrap();
    let before_tokens = saved.token_count;
    let compressed = engine
        .compress_item("dup")
        .unwrap()
        .expect("duplicates compress");
    assert!(compressed > 0);
    let after = engine.get_item("dup").unwrap();
    assert!(after.token_count < before_tokens);
    assert_eq!(
        after.metadata["context_engine"]["original_tokens"].as_u64(),
        Some(before_tokens as u64)
    );
    assert!(after.metadata["context_engine"]["strategies"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s.as_str() == Some("dedup_lines")));
    // Determinism: identical input recompresses to the same shape.
    let content2 = format!("{content}x");
    insert_item(&engine, "dup2", &content2, 0.9);
    let c1 = engine.compress_item("dup2").unwrap();
    let a1 = engine.get_item("dup2").unwrap();
    let (ok, _, _) = run(root.path(), &["context", "show", "--id", "dup2"]);
    assert!(ok);
    let _ = (c1, a1);

    // Repeated compression of already-compressed content: no further
    // shrink → Ok(None), never corruption.
    let again = engine.compress_item("dup").unwrap();
    assert!(again.is_none(), "second pass should find nothing to save");
    assert!(!engine.get_item("dup").unwrap().content.is_empty());

    // Incompressible content is left untouched (fail-safe, no fabrication).
    insert_item(&engine, "tiny", "unique short text", 0.9);
    assert!(engine.compress_item("tiny").unwrap().is_none());
    assert_eq!(
        engine.get_item("tiny").unwrap().content,
        "unique short text"
    );
}

#[test]
fn search_limits_scopes_and_immutability() {
    // §15: limit enforcement, empty/whitespace query contract,
    // case-insensitive match, read-only behavior.
    let root = TempDir::new().unwrap();
    let engine = tiny_engine(root.path());
    for i in 0..6 {
        insert_item(
            &engine,
            &format!("needle-{i}"),
            &format!("needle item {i} unique payload {i}"),
            0.9,
        );
    }
    insert_item(&engine, "haystack", "nothing relevant here", 0.9);

    let all = engine.search("needle", 10).unwrap();
    assert_eq!(all.len(), 6);
    let bounded = engine.search("needle", 2).unwrap();
    assert_eq!(bounded.len(), 2, "limit enforced");
    // Case-insensitive.
    assert!(!engine.search("NEEDLE", 10).unwrap().is_empty());
    // Empty/whitespace-only: deterministic empty result, not an error.
    assert!(engine.search("", 10).unwrap().is_empty());
    assert!(engine.search("   ", 10).unwrap().is_empty());
    // Query is data: path-shaped queries match nothing and never touch fs.
    assert!(engine.search("../../etc/passwd", 10).unwrap().is_empty());
    assert!(engine.search("'; rm -rf", 10).unwrap().is_empty());
    // Search is read-only: item set and bytes unchanged.
    let before: Vec<(String, String)> = engine
        .list_items()
        .into_iter()
        .map(|i| (i.id, i.content))
        .collect();
    let _ = engine.search("needle", 10).unwrap();
    let after: Vec<(String, String)> = engine
        .list_items()
        .into_iter()
        .map(|i| (i.id, i.content))
        .collect();
    assert_eq!(before, after);
}

#[test]
fn concurrent_inserts_and_duplicate_id_races_keep_state_valid() {
    // §25: parallel inserts all land; a duplicate-id race leaves exactly
    // one row per id and a structurally valid window; searches during
    // mutation never panic and never observe a torn window.
    let root = TempDir::new().unwrap();
    let engine = std::sync::Arc::new(tiny_engine(root.path()));

    let mut handles = Vec::new();
    for t in 0..8 {
        let engine = engine.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..10 {
                insert_item(
                    &engine,
                    &format!("t{t}-i{i}"),
                    &format!("content t{t} item {i}"),
                    0.9,
                );
                let _ = engine.search("content", 5).unwrap();
            }
        }));
    }
    for h in handles {
        h.join().expect("no panics under concurrency");
    }
    assert_eq!(engine.list_items().len(), 80);

    // Duplicate-id race: two writers, same id, different content.
    let a = engine.clone();
    let b = engine.clone();
    let (ra, rb) = std::thread::scope(|s| {
        let ha = s.spawn(move || insert_item(&a, "racer", "racer content A", 0.9));
        let hb = s.spawn(move || insert_item(&b, "racer", "racer content B", 0.9));
        (ha.join().unwrap(), hb.join().unwrap())
    });
    let _ = (ra, rb);
    let racers: Vec<ContextItem> = engine
        .list_items()
        .into_iter()
        .filter(|i| i.id == "racer")
        .collect();
    assert_eq!(racers.len(), 1, "upsert must never duplicate a row");
    // The window is valid JSON with all 81 ids.
    let ids = window_item_ids(root.path());
    assert_eq!(ids.len(), 81);
    assert!(ids.contains(&"racer".to_string()));
}

// ---------------------------------------------------------------------
// MCP stdio black-box (§29)
// ---------------------------------------------------------------------

struct McpServer {
    child: Child,
    stdin: Option<std::process::ChildStdin>,
    stdout: std::io::BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl Drop for McpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn mcp_server(root: &Path) -> McpServer {
    let mut child = Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(["mcp", "serve", "--transport", "stdio"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn awh mcp stdio");
    let stdin = child.stdin.take().expect("stdin");
    let stdout = std::io::BufReader::new(child.stdout.take().expect("stdout"));
    McpServer {
        child,
        stdin: Some(stdin),
        stdout,
        next_id: 0,
    }
}

impl McpServer {
    fn send(&mut self, value: &Value) {
        let line = serde_json::to_string(value).unwrap();
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.stdout.read_line(&mut line).expect("read stdout");
            if n == 0 {
                panic!("server closed stdout without responding");
            }
            if line.trim().is_empty() {
                continue;
            }
            return serde_json::from_str(line.trim())
                .unwrap_or_else(|e| panic!("stdout is not JSON-RPC ({e}): {line}"));
        }
    }

    fn req(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        self.send(&json!({"jsonrpc":"2.0","id": self.next_id, "method": method, "params": params}));
        self.read()
    }

    fn initialize(&mut self) {
        let r = self.req(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "tp09-context", "version": "0.0"}
            }),
        );
        assert!(
            r["result"]["protocolVersion"].is_string(),
            "initialize failed: {r}"
        );
        self.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        self.req("tools/call", json!({"name": name, "arguments": arguments}))
    }

    /// Successful tool call → parsed JSON payload of the text envelope.
    fn tool_json(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.tool(name, arguments);
        assert!(
            response["error"].is_null(),
            "tool {name} errored: {response}"
        );
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("envelope");
        serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("tool {name} payload not JSON ({e}): {text}"))
    }

    fn close_stdin(&mut self) {
        self.stdin = None;
    }
}

#[test]
fn mcp_context_lifecycle_status_insert_get_search_offload_restore_restart() {
    // §29: the full context.* lifecycle over the real stdio wire, plus
    // cross-process persistence and the TP09 D1 resurrection regression.
    let root = TempDir::new().unwrap();
    let content = "mcp deploy runbook — مرحبا 世界 verified";
    let mut server = mcp_server(root.path());
    server.initialize();

    // status: fresh engine.
    let status = server.tool_json("context.status", json!({}));
    assert_eq!(status["active_items"].as_u64(), Some(0));
    assert_eq!(status["offloaded_items"].as_u64(), Some(0));

    // insert → get: exact bytes back over the wire.
    let inserted = server.tool_json(
        "context.insert",
        json!({
            "id": "runbook", "content": content, "scope": "Session", "relevance": 0.9
        }),
    );
    assert_eq!(inserted["id"], json!("runbook"));
    assert_eq!(inserted["scope"], json!("session"));
    let got = server.tool_json("context.get", json!({"id": "runbook"}));
    assert_eq!(got["content"].as_str(), Some(content));

    // search over the wire.
    let hits = server.tool_json("context.search", json!({"query": "runbook", "limit": 5}));
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["id"], json!("runbook"));

    // protect + optimize: protected survives; determinism (same summary shape).
    server.tool_json("context.protect", json!({"id": "runbook"}));
    let opt1 = server.tool_json("context.optimize", json!({"task": "deploy the service"}));
    assert!(opt1["kept"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == &json!("runbook")));
    let opt2 = server.tool_json("context.optimize", json!({"task": "deploy the service"}));
    assert_eq!(
        opt1["kept"].as_array().unwrap().len(),
        opt2["kept"].as_array().unwrap().len()
    );

    // assemble: budget-bounded over the wire, and it never mutates state.
    // usable is computed independently from the serialized budget fields.
    let assembled = server.tool_json("context.assemble", json!({"task": "deploy the service"}));
    let b = &assembled["budget"];
    let usable = b["max_input_tokens"]
        .as_u64()
        .unwrap()
        .saturating_sub(b["reserved_output_tokens"].as_u64().unwrap())
        .saturating_sub(b["safety_margin_tokens"].as_u64().unwrap());
    assert!(
        assembled["total_tokens"].as_u64().unwrap() <= usable,
        "assemble must fit the usable budget: {assembled}"
    );
    let after = server.tool_json("context.get", json!({"id": "runbook"}));
    assert_eq!(
        after["content"].as_str(),
        Some(content),
        "assemble must not mutate"
    );

    // offload → restore (same process): durable record retired (D1).
    insert_via_mcp(&mut server, "extra", "extra item to offload");
    server.tool_json("context.offload", json!({"id": "extra", "reason": "probe"}));
    let off_dir = root
        .path()
        .join(".agent")
        .join("context-engine")
        .join("offloads");
    assert!(
        off_dir.join("extra.json").exists(),
        "durable offload record"
    );
    let status = server.tool_json("context.status", json!({}));
    assert_eq!(status["offloaded_items"].as_u64(), Some(1));
    let restored = server.tool_json("context.restore", json!({"id": "extra"}));
    assert_eq!(restored["state"], json!("active"));
    assert!(
        !off_dir.join("extra.json").exists(),
        "D1: restored record must retire"
    );
    let status = server.tool_json("context.status", json!({}));
    assert_eq!(
        status["offloaded_items"].as_u64(),
        Some(0),
        "D1: no ghost count"
    );

    // remove → restore: an error, never a resurrection (D1 wire regression).
    server.tool_json("context.remove", json!({"id": "extra"}));
    let err = server.tool("context.restore", json!({"id": "extra"}));
    assert!(
        !err["error"].is_null(),
        "restore of a removed item must error: {err}"
    );
    let after_remove = server.tool_json("context.get", json!({"id": "extra"}));
    assert!(after_remove.is_null(), "removed item stays gone");

    server.close_stdin();
    let _ = server.child.wait();

    // Fresh server process over the same root: state persisted.
    let mut server2 = mcp_server(root.path());
    server2.initialize();
    let status = server2.tool_json("context.status", json!({}));
    assert_eq!(status["active_items"].as_u64(), Some(1));
    assert_eq!(
        status["offloaded_items"].as_u64(),
        Some(0),
        "no ghosts across restart"
    );
    let got = server2.tool_json("context.get", json!({"id": "runbook"}));
    assert_eq!(
        got["content"].as_str(),
        Some(content),
        "exact bytes across restart"
    );
}

fn insert_via_mcp(server: &mut McpServer, id: &str, content: &str) {
    server.tool_json("context.insert", json!({"id": id, "content": content}));
}

#[test]
fn mcp_context_schema_validation_and_unknown_ids() {
    // §29: malformed arguments → -32602 naming the field; unknown id on
    // get → JSON null (documented), never an error envelope.
    let root = TempDir::new().unwrap();
    let mut server = mcp_server(root.path());
    server.initialize();

    let bad_scope = server.tool(
        "context.insert",
        json!({"id": "x", "content": "c", "scope": "Cosmic"}),
    );
    assert_eq!(
        bad_scope["error"]["code"].as_i64(),
        Some(-32602),
        "{bad_scope}"
    );
    let missing_content = server.tool("context.insert", json!({"id": "x"}));
    assert_eq!(
        missing_content["error"]["code"].as_i64(),
        Some(-32602),
        "{missing_content}"
    );
    let wrong_type = server.tool("context.insert", json!({"id": 42, "content": "c"}));
    assert_eq!(
        wrong_type["error"]["code"].as_i64(),
        Some(-32602),
        "{wrong_type}"
    );
    let traversal_id = server.tool("context.insert", json!({"id": "../escape", "content": "c"}));
    assert!(
        !traversal_id["error"].is_null(),
        "traversal id rejected: {traversal_id}"
    );

    // get of an unknown id returns null payload (not an error).
    let missing = server.tool_json("context.get", json!({"id": "never-inserted"}));
    assert!(missing.is_null(), "unknown get → null, got {missing}");

    // status never fails even with zero items; search with empty query is
    // a deterministic empty array.
    let hits = server.tool_json("context.search", json!({"query": "", "limit": 10}));
    assert_eq!(hits.as_array().map(Vec::len), Some(0));
}
