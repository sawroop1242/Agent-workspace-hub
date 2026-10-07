//! TP08 — Audit & Observability boundary verification
//! (`docs/testing-prompts/08-audit-and-observability.md`).
//!
//! Every test exercises the REAL canonical boundary: the compiled `awh`
//! binary in fresh processes (each invocation is a process restart of
//! the durable audit store), the durable `.agent/audit/audit.log` file
//! as the oracle (parsed independently here with serde_json + sha256,
//! never through the store's own read API), and the actual
//! consequential-operation producers (edit, terminal, memory, task,
//! collaboration) through their public CLIs.
//!
//! No second audit store, no test-only event model, no fake
//! persistence. The checksum envelope `{"checksum","event"}` is
//! verified with an independent SHA-256 over the exact serialized
//! payload embedded in each line.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const CANARY_TOKEN: &str = "Xk92JmQp7LwR4TnEvBnZ8yCd3Fq6Hs0";
const CANARY_ENV: &str = "sk-live-9zXk2mPvLwQr4TnE8uBc";
const TERMINAL_SENTINEL: &str = "TERMINAL_SENTINEL_9zXk2mPvLwQr4";

// ---------------------------------------------------------------------------
// Helpers — real binary, real durable store, independent oracle.
// ---------------------------------------------------------------------------

fn awh() -> &'static str {
    env!("CARGO_BIN_EXE_awh")
}

/// Runs one real CLI invocation in `dir`. Returns (ok, stdout, stderr).
fn run(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(awh())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn awh binary");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn init_ws(dir: &Path) {
    let (ok, _, err) = run(dir, &["init"]);
    assert!(ok, "awh init failed: {err}");
}

fn workspace_id(dir: &Path) -> String {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join(".agent/workspace.json")).expect("workspace manifest"),
    )
    .expect("valid manifest json");
    manifest["workspace_id"]
        .as_str()
        .expect("workspace id")
        .to_owned()
}

fn audit_path(dir: &Path) -> PathBuf {
    dir.join(".agent/audit/audit.log")
}

/// One parsed durable record plus the raw line it came from.
struct Record {
    raw: String,
    event: Value,
}

impl Record {
    fn checksum(&self) -> &str {
        self.event["checksum"].as_str().unwrap_or("")
    }
    /// The exact serialized payload that the writer checksummed — the
    /// substring after `"event":` up to the final `}` of the envelope.
    fn payload(&self) -> &str {
        let raw = &self.raw;
        let start = raw.find("\"event\":").expect("envelope carries an event") + "\"event\":".len();
        let end = raw.rfind('}').expect("envelope closes");
        &raw[start..end]
    }
    fn field(&self, name: &str) -> Option<&str> {
        self.event["event"][name].as_str()
    }
    /// Numeric fields (`sequence`, `schema_version`) are JSON numbers.
    fn number(&self, name: &str) -> Option<u64> {
        self.event["event"][name].as_u64()
    }
    fn sequence(&self) -> u64 {
        self.number("sequence").expect("sequence")
    }
}

/// Independent durable-store reader: parses every envelope line,
/// verifies each checksum with an independent SHA-256 implementation,
/// and returns records in file (write) order.
fn read_durable(dir: &Path) -> Vec<Record> {
    let text = match std::fs::read_to_string(audit_path(dir)) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("every durable line is a JSON envelope: {e}\nline: {line}"));
        let record = Record {
            raw: line.to_owned(),
            event,
        };
        let digest = Sha256::digest(record.payload().as_bytes());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            record.checksum(),
            "checksum must verify with an independent SHA-256"
        );
        assert_eq!(
            record.number("schema_version"),
            Some(1),
            "durable schema version"
        );
        out.push(record);
    }
    out
}

/// Edit events only (the producer under test in most flows).
fn edit_events(dir: &Path) -> Vec<Record> {
    read_durable(dir)
        .into_iter()
        .filter(|r| r.field("action") == Some("filesystem.replace"))
        .collect()
}

