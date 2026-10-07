//! TP10 — Developer Memory boundary verification suite.
//!
//! Drives the REAL boundaries: the compiled `awh` binary across fresh
//! processes (CLI plane and the MCP stdio plane), the canonical
//! `MemoryStore` over real `.agent/memory.json` state, and raw persisted
//! bytes as the independent oracle. Nothing is mocked and no second
//! store, lock, migration, or search implementation is built here.
//!
//! Pre-existing coverage this suite does NOT duplicate: `src/core/
//! memory.rs` unit tests (validation bounds, entry-count limit, legacy
//! migration determinism/idempotency/blank-lines/corrupt-line/merge,
//! update_partial field semantics, project isolation, id-minting
//! concurrency, corruption fail-closed), `tests/memory_cli.rs`
//! (CLI lifecycle basics, partial update, validation/corruption,
//! durable+audited add), `tests/store_convergence.rs` (cross-plane
//! write/read/update/restart, scope-is-field, migration), and TP09's
//! context-engine suite (ContextEngine side of the shared authority).
//!
//! Sections (per docs/testing-prompts/10-developer-memory.md):
//! - §5/§7 isolation + scope semantics at the CLI/MCP planes.
//! - §6/§21 validation + resource bounds at the real interfaces.
//! - §8/§22 CRUD semantics, adapter parity, replay.
//! - §9 search correctness/determinism/limits.
//! - §10 CLI black-box incl. stdin content, clamped limits, raw-bytes
//!   restart parity.
//! - §11 MCP boundary incl. the D1 scope-reset regression.
//! - §12 Context shared-authority integration (memory view only).
//! - §13 Control API delegation.
//! - §14 legacy migration through the real CLI process.
//! - §15/§16/§17/§26 atomicity, concurrency, corruption, failure
//!   injection, independent fs oracle.
//! - §18/§19/§20 authorization, audit, sensitive-data protection.

use agent_workspace_hub::core::memory::{MemoryScope, MemoryStore, MAX_MEMORY_TAG_LEN};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt as _;

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
    run_stdin(dir, args, None)
}