/// Lenient reader for intentionally damaged stores: parses only the
/// lines that are valid envelopes with verifying checksums (mirroring
/// the store's own exclusion rule for corrupt records), skipping the
/// rest. Used by corruption/torn-tail tests that *deliberately*
/// damage a line; the strict `read_durable` remains the oracle for
/// healthy stores.
fn read_valid_only(dir: &Path) -> Vec<Record> {
    let text = match std::fs::read_to_string(audit_path(dir)) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let record = Record {
            raw: line.to_owned(),
            event,
        };
        let digest = Sha256::digest(record.payload().as_bytes());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        if hex == record.checksum() {
            out.push(record);
        }
    }
    out
}

fn run_edit(dir: &Path, path: &str, old: &str, new: &str) -> String {
    let (ok, out, err) = run(dir, &["fs", "replace", "--json", path, old, new]);
    assert!(ok, "fs replace failed: {err}");
    out
}

fn make_file(dir: &Path, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).unwrap();
}

/// Seed a file and perform one real edit, returning the edit id the
/// caller received (the durable correlation key).
fn edit_once(dir: &Path, name: &str, old: &str, new: &str) -> String {
    let out = run_edit(dir, name, old, new);
    let v: Value = serde_json::from_str(out.trim()).expect("fs replace prints JSON");
    v["edit_id"].as_str().expect("edit id").to_owned()
}

// ---------------------------------------------------------------------------
// Workflow A + §6/§7/§11 — a successful consequential operation is
// audited durably, survives restart, keeps unique event identity and
// strictly monotonic order, and the diagnostic surface never stands in
// for the audit record.
// ---------------------------------------------------------------------------

#[test]
fn durable_events_survive_restart_with_independent_oracle() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_ws(root);
    make_file(root, "note.txt", "alpha\n");
    let ws = workspace_id(root);

    // Two separate processes = two store lifetimes.
    let edit_one = edit_once(root, "note.txt", "alpha", "beta");
    let edit_two = edit_once(root, "note.txt", "beta", "gamma");
    assert_ne!(edit_one, edit_two);

    let edits = edit_events(root);
    assert_eq!(edits.len(), 2, "exactly the two edit events");
    assert_eq!(
        edits[0].field("kind"),
        Some("allow"),
        "successful edit is an allow outcome"
    );
    assert_eq!(edits[0].field("edit_id"), Some(edit_one.as_str()));
    assert_eq!(edits[1].field("edit_id"), Some(edit_two.as_str()));

    // §23: workspace correlation is attached to every durable event.
    for record in &edits {
        assert_eq!(record.field("workspace_id"), Some(ws.as_str()));
    }

    // §6: unique event identity — an independent set is the oracle.
    let all = read_durable(root);
    let ids: HashSet<&str> = all.iter().map(|r| r.field("event_id").unwrap()).collect();
    assert_eq!(ids.len(), all.len(), "every event id is unique");
    for record in &all {
        let id = record.field("event_id").unwrap();
        assert!(id.starts_with("audit-"), "event ids use the audit prefix");
        assert!(id.len() >= "audit-".len() + 16, "not a bare timestamp");
    }

    // §7: strictly monotonic sequence in file order; timestamps are
    // metadata only.
    for window in all.windows(2) {
        assert!(
            window[1].sequence() > window[0].sequence(),
            "sequence must strictly increase in write order"
        );
    }

    // §10: the audit record lives only in the durable store — the
    // command's stdout is a diagnostic surface, never the audit event
    // (no envelope, no checksum, no sequence).
    let stdout = run_edit(root, "note.txt", "gamma", "delta");
    assert!(!stdout.contains("checksum"));
    assert!(!stdout.contains("sequence"));
    assert!(!stdout.contains("\"kind\""));

    // The file system agrees with the audit outcome (§25: operation +
    // state + audit must all match).
    assert_eq!(
        std::fs::read_to_string(root.join("note.txt")).unwrap(),
        "delta\n"
    );
}

// ---------------------------------------------------------------------------
// §16 — workspace isolation at the durable boundary: two real
// workspaces, identical relative paths, no cross-contamination.
// ---------------------------------------------------------------------------

#[test]
fn workspace_isolation_at_the_durable_boundary() {
    let dir_a = TempDir::new().unwrap();
    let dir_b = TempDir::new().unwrap();
    init_ws(dir_a.path());
    init_ws(dir_b.path());
    // Identical relative paths in both workspaces.
    for root in [dir_a.path(), dir_b.path()] {
        make_file(root, "shared_name.txt", "alpha\n");
    }
    let edit_a = edit_once(dir_a.path(), "shared_name.txt", "alpha", "beta");
    let edit_b = edit_once(dir_b.path(), "shared_name.txt", "alpha", "beta");

    let ws_a = workspace_id(dir_a.path());
    let ws_b = workspace_id(dir_b.path());
    assert_ne!(ws_a, ws_b, "workspaces have distinct identities");

    for (root, own_ws, own_edit, foreign_edit) in [
        (dir_a.path(), &ws_a, &edit_a, &edit_b),
        (dir_b.path(), &ws_b, &edit_b, &edit_a),
    ] {
        let all = read_durable(root);
        assert!(!all.is_empty(), "the workspace produced evidence");
        for record in &all {
            assert_eq!(
                record.field("workspace_id"),
                Some(own_ws.as_str()),
                "every event claims this workspace and only this workspace"
            );
        }
        let edits = edit_events(root);
        assert_eq!(edits.len(), 1, "only this workspace's edit is present");
        assert_eq!(edits[0].field("edit_id"), Some(own_edit.as_str()));
        // The foreign edit id never appears anywhere in this
        // workspace's durable bytes.
        let raw = std::fs::read_to_string(audit_path(root)).unwrap();
        assert!(
            !raw.contains(foreign_edit),
            "foreign edit id must not leak into another workspace"
        );
    }
}

// ---------------------------------------------------------------------------
// §20 + §26 regression — concurrent real processes appending to ONE
// disposable store: no duplicate sequence, no duplicate id, no torn
// line, and the store still initializes cleanly afterwards.
// ---------------------------------------------------------------------------

#[test]
fn concurrent_processes_append_cleanly() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_ws(root);
    for i in 1..=8 {
        make_file(root, &format!("f{i}.txt"), &format!("alpha{i}\n"));
    }

    for round in 0..2 {
        let mut round_handles = Vec::new();
        for i in 1..=4 {
            let root = root.to_path_buf();
            round_handles.push(std::thread::spawn(move || {
                // Round 0 turns alpha→beta; round 1 (joined after
                // round 0 completes) turns the same files beta→gamma.
                let (old, new) = if round == 0 {
                    (format!("alpha{i}"), format!("beta{i}"))
                } else {
                    (format!("beta{i}"), format!("gamma{i}"))
                };
                let (ok, _, err) = run(
                    &root,
                    &["fs", "replace", "--json", &format!("f{i}.txt"), &old, &new],
                );
                assert!(ok, "concurrent edit failed: {err}");
            }));
        }
        for handle in round_handles {
            handle.join().unwrap();
        }
    }

    let all = read_durable(root);
    assert!(
        all.len() >= 8,
        "every concurrent append landed: only {} events",
        all.len()
    );
    let mut seen_seq = HashSet::new();
    let mut seen_id = HashSet::new();
    let mut previous = 0u64;
    for record in &all {
        let seq = record.sequence();
        assert!(
            seen_seq.insert(seq),
            "duplicate sequence {seq} published by concurrent processes"
        );
        assert!(seen_id.insert(record.field("event_id").unwrap()));
        assert!(
            seq > previous,
            "file order must be strictly monotonic: {seq} after {previous}"
        );
        previous = seq;
    }
    assert_eq!(edit_events(root).len(), 8, "all eight edits are durable");

    // The bricked-vs-healthy discriminator: a fresh process must still
    // initialize the durable store (no `sequence regression`
    // corruption) and its event must persist.
    let edit = edit_once(root, "f1.txt", "gamma1", "delta1");
    assert_eq!(edit_events(root).len(), 9);
    assert!(
        read_durable(root)
            .iter()
            .any(|r| r.field("edit_id") == Some(edit.as_str())),
        "post-race append is durable"
    );
}