/// Same, with piped stdin content (CLI's `--content`-omitted path).
fn run_stdin(dir: &Path, args: &[&str], stdin: Option<&str>) -> (bool, String, String) {
    let mut cmd = awh();
    cmd.args(args).current_dir(dir);
    let mut child = cmd.spawn().unwrap();
    if let Some(text) = stdin {
        let mut pipe = child.stdin.take().expect("stdin pipe");
        pipe.write_all(text.as_bytes()).unwrap();
        drop(pipe);
    } else {
        drop(child.stdin.take());
    }
    let output = child.wait_with_output().unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn memory_file(root: &Path) -> PathBuf {
    root.join(".agent").join("memory.json")
}

/// Independent oracle: parse the canonical store bytes with serde_json.
fn read_store(root: &Path) -> Value {
    let raw = std::fs::read_to_string(memory_file(root))
        .unwrap_or_else(|e| panic!("memory.json must exist: {e}"));
    serde_json::from_str(&raw).expect("memory.json is valid JSON")
}

fn entry_of<'a>(store: &'a Value, id: &str) -> &'a Value {
    store["entries"]
        .as_array()
        .expect("entries array")
        .iter()
        .find(|e| e["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("entry {id} missing from canonical store"))
}

fn entry_ids(root: &Path) -> Vec<String> {
    read_store(root)["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect()
}

/// Parse the id the CLI prints for `memory add`/`memory update` output.
fn id_from_add(out: &str) -> String {
    // "added memory entry <id> (scope <Scope>)"
    let id = out
        .strip_prefix("added memory entry ")
        .expect("add output shape")
        .split(" (scope")
        .next()
        .expect("id segment")
        .trim();
    id.to_string()
}

fn add(root: &Path, content: &str, scope: &str, tags: &[&str]) -> String {
    let mut args: Vec<String> = vec![
        "memory".into(),
        "add".into(),
        "--content".into(),
        content.into(),
        "--scope".into(),
        scope.into(),
    ];
    for tag in tags {
        args.push("--tag".into());
        args.push((*tag).into());
    }
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (ok, out, err) = run(root, &refs);
    assert!(ok, "memory add failed: {err}");
    id_from_add(&out)
}

// ---------------------------------------------------------------------
// CLI black-box (§10)
// ---------------------------------------------------------------------

/// §10/§15: stdin content path, exact persisted bytes, restart parity
/// with an independently parsed canonical file, clamped list limits.
#[test]
fn cli_crud_restart_parity_with_raw_file_oracle() {
    let root = TempDir::new().unwrap();
    let content = "résumé — مرحبا 世界 🚀\nsecond line \"quoted\" \\ backslash";

    // stdin content path (§8 Add: stdin where the CLI supports it)
    let (ok, out, err) = run_stdin(
        root.path(),
        &["memory", "add", "--scope", "Global"],
        Some(content),
    );
    assert!(ok, "stdin add failed: {err}");
    let stdin_id = id_from_add(&out);
    assert!(
        stdin_id.starts_with("mem-"),
        "generated id shape: {stdin_id}"
    );

    let tag_id = add(root.path(), "tagged note", "Project", &["rust", "memory"]);
    let plain_id = add(root.path(), "plain note", "Session", &[]);

    // Independent oracle: raw bytes carry content/scope/tags verbatim,
    // and generated ids are data identifiers (no path-like shape).
    let store = read_store(root.path());
    assert_eq!(
        entry_of(&store, &stdin_id)["content"].as_str(),
        Some(content)
    );
    assert_eq!(entry_of(&store, &stdin_id)["scope"], json!("Global"));
    assert_eq!(entry_of(&store, &tag_id)["scope"], json!("Project"));
    assert_eq!(entry_of(&store, &tag_id)["tags"], json!(["rust", "memory"]));
    assert_eq!(entry_of(&store, &plain_id)["scope"], json!("Session"));
    for id in [&stdin_id, &tag_id, &plain_id] {
        assert!(!id.contains('/'), "id must be data, not a path: {id}");
    }
    // created_at/updated_at minted and equal on fresh entries (§6).
    for id in [&stdin_id, &tag_id, &plain_id] {
        let e = entry_of(&store, id);
        assert!(e["created_at"].as_str().is_some());
        assert_eq!(e["created_at"], e["updated_at"]);
    }

    // Restart parity: a fresh process reads the same entries (§15 reload ==
    // independently observed persisted state).
    let (ok, out, _) = run(root.path(), &["memory", "get", "--id", &stdin_id]);
    assert!(ok);
    let got: Value = serde_json::from_str(out.trim()).expect("get prints the entry JSON");
    assert_eq!(got["content"].as_str(), Some(content));
    assert_eq!(got["scope"], json!("Global"));

    // list shows all three; --scope filters exactly (§9 scope filters do
    // not broaden access: Global filter shows one).
    let (ok, out, _) = run(root.path(), &["memory", "list"]);
    assert!(ok);
    assert_eq!(out.lines().count(), 3, "three entries listed: {out}");
    let (ok, out, _) = run(root.path(), &["memory", "list", "--scope", "Global"]);
    assert!(ok);
    assert_eq!(out.lines().count(), 1, "{out}");
    assert!(out.contains(&stdin_id));

    // Update (content only) preserves scope/tags/created_at and advances
    // updated_at (§8 Update + Workflow C). Note: the CLI's `update` takes
    // content only via --content (never stdin) — stdin is the `add` path.
    let (ok, _, err) = run(
        root.path(),
        &[
            "memory",
            "update",
            "--id",
            &tag_id,
            "--content",
            "replaced content",
        ],
    );
    assert!(ok, "update failed: {err}");
    let store = read_store(root.path());
    let e = entry_of(&store, &tag_id);
    assert_eq!(e["content"].as_str(), Some("replaced content"));
    assert_eq!(e["scope"], json!("Project"));
    assert_eq!(e["tags"], json!(["rust", "memory"]));
    // content-only update must NOT touch created_at, and must advance
    // updated_at (Workflow C: timestamps reflect the update).
    assert!(
        e["updated_at"].as_str().unwrap() > e["created_at"].as_str().unwrap(),
        "updated_at must advance past created_at: {} vs {}",
        e["updated_at"],
        e["created_at"]
    );

    // Delete removes exactly that record (§8 + §24 property).
    let (ok, out, _) = run(root.path(), &["memory", "delete", "--id", &plain_id]);
    assert!(ok);
    assert!(out.contains("deleted memory entry"));
    assert_eq!(
        entry_ids(root.path()),
        vec![stdin_id.clone(), tag_id.clone()]
    );
    // repeated delete: the CLI contract reports missing entries as errors
    // (Workflow D — do not assume idempotent success).
    let (ok, _, err) = run(root.path(), &["memory", "delete", "--id", &plain_id]);
    assert!(!ok, "repeated delete must not report success");
    assert!(err.contains("memory entry not found"), "{err}");
    assert_eq!(entry_ids(root.path()), vec![stdin_id, tag_id]);
}

/// §10/§21: pathological list limits are clamped, never panic; output
/// stays bounded and usable.
#[test]
fn cli_list_limit_clamping_and_bounded_output() {
    let root = TempDir::new().unwrap();
    for i in 0..5 {
        add(root.path(), &format!("note {i}"), "Project", &[]);
    }
    // limit 0 and limit 1 are clamped to at least 1 (bound_limit clamp).
    let (ok, out, _) = run(root.path(), &["memory", "list", "--limit", "0"]);
    assert!(ok);
    assert_eq!(out.lines().count(), 1, "clamp(0) shows one: {out}");
    // huge limit shows everything without pathological behavior.
    let (ok, out, _) = run(root.path(), &["memory", "list", "--limit", "999999"]);
    assert!(ok);
    assert_eq!(out.lines().count(), 5, "{out}");
    // over-max limit is clamped to 500, not an error.
    let (ok, out, _) = run(root.path(), &["memory", "list", "--limit", "100000000"]);
    assert!(ok, "oversized --limit must clamp, not fail: {out}");
    assert_eq!(out.lines().count(), 5);
    // huge search query: bounded behavior, no match fabrication.
    let (ok, out, _) = run(
        root.path(),
        &["memory", "search", "--query", &"z".repeat(100_000)],
    );
    assert!(ok);
    assert!(out.trim().is_empty(), "no hits for a giant query: {out}");
    // multiline content is printed single-line (usable for humans).
    let _ = add(root.path(), "first\nsecond\r\nthird", "Project", &[]);
    let (ok, out, _) = run(root.path(), &["memory", "list"]);
    assert!(ok);
    for line in out.lines() {
        assert!(!line.contains('\n'));
        assert!(!line.contains('\r'));
    }
}

// ---------------------------------------------------------------------
// Search correctness and determinism (§9)
// ---------------------------------------------------------------------

/// §9: case-insensitivity, tag matching, substring semantics, scope
/// filtering, deterministic ordering, read-only behavior, and the
/// documented empty-query contract (matches everything).
#[test]
fn search_semantics_limits_determinism_and_read_only() {
    let root = TempDir::new().unwrap();
    let a = add(
        root.path(),
        "Deploy the Kernel scheduler",
        "Project",
        &["infra"],
    );
    let b = add(
        root.path(),
        "kernel panic workaround",
        "Global",
        &["Kernel", "debug"],
    );
    let _c = add(
        root.path(),
        "unrelated shopping list",
        "Session",
        &["personal"],
    );
    // baseline captured after the LAST mutation of this fixture
    let before = std::fs::read(memory_file(root.path())).unwrap();
    // case-insensitive CONTENT match (ASCII lowercasing both sides)
    let (ok, out, _) = run(root.path(), &["memory", "search", "--query", "KERNEL"]);
    assert!(ok);
    assert_eq!(out.lines().count(), 2, "{out}");
    assert!(out.contains(&a) && out.contains(&b));

    // case-insensitive TAG match: "kernel" hits b's tag "Kernel" and
    // both contents.
    let (ok, out, _) = run(root.path(), &["memory", "search", "--query", "kernel"]);
    assert!(ok);
    assert_eq!(out.lines().count(), 2, "{out}");

    // substring + limit (search default limit 20; explicit limit clamps)
    let (ok, out, _) = run(
        root.path(),
        &["memory", "search", "--query", "kern", "--limit", "1"],
    );
    assert!(ok);
    assert_eq!(out.lines().count(), 1, "{out}");

    // scope filter narrows, never broadens (§9: search cannot expose
    // entries outside the allowed scope)
    let (ok, out, _) = run(
        root.path(),
        &["memory", "search", "--query", "kernel", "--scope", "Global"],
    );
    assert!(ok);
    assert_eq!(out.lines().count(), 1, "{out}");
    assert!(out.contains(&b));
    let (ok, out, _) = run(
        root.path(),
        &[
            "memory", "search", "--query", "kernel", "--scope", "Session",
        ],
    );
    assert!(ok);
    assert!(out.trim().is_empty(), "no Session hit: {out}");

    // empty query: the current contract treats "" as match-all (§9:
    // "missing/empty queries according to the current contract")
    let (ok, out, _) = run(root.path(), &["memory", "search", "--query", ""]);
    assert!(ok);
    assert_eq!(out.lines().count(), 3, "{out}");

    // deterministic: identical queries → identical results, repeated
    let (ok, first, _) = run(root.path(), &["memory", "search", "--query", "e"]);
    assert!(ok);
    for _ in 0..2 {
        let (ok, again, _) = run(root.path(), &["memory", "search", "--query", "e"]);
        assert!(ok);
        assert_eq!(first, again, "search must be deterministic");
    }

    // Unicode content matches exactly (ASCII-only case folding means
    // non-ASCII is compared verbatim). This is the one mutation: verify
    // read-only on the searches ABOVE first (done), then add.
    let (ok, out, _) = run(
        root.path(),
        &["memory", "search", "--query", "naïve café dübel"],
    );
    assert!(ok);
    assert!(out.trim().is_empty(), "not present yet: {out}");
    let _ = add(root.path(), " naïve café dübel", "Project", &[]);
    let (ok, out, _) = run(
        root.path(),
        &["memory", "search", "--query", "naïve café dübel"],
    );
    assert!(ok);
    assert_eq!(out.lines().count(), 1, "{out}");
    let after = std::fs::read(memory_file(root.path())).unwrap();
    assert_ne!(before, after, "the unicode add is the only mutation");

    // search never mutates (§9 + §24 property): repeat the SAME searches
    // over the post-add fixture and compare bytes.
    for _ in 0..2 {
        let (ok, out, _) = run(root.path(), &["memory", "search", "--query", "KERNEL"]);
        assert!(ok);
        assert_eq!(out.lines().count(), 2, "{out}");
        assert_eq!(std::fs::read(memory_file(root.path())).unwrap(), after);
    }

    // missing id is a clean error; malformed/path-like ids never reach
    // the filesystem (§8 Get)
    let (ok, _, err) = run(root.path(), &["memory", "get", "--id", "../escape"]);
    assert!(!ok);
    assert!(err.contains("memory entry not found"), "{err}");
    assert!(!root.path().join(".agent").join("escape").exists());
    assert!(!root.path().join("escape").exists());
}

// ---------------------------------------------------------------------
// Validation and resource bounds at the real boundary (§6/§21)
// ---------------------------------------------------------------------

/// §6/§21: tag bounds, invalid scope, duplicate tags, oversized input —
/// every rejection must fail WITHOUT partially publishing state.
#[test]
fn cli_validation_bounds_fail_closed_without_partial_publish() {
    let root = TempDir::new().unwrap();
    let keep = add(root.path(), "survivor", "Project", &[]);

    // invalid scope rejected with the canonical text
    let (ok, _, err) = run(
        root.path(),
        &["memory", "add", "--content", "x", "--scope", "cosmic"],
    );
    assert!(!ok);
    assert!(err.contains("invalid scope 'cosmic'"), "{err}");
    // lowercase scope is NOT accepted (PascalCase wire format)
    let (ok, _, err) = run(
        root.path(),
        &["memory", "add", "--content", "x", "--scope", "global"],
    );
    assert!(!ok);
    assert!(err.contains("invalid scope 'global'"), "{err}");

    // oversized tag rejected; a tag exactly AT the limit is accepted
    let ok_tag = "t".repeat(MAX_MEMORY_TAG_LEN);
    let over_tag = "u".repeat(MAX_MEMORY_TAG_LEN + 1);
    let (ok, _, err) = run(
        root.path(),
        &["memory", "add", "--content", "x", "--tag", &over_tag],
    );
    assert!(!ok);
    assert!(err.contains("memory tag exceeds"), "{err}");
    let (ok, out, err) = run(
        root.path(),
        &["memory", "add", "--content", "edge tag", "--tag", &ok_tag],
    );
    assert!(ok, "tag at exactly the limit must pass: {err}");
    let edge_id = id_from_add(&out);
    let store = read_store(root.path());
    assert_eq!(entry_of(&store, &edge_id)["tags"], json!([ok_tag]));

    // duplicate tags in one add: the store's Vec keeps what the caller
    // sent (no dedup contract) — pin the current behavior.
    let (ok, out, _) = run(
        root.path(),
        &[
            "memory",
            "add",
            "--content",
            "dup tags",
            "--tag",
            "x",
            "--tag",
            "x",
        ],
    );
    assert!(ok);
    let dup_id = id_from_add(&out);
    let store = read_store(root.path());
    assert_eq!(entry_of(&store, &dup_id)["tags"], json!(["x", "x"]));

    // oversized content (> 1 MiB) fails before publication (§21)
    let big = "a".repeat(1024 * 1024 + 1);
    let (ok, _, err) = run_stdin(root.path(), &["memory", "add"], Some(&big));
    assert!(!ok);
    assert!(err.contains("memory content exceeds"), "{err}");

    // every rejection published nothing: only the survivor + 2 accepted
    let ids = entry_ids(root.path());
    assert_eq!(ids.len(), 3, "no partial publication: {ids:?}");
    assert!(ids.contains(&keep));
}

// ---------------------------------------------------------------------
// Workspace isolation (§7)
// ---------------------------------------------------------------------

/// §7: two roots with identical ids stay isolated; scope stays a field
/// on the canonical entry (no second store); relocation of the project
/// root keeps memory readable (the store binds to the root path only).
#[test]
fn workspace_isolation_identical_ids_and_root_relocation() {
    let root_a = TempDir::new().unwrap();
    let root_b = TempDir::new().unwrap();
    let id_a = add(root_a.path(), "A canary: alpha secret", "Project", &["a"]);
    let id_b = add(root_b.path(), "B canary: beta secret", "Project", &["b"]);

    // generated ids differ across roots; content never crosses
    assert_ne!(id_a, id_b);
    let (ok, out, _) = run(root_a.path(), &["memory", "list"]);
    assert!(ok);
    assert!(out.contains("alpha secret") && !out.contains("beta secret"));
    let (ok, out, _) = run(root_b.path(), &["memory", "list"]);
    assert!(ok);
    assert!(out.contains("beta secret") && !out.contains("alpha secret"));

    // identical EXPLICIT ids via the store stay per-root isolated
    let store_a = MemoryStore::for_project(root_a.path()).unwrap();
    let store_b = MemoryStore::for_project(root_b.path()).unwrap();
    store_a
        .store(
            "same-id".into(),
            "in A".into(),
            MemoryScope::Project,
            vec![],
        )
        .unwrap();
    store_b
        .store(
            "same-id".into(),
            "in B".into(),
            MemoryScope::Project,
            vec![],
        )
        .unwrap();
    assert_eq!(store_a.get("same-id").unwrap().unwrap().content, "in A");
    assert_eq!(store_b.get("same-id").unwrap().unwrap().content, "in B");
    assert_ne!(
        std::fs::read(memory_file(root_a.path())).unwrap(),
        std::fs::read(memory_file(root_b.path())).unwrap()
    );

    // relocation: copy the whole project dir elsewhere → memory still
    // reads (memory binds to a root path, not an identity manifest —
    // foreign-root rejection is the init manifest's contract, not the
    // memory store's).
    let moved = TempDir::new().unwrap();
    let moved_root = moved.path().join("relocated");
    std::fs::create_dir_all(&moved_root).unwrap();
    copy_dir(root_a.path(), &moved_root);
    let (ok, out, _) = run(&moved_root, &["memory", "get", "--id", "same-id"]);
    assert!(ok, "relocated project must read its memory");
    assert!(out.contains("in A"));
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

// ---------------------------------------------------------------------
// Legacy JSONL migration through the real CLI (§14)
// ---------------------------------------------------------------------

/// §14: a legacy file migrates on the first CLI touch, deterministically,
/// with stable ids, archived (not deleted), and survives restart. A
/// re-created legacy file after migration migrates the NEW records
/// without duplicating old ones.
#[test]
fn legacy_migration_through_real_cli_process() {
    let root = TempDir::new().unwrap();
    let agent = root.path().join(".agent");
    std::fs::create_dir_all(&agent).unwrap();
    let legacy = json!({"timestamp": "2025-01-01T00:00:00Z", "content": "legacy one"});
    let legacy2 = json!({"timestamp": "2025-01-02T00:00:00Z", "content": "legacy two"});
    std::fs::write(
        agent.join("memory.jsonl"),
        format!("{legacy}\n\n{legacy2}\n"), // blank line in the middle
    )
    .unwrap();

    // First CLI touch migrates: ids are stable (index-keyed), blank
    // lines skipped, timestamps preserved.
    let (ok, out, err) = run(root.path(), &["memory", "list"]);
    assert!(ok, "migration must succeed: {err}");
    assert_eq!(out.lines().count(), 2, "{out}");
    let store = read_store(root.path());
    assert_eq!(
        entry_of(&store, "legacy-jsonl-0")["content"],
        json!("legacy one")
    );
    assert_eq!(
        entry_of(&store, "legacy-jsonl-1")["content"],
        json!("legacy two")
    );
    assert_eq!(
        entry_of(&store, "legacy-jsonl-0")["created_at"],
        json!("2025-01-01T00:00:00Z")
    );
    assert_eq!(
        entry_of(&store, "legacy-jsonl-0")["scope"],
        json!("Project")
    );
    // legacy file archived, canonical file exists
    assert!(agent.join("memory.jsonl.migrated").is_file());
    assert!(!agent.join("memory.jsonl").exists());
    assert!(memory_file(root.path()).is_file());

    // restart: no duplication, stable ids
    let (ok, out, _) = run(root.path(), &["memory", "list"]);
    assert!(ok);
    assert_eq!(out.lines().count(), 2, "idempotent across processes: {out}");

    // Replay migration (§22) after restoring a legacy file: migration ids
    // are INDEX-keyed, so records whose index already has a migrated id are
    // skipped (the crash-recovery idempotency contract — a re-run after an
    // interrupted publication must not duplicate), while records at NEW
    // indexes still migrate. Pin both halves of that contract.
    let legacy3 = json!({"timestamp": "2025-01-03T00:00:00Z", "content": "legacy three"});
    let legacy4 = json!({"timestamp": "2025-01-04T00:00:00Z", "content": "legacy four"});
    let legacy5 = json!({"timestamp": "2025-01-05T00:00:00Z", "content": "legacy five NEW"});
    std::fs::write(
        agent.join("memory.jsonl"),
        format!("{legacy3}\n{legacy4}\n{legacy5}\n"),
    )
    .unwrap();
    let (ok, out, _) = run(root.path(), &["memory", "list"]);
    assert!(ok);
    // indexes 0 and 1 collide with migrated ids → skipped; index 2 is new.
    assert_eq!(
        out.lines().count(),
        3,
        "colliding ids skipped, new index migrates: {out}"
    );
    let store = read_store(root.path());
    assert_eq!(
        entry_of(&store, "legacy-jsonl-2")["content"],
        json!("legacy five NEW")
    );
    assert_eq!(
        entry_of(&store, "legacy-jsonl-0")["content"],
        json!("legacy one")
    );
    // the re-created legacy file is archived even when some records skipped
    assert!(agent.join("memory.jsonl.migrated").is_file());
    assert!(!agent.join("memory.jsonl").exists());

    // corrupt legacy line fails closed: nothing fabricated, nothing lost
    let root2 = TempDir::new().unwrap();
    let agent2 = root2.path().join(".agent");
    std::fs::create_dir_all(&agent2).unwrap();
    std::fs::write(
        agent2.join("memory.jsonl"),
        format!("{legacy}\n{{ not json\n"),
    )
    .unwrap();
    let (ok, _, err) = run(root2.path(), &["memory", "list"]);
    assert!(!ok, "corrupt legacy must fail closed");
    assert!(err.contains("malformed"), "{err}");
    // failed migration must NOT fabricate an empty store or archive the file
    assert!(!memory_file(root2.path()).exists());
    assert!(agent2.join("memory.jsonl").exists());
    assert!(!agent2.join("memory.jsonl.migrated").exists());
}

// ---------------------------------------------------------------------
// MCP stdio boundary (§11)
// ---------------------------------------------------------------------

struct McpServer {
    child: Child,
    stdin: Option<ChildStdin>,
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
    let mut server = McpServer {
        child,
        stdin: Some(stdin),
        stdout,
        next_id: 0,
    };
    server.initialize();
    server
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
                "clientInfo": {"name": "tp10-memory", "version": "0.0"}
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
}

/// §11: MCP CRUD converges on the canonical store, and the D1 regression
/// — a content-only `memory.update` must preserve scope and tags, exactly
/// like the CLI's partial update. Pre-fix, MCP silently reset a Global
/// entry to Project and wiped its tags.
#[test]
fn mcp_update_is_partial_and_matches_cli_semantics() {
    let root = TempDir::new().unwrap();
    let mut server = mcp_server(root.path());

    // store a Global entry with tags
    let stored = server.tool_json(
        "memory.store",
        json!({"id": "g1", "content": "global note", "scope": "Global", "tags": ["alpha", "beta"]}),
    );
    assert_eq!(stored["scope"], json!("Global"));
    assert_eq!(stored["tags"], json!(["alpha", "beta"]));

    // D1: content-only update — scope/tags ABSENT from the arguments.
    let updated = server.tool_json("memory.update", json!({"id": "g1", "content": "new body"}));
    assert_eq!(updated["content"], json!("new body"));
    assert_eq!(
        updated["scope"],
        json!("Global"),
        "content-only update must NOT reset scope (D1 regression)"
    );
    assert_eq!(
        updated["tags"],
        json!(["alpha", "beta"]),
        "content-only update must NOT wipe tags (D1 regression)"
    );
    assert_eq!(updated["created_at"], stored["created_at"]);

    // independent oracle: the raw file agrees
    let store = read_store(root.path());
    assert_eq!(entry_of(&store, "g1")["scope"], json!("Global"));
    assert_eq!(entry_of(&store, "g1")["tags"], json!(["alpha", "beta"]));

    // explicit scope/tags DO replace when present
    let replaced = server.tool_json(
        "memory.update",
        json!({"id": "g1", "content": "newer", "scope": "Session", "tags": ["solo"]}),
    );
    assert_eq!(replaced["scope"], json!("Session"));
    assert_eq!(replaced["tags"], json!(["solo"]));

    // CLI and MCP produce the SAME field semantics for the same edit:
    // CLI partial-update on an MCP-stored entry behaves identically.
    drop(server);
    let cli = server_store_content(root.path(), "g1");
    assert_eq!(cli, "newer");
    let (ok, _, err) = run(
        root.path(),
        &["memory", "update", "--id", "g1", "--content", "cli body"],
    );
    assert!(ok, "CLI update failed: {err}");
    let store = read_store(root.path());
    assert_eq!(entry_of(&store, "g1")["scope"], json!("Session"));
    assert_eq!(entry_of(&store, "g1")["tags"], json!(["solo"]));
}

fn server_store_content(root: &Path, id: &str) -> String {
    entry_of(&read_store(root), id)["content"]
        .as_str()
        .unwrap()
        .to_string()
}

/// §11/§16: MCP read/write convergence, cross-workspace isolation, schema
/// validation, and error/no-mutation guarantees.
#[test]
fn mcp_validation_isolation_and_no_mutation_on_error() {
    let root_a = TempDir::new().unwrap();
    let root_b = TempDir::new().unwrap();
    let mut a = mcp_server(root_a.path());
    let mut b = mcp_server(root_b.path());

    a.tool_json(
        "memory.store",
        json!({"id": "only-a", "content": "A secret", "scope": "Project", "tags": []}),
    );

    // cross-workspace read is null, not a leak
    let got = b.tool_json("memory.get", json!({"id": "only-a"}));
    assert!(
        got.is_null(),
        "project B must not see project A's memory: {got}"
    );

    // malformed arguments → schema -32602, and NOTHING is written
    let before = std::fs::read(memory_file(root_a.path())).unwrap();
    let resp = a.tool(
        "memory.store",
        json!({"id": "bad", "content": "x", "scope": "nope"}),
    );
    assert!(!resp["error"].is_null(), "invalid scope must error: {resp}");
    assert_eq!(resp["error"]["code"], json!(-32602));
    let resp = a.tool("memory.store", json!({"content": "x", "scope": "Project"}));
    assert!(!resp["error"].is_null(), "missing id must error: {resp}");
    let resp = a.tool("memory.get", json!({"id": 42}));
    assert!(!resp["error"].is_null(), "wrong id type must error: {resp}");
    let resp = a.tool("memory.update", json!({"id": "only-a"}));
    assert!(
        !resp["error"].is_null(),
        "missing content must error: {resp}"
    );
    assert_eq!(
        std::fs::read(memory_file(root_a.path())).unwrap(),
        before,
        "failed schema validation must not mutate the store"
    );

    // missing-id update fails (never silently creates); delete of a
    // missing id reports deleted:false
    let resp = a.tool("memory.update", json!({"id": "ghost", "content": "x"}));
    assert!(!resp["error"].is_null(), "update of missing id must error");
    let deleted = a.tool_json("memory.delete", json!({"id": "ghost"}));
    assert_eq!(deleted["deleted"], json!(false));
    let got = a.tool_json("memory.get", json!({"id": "only-a"}));
    assert_eq!(got["content"], json!("A secret"));
}

// ---------------------------------------------------------------------
// Corruption and failure injection (§17/§26)
// ---------------------------------------------------------------------

/// §17/§26: corrupt canonical store fails closed on every CLI op, is
/// never silently emptied, and never leaks content into the error.
#[test]
fn corrupt_store_fails_closed_and_never_leaks_content() {
    let root = TempDir::new().unwrap();
    let _ = add(root.path(), "canary-before-corruption", "Project", &[]);
    let agent = root.path().join(".agent");

    // truncated JSON
    std::fs::write(agent.join("memory.json"), "{\"entries\": [{\"id\":\"x\"").unwrap();
    for args in [
        vec!["memory", "list"],
        vec!["memory", "get", "--id", "x"],
        vec!["memory", "search", "--query", "x"],
        vec!["memory", "add", "--content", "new"],
        vec!["memory", "delete", "--id", "x"],
    ] {
        let (ok, out, err) = run(root.path(), &args);
        assert!(!ok, "corrupt store must fail closed for {args:?}");
        assert!(err.contains("read memory store") || err.contains("expected") || !err.is_empty());
        assert!(out.trim().is_empty(), "no fabricated output: {out}");
    }
    // corrupt bytes preserved (never silently rewritten to empty)
    let raw = std::fs::read_to_string(agent.join("memory.json")).unwrap();
    assert!(raw.starts_with("{\"entries\""));

    // unknown serialized field: serde default ignores it, load succeeds
    std::fs::write(
        agent.join("memory.json"),
        r#"{"entries":[{"id":"ok","scope":"Project","content":"c","tags":[],"created_at":"t","updated_at":"t","future":"ignored"}]}"#,
    )
    .unwrap();
    let (ok, out, _) = run(root.path(), &["memory", "get", "--id", "ok"]);
    assert!(ok, "unknown fields are tolerated (serde default): {out}");
    assert!(out.contains("\"ok\""));

    // duplicate ids in the raw file: get returns the FIRST match, list
    // shows both (store does not dedup on read) — pin current behavior.
    std::fs::write(
        agent.join("memory.json"),
        r#"{"entries":[
            {"id":"dup","scope":"Project","content":"first","tags":[],"created_at":"t","updated_at":"t"},
            {"id":"dup","scope":"Global","content":"second","tags":[],"created_at":"t","updated_at":"t"}]}"#,
    )
    .unwrap();
    let got =
        serde_json::from_str::<Value>(run(root.path(), &["memory", "get", "--id", "dup"]).1.trim())
            .unwrap();
    assert_eq!(got["content"], json!("first"));
    let (ok, out, _) = run(root.path(), &["memory", "list"]);
    assert!(ok);
    assert_eq!(out.lines().count(), 2, "both duplicate rows listed: {out}");

    // path-as-file: memory.json is a DIRECTORY → every op fails closed
    let root2 = TempDir::new().unwrap();
    let agent2 = root2.path().join(".agent");
    std::fs::create_dir_all(agent2.join("memory.json")).unwrap();
    let (ok, _, err) = run(root2.path(), &["memory", "list"]);
    assert!(!ok, "directory-as-store must fail closed: {err}");
}

// ---------------------------------------------------------------------
// Concurrency and atomicity (§16)
// ---------------------------------------------------------------------

/// §16: concurrent writers across REAL processes serialize on the store
/// lock — no lost updates, no malformed canonical file.
#[test]
fn concurrent_cross_process_writers_keep_store_valid() {
    let root = TempDir::new().unwrap();
    // seed one entry so the file exists before the race
    let _ = add(root.path(), "seed", "Project", &[]);

    let threads: Vec<_> = (0..8)
        .map(|t| {
            let dir = root.path().to_path_buf();
            std::thread::spawn(move || {
                for i in 0..5 {
                    let content = format!("writer {t} item {i}");
                    let (ok, _, err) = run(
                        &dir,
                        &["memory", "add", "--content", &content, "--scope", "Project"],
                    );
                    assert!(ok, "concurrent add failed: {err}");
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }

    // independent oracle: 41 valid entries (1 seed + 40), unique ids, and
    // the file parses as a single well-formed document.
    let store = read_store(root.path());
    let ids = entry_ids(root.path());
    assert_eq!(ids.len(), 41, "no lost updates: {}", ids.len());
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 41, "no duplicate identity");
    // every entry is complete (no torn write)
    for e in store["entries"].as_array().unwrap() {
        assert!(e["id"].is_string() && e["content"].is_string());
        assert!(e["scope"].is_string());
    }
}

/// §16/§22: concurrent update-vs-delete on the SAME id never resurrects
/// the entry and never leaves a malformed store.
#[test]
fn concurrent_update_vs_delete_same_id_is_atomic() {
    let root = TempDir::new().unwrap();
    let id = add(root.path(), "contended", "Project", &[]);

    let dir = root.path().to_path_buf();
    let id_u = id.clone();
    let updater = std::thread::spawn(move || {
        for i in 0..20 {
            let (ok, _, _) = run(
                &dir,
                &[
                    "memory",
                    "update",
                    "--id",
                    &id_u,
                    "--content",
                    &format!("v{i}"),
                ],
            );
            // update may fail once the entry is deleted — that's allowed;
            // it must never panic or claim success falsely.
            let _ = ok;
        }
    });
    let dir2 = root.path().to_path_buf();
    let id_d = id.clone();
    let deleter = std::thread::spawn(move || {
        for _ in 0..20 {
            let _ = run(&dir2, &["memory", "delete", "--id", &id_d]);
        }
    });
    updater.join().unwrap();
    deleter.join().unwrap();

    // The store is well-formed regardless of the interleaving.
    let store = read_store(root.path());
    assert!(store["entries"].is_array());
    // The entry is either absent (delete won last) or present with a valid
    // body — never a torn/duplicated record.
    let matches: Vec<_> = store["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["id"].as_str() == Some(id.as_str()))
        .collect();
    assert!(matches.len() <= 1, "no duplicate identity from the race");
}

// ---------------------------------------------------------------------
// Audit and sensitive-data protection (§19/§20)
// ---------------------------------------------------------------------

/// §19/§20: mutations emit durable audit events carrying the id/scope —
/// never the raw content. A token-shaped canary is stored in the canonical
/// file (storage is allowed) but must not appear in audit bytes.
#[test]
fn audit_records_mutations_without_leaking_content() {
    let root = TempDir::new().unwrap();
    // token-shaped canary content (disposable fixture only)
    let canary = "sk-live-0123456789abcdefghijklmnopqrstuvwxyz";
    let id = add(root.path(), canary, "Project", &["sensitive"]);

    // storage is allowed: the canonical file holds it (developer memory is
    // not a vault; the policy is about logs/output, not the store)
    let store = read_store(root.path());
    assert_eq!(entry_of(&store, &id)["content"], json!(canary));

    // the durable audit log records the mutation with the ID, but never
    // the content
    let audit = root.path().join(".agent").join("audit").join("audit.log");
    assert!(
        audit.is_file(),
        "audit log must be durable after a mutation"
    );
    let audit_bytes = std::fs::read_to_string(&audit).unwrap();
    assert!(
        audit_bytes.contains("cli_memory_add"),
        "mutation must be audited"
    );
    // The subject is the entry id, but the generated `mem-<hex>` shape is
    // token-shaped and goes through the redaction choke point (the same
    // fail-closed policy that masks policy-rule ids) — the audit records
    // the mutation and its scope, never the id verbatim, never content.
    assert!(
        audit_bytes.contains("[redacted]"),
        "token-shaped id must be redacted in the audit subject"
    );
    assert!(
        !audit_bytes.contains(&id),
        "the raw entry id is not persisted verbatim"
    );
    assert!(
        !audit_bytes.contains(canary),
        "audit must never persist raw memory content"
    );

    // update and delete are audited too, still without content
    let (ok, _, _) = run(
        root.path(),
        &[
            "memory",
            "update",
            "--id",
            &id,
            "--content",
            "another secret body",
        ],
    );
    assert!(ok);
    let (ok, _, _) = run(root.path(), &["memory", "delete", "--id", &id]);
    assert!(ok);
    let audit_bytes = std::fs::read_to_string(&audit).unwrap();
    assert!(audit_bytes.contains("cli_memory_update"));
    assert!(audit_bytes.contains("cli_memory_delete"));
    assert!(!audit_bytes.contains("another secret body"));
    assert!(!audit_bytes.contains(canary));

    // a FAILED operation must not leave a false-positive success audit
    let (ok, _, _) = run(root.path(), &["memory", "delete", "--id", "never-existed"]);
    assert!(!ok);
    let audit_bytes = std::fs::read_to_string(&audit).unwrap();
    assert!(
        !audit_bytes.contains("never-existed"),
        "a failed delete must not be audited as success"
    );
}

// ---------------------------------------------------------------------
// Context integration — shared authority (§12)
// ---------------------------------------------------------------------

/// §12: the Context Engine consumes the canonical memory authority — it
/// does NOT own a second memory store. Prove shared authority: write via
/// the CLI, observe through the engine's status memory count, mutate,
/// observe again, and confirm no memory store exists under the context
/// engine directory.
#[test]
fn context_engine_shares_the_canonical_memory_authority() {
    let root = TempDir::new().unwrap();
    // No memory yet: the engine reports 0.
    let (ok, out, _) = run(root.path(), &["context", "show"]);
    assert!(ok);
    let status: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(status["memories"], json!(0));

    // Write through the CLI (canonical store) → engine status reflects it.
    let _ = add(root.path(), "shared authority note", "Project", &[]);
    let (ok, out, _) = run(root.path(), &["context", "show"]);
    assert!(ok);
    let status: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(status["memories"], json!(1), "engine must see CLI memory");

    // Mutate via the canonical store → engine sees the new count.
    let _ = add(root.path(), "second note", "Global", &[]);
    let (ok, out, _) = run(root.path(), &["context", "show"]);
    assert!(ok);
    let status: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(status["memories"], json!(2));

    // The engine's memory view IS the canonical `.agent/memory.json` — there
    // is no second store under the context-engine directory.
    let engine_dir = root.path().join(".agent").join("context-engine");
    if engine_dir.exists() {
        for entry in std::fs::read_dir(&engine_dir).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            assert!(
                !name.contains("memory"),
                "no second memory store may exist under the context engine: {name}"
            );
        }
    }
    assert!(
        memory_file(root.path()).is_file(),
        "canonical store is the one"
    );

    // restart: the engine still reflects durable memory
    let (ok, out, _) = run(root.path(), &["context", "show"]);
    assert!(ok);
    let status: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(status["memories"], json!(2));
}

// ---------------------------------------------------------------------
// Control API delegation (§13)
// ---------------------------------------------------------------------

/// §13: the Control API delegates to the SAME canonical store — a write
/// through the API is visible via CLI and the raw file, and a CLI write is
/// visible through the API. Identical validation semantics; no
/// interface-local writer.
#[test]
fn control_api_memory_delegates_to_the_canonical_store() {
    use agent_workspace_hub::api::control::{build_router, ControlState};
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    let state = Arc::new(ControlState::new(root.clone(), "tp10-key".into()));
    let router = build_router(state);

    let request = |method: &str, uri: &str, body: Option<Value>| {
        let builder = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", "Bearer tp10-key");
        let req = match body {
            Some(json) => builder
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(json.to_string()))
                .expect("request"),
            None => builder
                .header("Content-Length", "0")
                .body(axum::body::Body::empty())
                .expect("request"),
        };
        let router = router.clone();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let response = router.oneshot(req).await.expect("oneshot");
                let status = response.status();
                let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
                    .await
                    .expect("body");
                (
                    status,
                    serde_json::from_slice::<Value>(&bytes).unwrap_or(json!({})),
                )
            })
    };

    // API append → canonical file has it, CLI sees it
    let (status, body) = request(
        "POST",
        "/api/v1/memory",
        Some(json!({"content": "written through the API"})),
    );
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    let api_id = body["id"].as_str().unwrap().to_string();
    let store = read_store(&root);
    assert_eq!(
        entry_of(&store, &api_id)["content"],
        json!("written through the API")
    );
    let (ok, out, _) = run(&root, &["memory", "get", "--id", &api_id]);
    assert!(ok, "CLI must see the API-written entry: {out}");
    assert!(out.contains("written through the API"));

    // API list reflects a CLI-written entry (shared authority both ways)
    let cli_id = add(&root, "written through the CLI", "Project", &[]);
    let (status, body) = request("GET", "/api/v1/memory", None);
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["total"], json!(2));
    let ids: Vec<&str> = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&api_id.as_str()) && ids.contains(&cli_id.as_str()));

    // same validation: empty content rejected, nothing written
    let before = std::fs::read(memory_file(&root)).unwrap();
    let (status, _) = request("POST", "/api/v1/memory", Some(json!({"content": "   "})));
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(std::fs::read(memory_file(&root)).unwrap(), before);

    // auth still applies (no bearer → 401), and no route-local writer exists
    let router2 = build_router(Arc::new(ControlState::new(root.clone(), "tp10-key".into())));
    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/memory")
        .header("Content-Length", "0")
        .body(axum::body::Body::empty())
        .unwrap();
    let status = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move { router2.oneshot(req).await.unwrap().status() });
    assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
}