// ---------------------------------------------------------------------------
// Workflow E + §9 — a security investigation: correlated operations
// carrying agent/session identity reconstruct from the durable audit,
// through a real CLI readback surface.
// ---------------------------------------------------------------------------

#[test]
fn identity_correlated_operations_reconstruct_from_durable_audit() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_ws(root);

    // Real identity lifecycle, each step in its own process.
    let (ok, _, err) = run(
        root,
        &["agent", "create", "agent-a", "Alice", "--role", "worker"],
    );
    assert!(ok, "agent create failed: {err}");
    let (ok, _, err) = run(root, &["agent", "start", "agent-a"]);
    assert!(ok, "agent start failed: {err}");
    let (ok, _, err) = run(
        root,
        &[
            "task",
            "create",
            "--id",
            "task-1",
            "--title",
            "T",
            "--description",
            "d",
        ],
    );
    assert!(ok, "task create failed: {err}");
    let (ok, out, err) = run(root, &["agent", "session", "open", "agent-a"]);
    assert!(ok, "session open failed: {err}");
    let session_id = out
        .lines()
        .find_map(|l| l.strip_prefix("opened session "))
        .and_then(|l| l.split(' ').next())
        .expect("session id in output")
        .to_owned();

    // The consequential identity-carrying operation.
    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
            "--session",
            &session_id,
        ],
    );
    assert!(ok, "collab assign failed: {err}");

    let ws = workspace_id(root);
    let durable = read_durable(root);
    let collab: Vec<&Record> = durable
        .iter()
        .filter(|r| r.field("kind") == Some("collab"))
        .collect();
    assert_eq!(collab.len(), 1, "exactly one collab transition event");
    let event = collab[0];
    assert_eq!(event.field("action"), Some("assign"));
    assert_eq!(event.field("agent_id"), Some("agent-a"));
    assert_eq!(event.field("session_id"), Some(session_id.as_str()));
    assert_eq!(event.field("workspace_id"), Some(ws.as_str()));
    assert_eq!(event.field("reason"), Some("assigned"));
    // Possession of the session id is not authority: the identity
    // fields record WHO, and the detail records the decision — the
    // task resource id, never contents.
    assert!(event.field("detail").unwrap().contains("owner=agent-a"));

    // §4 Workflow E reconstruction: query by agent identity through a
    // real CLI audit-readback surface and rebuild the sequence.
    let (ok, out, err) = run(root, &["collaboration", "events"]);
    assert!(ok, "collaboration events failed: {err}");
    assert!(
        out.contains("assign") && out.contains("task:task-1"),
        "readback shows the assign transition"
    );

    // Ordering of the reconstruction: the collab event is the last
    // durable event (the most recent consequential operation).
    let last = durable.last().unwrap();
    assert_eq!(last.field("event_id"), event.field("event_id"));
}

// ---------------------------------------------------------------------------
// §14 — secret/sensitive-data protection: canaries driven through real
// consequential paths never reach the durable audit bytes, while the
// operations' real state independently confirms they succeeded.
// ---------------------------------------------------------------------------

#[test]
fn canaries_never_reach_durable_audit() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_ws(root);

    // (1) File-content secret: the edit's NEW text embeds a canary.
    make_file(root, "secret_target.txt", "plain start\n");
    let edit = edit_once(
        root,
        "secret_target.txt",
        "plain start",
        &format!("token {CANARY_TOKEN} embedded"),
    );

    // (2) Environment/credential-shaped value through the memory plane.
    let (ok, _, err) = run(
        root,
        &[
            "memory",
            "add",
            "--content",
            &format!("credential {CANARY_ENV} value"),
        ],
    );
    assert!(ok, "memory add failed: {err}");

    // (3) Sensitive command argument through the terminal plane.
    let (ok, _, err) = run(
        root,
        &["terminal", "run", "--", "printf", TERMINAL_SENTINEL],
    );
    assert!(ok, "terminal run failed: {err}");

    // Operations really happened — independent state oracle (§25).
    assert_eq!(
        std::fs::read_to_string(root.join("secret_target.txt")).unwrap(),
        format!("token {CANARY_TOKEN} embedded\n")
    );
    let (ok, out, _) = run(root, &["memory", "list"]);
    assert!(ok);
    assert!(out.contains("credential"), "memory entry exists");

    // The durable audit bytes never contain any canary.
    let raw = std::fs::read_to_string(audit_path(root)).unwrap();
    for canary in [CANARY_TOKEN, CANARY_ENV, TERMINAL_SENTINEL] {
        assert!(
            !raw.contains(canary),
            "canary leaked into the durable audit log"
        );
    }
    // And every event still correlates correctly.
    assert!(
        read_durable(root)
            .iter()
            .any(|r| r.field("edit_id") == Some(edit.as_str())),
        "the edit remains correlatable"
    );
}

// ---------------------------------------------------------------------------
// §19 + §28 — append failure semantics: when the durable store cannot
// be opened, the consequential operation's outcome stands, and the
// previously durable evidence survives untouched. A repaired store
// resumes appending without reusing identities.
// ---------------------------------------------------------------------------

#[test]
fn audit_storage_failure_is_best_effort_and_evidence_survives() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_ws(root);
    make_file(root, "note.txt", "one\n");
    let edit_one = edit_once(root, "note.txt", "one", "two");
    let before = std::fs::read(audit_path(root)).unwrap();

    // Storage failure: `.agent/audit` replaced by a regular file —
    // the durable store cannot initialize (ENOTDIR, cross-platform).
    let audit_dir = root.join(".agent/audit");
    let stashed: PathBuf = root.join(".agent/audit_stash");
    std::fs::rename(&audit_dir, &stashed).expect("stash audit dir");
    std::fs::write(&audit_dir, "not a directory").expect("block the audit path");

    let (ok, out, err) = run(
        root,
        &["fs", "replace", "--json", "note.txt", "two", "three"],
    );
    assert!(ok, "operation must stand when audit storage fails: {err}");
    let edit_two: Value = serde_json::from_str(out.trim()).unwrap();
    let edit_two = edit_two["edit_id"].as_str().unwrap().to_owned();
    assert_eq!(
        std::fs::read_to_string(root.join("note.txt")).unwrap(),
        "three\n",
        "the audited mutation really happened"
    );
    // No degraded process silently re-created the audit directory.
    assert!(audit_dir.is_file(), "the blocked path stays blocked");

    // Repair: restore the durable directory. Previously durable
    // evidence is byte-identical (§28: prior history intact).
    std::fs::remove_file(&audit_dir).unwrap();
    std::fs::rename(&stashed, &audit_dir).unwrap();
    assert_eq!(
        std::fs::read(audit_path(root)).unwrap(),
        before,
        "existing evidence is preserved across the outage"
    );
    // The event from the degraded process was buffered, not persisted
    // (documented tail-loss: the process died with its buffer).
    assert!(
        !read_durable(root)
            .iter()
            .any(|r| r.field("edit_id") == Some(edit_two.as_str())),
        "a degraded-mode event is not fabricated into history"
    );

    // A fresh process resumes durable auditing without reusing the
    // earlier edit's identity or regressing sequences.
    let edit_three = edit_once(root, "note.txt", "three", "four");
    let durable = read_durable(root);
    assert!(
        durable
            .iter()
            .any(|r| r.field("edit_id") == Some(edit_one.as_str())),
        "pre-outage evidence readable"
    );
    assert!(
        durable
            .iter()
            .any(|r| r.field("edit_id") == Some(edit_three.as_str())),
        "post-repair evidence appended"
    );
    let mut previous = 0u64;
    for record in &durable {
        assert!(record.sequence() > previous, "order intact after repair");
        previous = record.sequence();
    }
}

// ---------------------------------------------------------------------------
// §13 — middle corruption: fails closed, evidence preserved, later
// operations degrade truthfully instead of fabricating or overwriting.
// ---------------------------------------------------------------------------

#[test]
fn middle_corruption_fails_closed_and_preserves_evidence() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_ws(root);
    make_file(root, "note.txt", "one\n");
    edit_once(root, "note.txt", "one", "two");
    edit_once(root, "note.txt", "two", "three");

    // Corrupt a MIDDLE line (not the last): flip one hex digit of its
    // checksum so integrity verification fails deterministically.
    let path = audit_path(root);
    let text = std::fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    assert!(lines.len() >= 2, "at least two records exist");
    let first = lines[0].clone();
    let pos = first.find("\"checksum\":\"").unwrap() + "\"checksum\":\"".len();
    let flipped = if first.as_bytes()[pos] == b'0' {
        '1'
    } else {
        '0'
    };
    lines[0] = format!("{}{}{}", &first[..pos], flipped, &first[pos + 1..]);
    let corrupted: String = lines.join("\n") + "\n";
    std::fs::write(&path, &corrupted).unwrap();

    // The next process must NOT silently re-initialize over evidence:
    // the op proceeds (degraded, best-effort audit) but the damaged
    // history is left byte-identical and no new record is fabricated.
    let (ok, _, _) = run(
        root,
        &["fs", "replace", "--json", "note.txt", "three", "four"],
    );
    assert!(ok, "degraded op still serves the workspace");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        corrupted,
        "corrupt evidence is never overwritten or fabricated"
    );
    // The corrupt record is excluded from reads; the valid ones stay.
    assert_eq!(
        read_valid_only(root).len(),
        lines.len() - 1,
        "reads exclude the corrupt record, keep valid ones"
    );

    // Repair: restore the valid first line. New events resume with no
    // sequence reuse and no gap fabrication.
    lines[0] = first;
    std::fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();
    let pre_repair_seqs: Vec<u64> = read_valid_only(root).iter().map(|r| r.sequence()).collect();
    edit_once(root, "note.txt", "four", "five");
    let after = read_valid_only(root);
    let new_seqs: Vec<u64> = after.iter().map(|r| r.sequence()).collect();
    assert!(
        new_seqs.len() > pre_repair_seqs.len(),
        "appending resumed after repair"
    );
    for seq in pre_repair_seqs {
        assert!(
            new_seqs.contains(&seq),
            "prior identities are stable across the repair"
        );
    }
    let mut previous = 0u64;
    for record in &after {
        assert!(
            record.sequence() > previous,
            "strictly monotonic after repair"
        );
        previous = record.sequence();
    }
}

// ---------------------------------------------------------------------------
// §12 — torn tail (interrupted append): repaired at startup, prior
// history byte-identical, and the next append continues the sequence
// without reuse or fabrication.
// ---------------------------------------------------------------------------

#[test]
fn torn_tail_is_repaired_with_sequence_continuity() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_ws(root);
    make_file(root, "note.txt", "one\n");
    let edit_one = edit_once(root, "note.txt", "one", "two");
    let before = std::fs::read(audit_path(root)).unwrap();

    // Simulate an interrupted append: a partial envelope with no
    // trailing newline.
    let torn = "{\"checksum\":\"ab";
    let mut text = before.clone();
    text.extend_from_slice(torn.as_bytes());
    std::fs::write(audit_path(root), &text).unwrap();

    let max_seq = read_valid_only(root)
        .iter()
        .map(|r| r.sequence())
        .max()
        .unwrap();

    // The next process detects and truncates the torn tail, then
    // appends on a clean boundary with a fresh sequence.
    let edit_two = edit_once(root, "note.txt", "two", "three");
    let after = read_valid_only(root);
    let file = std::fs::read(audit_path(root)).unwrap();
    assert!(
        file.starts_with(&before),
        "torn tail removed; prior history byte-identical"
    );
    assert!(
        !String::from_utf8_lossy(&file).contains(torn),
        "the torn bytes are gone"
    );
    assert_eq!(
        file.len(),
        before.len() + after.last().map(|r| r.raw.len() + 1).unwrap_or(0),
        "exactly the prior bytes plus one clean appended line"
    );
    assert!(
        after
            .iter()
            .any(|r| r.field("edit_id") == Some(edit_one.as_str())),
        "prior evidence readable"
    );
    assert!(
        after
            .iter()
            .any(|r| r.field("edit_id") == Some(edit_two.as_str())),
        "new event landed after repair"
    );
    let new_event = after
        .iter()
        .find(|r| r.field("edit_id") == Some(edit_two.as_str()))
        .unwrap();
    assert!(
        new_event.sequence() == max_seq + 1,
        "sequence continues past the maximum without reuse: got {}, want {}",
        new_event.sequence(),
        max_seq + 1
    );
}

// ---------------------------------------------------------------------------
// §17 — query/read surface: `collaboration events` is the implemented
// bounded audit readback; filters compose without broadening scope and
// queries never mutate history.
// ---------------------------------------------------------------------------

#[test]
fn bounded_readback_filters_and_never_mutates() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_ws(root);
    let (ok, _, err) = run(
        root,
        &["agent", "create", "agent-a", "Alice", "--role", "worker"],
    );
    assert!(ok, "{err}");
    let (ok, _, err) = run(root, &["agent", "start", "agent-a"]);
    assert!(ok, "{err}");
    let (ok, _, err) = run(
        root,
        &[
            "task",
            "create",
            "--id",
            "task-1",
            "--title",
            "T",
            "--description",
            "d",
        ],
    );
    assert!(ok, "{err}");
    let (ok, out, err) = run(root, &["agent", "session", "open", "agent-a"]);
    assert!(ok, "{err}");
    let session_id = out
        .lines()
        .find_map(|l| l.strip_prefix("opened session "))
        .and_then(|l| l.split(' ').next())
        .expect("session id")
        .to_owned();
    let (ok, _, err) = run(
        root,
        &[
            "collaboration",
            "assign",
            "--kind",
            "task",
            "--id",
            "task-1",
            "--agent",
            "agent-a",
            "--session",
            &session_id,
        ],
    );
    assert!(ok, "{err}");
    let before = std::fs::read(audit_path(root)).unwrap();

    // Empty-result filter is explicit.
    let (ok, out, _) = run(root, &["collaboration", "events", "--action", "release"]);
    assert!(ok);
    assert!(
        out.contains("no collaboration events"),
        "explicit empty result: {out}"
    );

    // Exact-match filter narrows; an unknown action cannot broaden.
    let (ok, out, _) = run(root, &["collaboration", "events", "--action", "assign"]);
    assert!(ok);
    assert!(out.contains("assign"));
    assert!(out.lines().count() == 1, "only the matching action: {out}");

    // Malformed filter input is data, never code/path/shell.
    let (ok, _, _) = run(
        root,
        &["collaboration", "events", "--action", "../../etc/passwd"],
    );
    assert!(ok, "a path-shaped filter is just a non-matching value");

    // Reads never mutate history.
    assert_eq!(
        std::fs::read(audit_path(root)).unwrap(),
        before,
        "querying does not rewrite history"
    );
}
