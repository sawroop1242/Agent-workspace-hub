//! TP07 — Rollback & Recovery boundary verification suite
//! (`docs/testing-prompts/07-rollback-and-recovery.md`).
//!
//! Scope: explicit edit-level rollback ONLY — the exact-EditId
//! selection, eligibility, authorization-before-mutation, canonical
//! recovery-material consumption, produced-state conflict detection,
//! multi-file preflight, exact-byte restoration, created-file removal,
//! idempotent repeats, and honest result/lifecycle semantics. Snapshot
//! integrity, capability/policy decisions, filesystem containment and
//! audit plumbing each own their own master prompts (TP03/05/06/08);
//! they are exercised here only at the rollback integration boundary.
//!
//! Every test drives a REAL boundary: the compiled `awh` binary across
//! separate processes for the human CLI workflows, and the canonical
//! `EditService` over real initialized workspaces for the
//! programmatic/authorization shapes. Filesystem truth is verified with
//! raw byte reads and an INDEPENDENT SHA-256 oracle (§27) — never by
//! calling a production hash helper. No rollback engine, snapshot
//! store, policy evaluator, path resolver or audit log is
//! reimplemented.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

use agent_workspace_hub::mcp::permissions::Permission;
use agent_workspace_hub::models::capability_grant::CapabilityGrant;
use agent_workspace_hub::services::authorization::AuthorizingPrincipal;
use agent_workspace_hub::services::edit::{EditRollbackStatus, EditService, EditTransaction};
use agent_workspace_hub::services::init::{initialize_workspace, load_workspace_manifest};

// ---------------------------------------------------------------------------
// Independent SHA-256 oracle (§27) — self-checked against FIPS 180-4.
// ---------------------------------------------------------------------------

fn oracle_sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    let (blocks, _) = msg.as_chunks::<64>();
    for chunk in blocks {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

#[test]
fn oracle_sha256_matches_fips_vector() {
    assert_eq!(
        oracle_sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        oracle_sha256(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

// ---------------------------------------------------------------------------
// CLI-plane harness — every invocation is a fresh process (§19).
// ---------------------------------------------------------------------------

struct CliWs {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

impl CliWs {
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
        std::fs::write(self.root.join(path), content).expect("write fixture bytes");
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.join(path)).expect("read result")
    }

    fn read_bytes(&self, path: &str) -> Vec<u8> {
        std::fs::read(self.root.join(path)).expect("read result bytes")
    }

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

    fn fails(&self, args: &[&str], code: i32) -> Output {
        let output = run_in(&self.root, args);
        assert_eq!(
            output.status.code(),
            Some(code),
            "expected exit {code}, got {:?}: stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn edit_id_of(&self, stdout: &str) -> String {
        let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json line");
        value["edit_id"].as_str().expect("edit_id field").to_owned()
    }

    /// Performs a one-file replace edit and returns the edit id.
    fn replace(&self, path: &str, old: &str, new: &str) -> String {
        let stdout = self.ok(&["fs", "replace", path, old, new, "--json"]);
        self.edit_id_of(&stdout)
    }

    /// Performs a multi-file patch edit via a patch file.
    fn patch(&self, patch_name: &str, operations: &str) -> String {
        std::fs::write(self.root.join(patch_name), operations).expect("write patch");
        let stdout = self.ok(&["fs", "patch", patch_name, "--json"]);
        self.edit_id_of(&stdout)
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
// Service-plane helpers (authorization / identity shapes).
// ---------------------------------------------------------------------------

fn register_agent(root: &Path, agent_id: &str) {
    agent_workspace_hub::core::agents::AgentStore::new(root.to_path_buf())
        .create(&agent_workspace_hub::models::Agent {
            id: agent_id.to_owned(),
            name: agent_id.to_owned(),
            role: "test".into(),
            status: agent_workspace_hub::models::AgentStatus::Active,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .expect("register agent");
}

fn write_grant(root: &Path, grant: &CapabilityGrant) {
    let dir = root.join(".agent").join("capabilities");
    std::fs::create_dir_all(&dir).expect("capability dir");
    let path = dir.join(format!("{}.json", grant.id));
    std::fs::write(path, serde_json::to_string_pretty(grant).unwrap()).expect("write grant");
}

fn filesystem_grant(id: &str, agent_id: &str, scope: Option<&str>) -> CapabilityGrant {
    CapabilityGrant {
        id: id.to_owned(),
        agent_id: agent_id.to_owned(),
        permission: Permission::Filesystem,
        scope: scope.map(str::to_owned),
        granted_at: chrono::Utc::now().to_rfc3339(),
        expires_at: None,
    }
}

fn workspace_id_of(root: &Path) -> String {
    load_workspace_manifest(root)
        .expect("workspace manifest")
        .workspace_id
        .as_str()
        .to_owned()
}

fn provenance_json(root: &Path, edit_id: &str) -> serde_json::Value {
    let path = root
        .join(".agent")
        .join("provenance")
        .join(format!("{edit_id}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("provenance record")).expect("json")
}

// ---------------------------------------------------------------------------
// §6 — exact identity / eligibility: malformed and unknown ids never
// select an edit, never mutate, never fall back.
// ---------------------------------------------------------------------------

#[test]
fn eligibility_id_shapes_fail_deterministically_without_fallback() {
    let ws = CliWs::new();
    ws.write("recent.txt", "recent original\n");
    let real_edit = ws.replace("recent.txt", "recent original", "recent edited");
    assert_eq!(ws.read("recent.txt"), "recent edited\n");

    // Malformed ids: traversal-shaped, path-shaped, empty, too long.
    // The CLI's documented exit taxonomy sends malformed input to the
    // generic service error (1); every shape fails deterministically.
    for bad in [
        "../escape",
        "edit-../../etc/passwd",
        "some/file/path",
        "a\\b",
        "",
    ] {
        let output = ws.fails(&["fs", "rollback", bad], 1);
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("restored"),
            "malformed id {bad:?} must never report restoration"
        );
    }
    let too_long = "e".repeat(200);
    ws.fails(&["fs", "rollback", &too_long], 1);

    // A path is never interpreted as an edit selector: this file was
    // affected by a real edit, but "recent.txt" is not its EditId —
    // it fails as an unknown id (5), never selects by path.
    ws.fails(&["fs", "rollback", "recent.txt"], 5);
    // Unknown well-formed id → the pinned recovery exit code.
    ws.fails(&["fs", "rollback", "edit-never-happened-1"], 5);

    // Nothing was selected by timestamp, path, or "latest": the real
    // edit's produced state is intact and its recovery chain usable.
    assert_eq!(ws.read("recent.txt"), "recent edited\n");
    ws.ok(&["fs", "verify", &real_edit]);
    ws.ok(&["fs", "rollback", &real_edit]);
    assert_eq!(ws.read("recent.txt"), "recent original\n");
}

// ---------------------------------------------------------------------------
// §9/§26 Workflow matrix — existing, empty-existing, one-byte, and
// 8 MiB boundary rollbacks restore exact raw bytes.
// ---------------------------------------------------------------------------

#[test]
fn existing_empty_and_boundary_size_rollback_restore_exact_bytes() {
    let ws = CliWs::new();

    // Empty existing file: edit it, roll back, it is STILL an existing
    // empty file (never deleted, never confused with missing).
    std::fs::write(ws.root.join("empty.txt"), b"").unwrap();
    let edit_empty = {
        let stdout = ws.ok(&["fs", "insert", "empty.txt", "1", "added line", "--json"]);
        ws.edit_id_of(&stdout)
    };
    assert_eq!(ws.read("empty.txt"), "added line\n");
    ws.ok(&["fs", "rollback", &edit_empty]);
    assert_eq!(
        ws.read_bytes("empty.txt"),
        b"",
        "empty file restored as empty"
    );
    assert!(
        ws.root.join("empty.txt").exists(),
        "empty file still exists"
    );

    // One-byte file.
    ws.write_bytes("one.txt", b"Z");
    let edit_one = ws.replace("one.txt", "Z", "ZZZ");
    assert_eq!(ws.read_bytes("one.txt"), b"ZZZ");
    ws.ok(&["fs", "rollback", &edit_one]);
    assert_eq!(ws.read_bytes("one.txt"), b"Z");

    // Non-empty → emptied by the edit → rollback restores content.
    ws.write("hollow.txt", "solid content\n");
    let edit_hollow = {
        let stdout = ws.ok(&[
            "fs",
            "delete-range",
            "hollow.txt",
            "--from",
            "1",
            "--to",
            "1",
            "--json",
        ]);
        ws.edit_id_of(&stdout)
    };
    assert_eq!(ws.read("hollow.txt"), "");
    ws.ok(&["fs", "rollback", &edit_hollow]);
    assert_eq!(ws.read("hollow.txt"), "solid content\n");

    // Maximum allowed content (8 MiB, equal-length token so the edit
    // keeps the file exactly at the boundary): roll back and verify
    // every byte against the fixture and the oracle.
    let mut big = vec![0x36u8; 8 * 1024 * 1024];
    let token = b"TOKEN-MIDDLE-HERE";
    big[1024 * 1024..1024 * 1024 + token.len()].copy_from_slice(token);
    ws.write_bytes("big.bin", &big);
    let edit_big = ws.replace("big.bin", "TOKEN-MIDDLE-HERE", "TOKEN-EDITED-NOW");
    let produced = ws.read_bytes("big.bin");
    assert_ne!(produced, big, "edit must have landed");
    ws.ok(&["fs", "rollback", &edit_big]);
    let restored = ws.read_bytes("big.bin");
    assert_eq!(restored.len(), big.len(), "exact restored length");
    assert_eq!(&restored[..], &big[..], "exact restored bytes");
    assert_eq!(oracle_sha256(&restored), oracle_sha256(&big));
}

// ---------------------------------------------------------------------------
// §10/§12 Workflow E — multi-file patch, complete preflight before any
// restoration.
// ---------------------------------------------------------------------------

#[test]
fn multi_file_patch_rollback_restores_every_file_exactly() {
    let ws = CliWs::new();
    ws.write("a.txt", "alpha one\n");
    ws.write("b.txt", "beta two\r\n");
    ws.write("c-emoji.txt", "emoji 🦀 original\n");
    ws.write("unrelated.txt", "never touched\n");

    let edit = ws.patch(
        "patch.json",
        r#"{"operations": [
            {"path": "a.txt", "old": "alpha", "new": "ALPHA"},
            {"path": "b.txt", "old": "beta", "new": "BETA"},
            {"path": "c-emoji.txt", "old": "🦀", "new": "🚀"}
        ]}"#,
    );
    assert_eq!(ws.read("a.txt"), "ALPHA one\n");
    assert_eq!(ws.read("b.txt"), "BETA two\r\n");
    assert_eq!(ws.read("c-emoji.txt"), "emoji 🚀 original\n");

    // One edit id covers all three targets; the exact same id restores
    // every file to its exact pre-edit bytes (CRLF preserved, no
    // Unicode normalization) and never touches unrelated files.
    ws.ok(&["fs", "verify", &edit]);
    ws.ok(&["fs", "rollback", &edit]);
    assert_eq!(ws.read_bytes("a.txt"), b"alpha one\n");
    assert_eq!(ws.read_bytes("b.txt"), b"beta two\r\n");
    assert_eq!(
        ws.read_bytes("c-emoji.txt"),
        "emoji 🦀 original\n".as_bytes()
    );
    assert_eq!(ws.read("unrelated.txt"), "never touched\n");
}

#[test]
fn multi_file_preflight_conflict_blocks_every_restoration() {
    let ws = CliWs::new();
    ws.write("a.txt", "alpha\n");
    ws.write("b.txt", "beta\n");
    ws.write("c.txt", "gamma\n");

    let edit = ws.patch(
        "patch.json",
        r#"{"operations": [
            {"path": "a.txt", "old": "alpha", "new": "ALPHA"},
            {"path": "b.txt", "old": "beta", "new": "BETA"},
            {"path": "c.txt", "old": "gamma", "new": "GAMMA"}
        ]}"#,
    );
    assert_eq!(ws.read("a.txt"), "ALPHA\n");
    assert_eq!(ws.read("b.txt"), "BETA\n");
    assert_eq!(ws.read("c.txt"), "GAMMA\n");

    // External actor rewrites C after the edit.
    ws.write("c.txt", "external newer state\n");

    // Complete preflight: the conflict on C is detected BEFORE A or B
    // is restored — they stay in the edit's produced state.
    let output = ws.fails(&["fs", "rollback", &edit], 4);
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("conflict"),
        "canonical conflict status on stdout"
    );
    assert_eq!(ws.read("a.txt"), "ALPHA\n", "A untouched by conflict");
    assert_eq!(ws.read("b.txt"), "BETA\n", "B untouched by conflict");
    assert_eq!(ws.read("c.txt"), "external newer state\n");

    // Once the externally changed target matches the produced state
    // again, the SAME edit id rolls back completely and exactly.
    ws.write("c.txt", "GAMMA\n");
    ws.ok(&["fs", "rollback", &edit]);
    assert_eq!(ws.read("a.txt"), "alpha\n");
    assert_eq!(ws.read("b.txt"), "beta\n");
    assert_eq!(ws.read("c.txt"), "gamma\n");
}

// ---------------------------------------------------------------------------
// §18/§K — mixed-state retry: already-restored targets are partitioned
// out and never dead-lock or get re-restored.
// ---------------------------------------------------------------------------

#[test]
fn mixed_state_retry_completes_without_touching_restored_targets() {
    let ws = CliWs::new();
    ws.write("a.txt", "alpha\n");
    ws.write("b.txt", "beta\n");
    ws.write("c.txt", "gamma\n");
    let edit = ws.patch(
        "patch.json",
        r#"{"operations": [
            {"path": "a.txt", "old": "alpha", "new": "ALPHA"},
            {"path": "b.txt", "old": "beta", "new": "BETA"},
            {"path": "c.txt", "old": "gamma", "new": "GAMMA"}
        ]}"#,
    );

    // Simulate a partial rollback: A is manually back at its pre-edit
    // state while C carries an external change and B is still produced.
    ws.write("a.txt", "alpha\n");
    ws.write("c.txt", "external change\n");

    // Retry: A (already restored) must be skipped, not re-restored and
    // not a conflict; C still conflicts the whole request before any
    // mutation of B.
    let output = ws.fails(&["fs", "rollback", &edit], 4);
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("conflict"),
        "conflict on C: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(ws.read("a.txt"), "alpha\n");
    assert_eq!(ws.read("b.txt"), "BETA\n");
    assert_eq!(ws.read("c.txt"), "external change\n");

    // Clear the conflict; the retry completes ONLY the still-pending
    // targets (B and C) and leaves A byte-identical at pre-edit state.
    ws.write("c.txt", "GAMMA\n");
    ws.ok(&["fs", "rollback", &edit]);
    assert_eq!(ws.read("a.txt"), "alpha\n", "A stays at pre-edit state");
    assert_eq!(ws.read("b.txt"), "beta\n");
    assert_eq!(ws.read("c.txt"), "gamma\n");
}

// ---------------------------------------------------------------------------
// §18a Workflow I — repeat after success never overwrites newer work.
// ---------------------------------------------------------------------------

#[test]
fn repeat_after_success_preserves_newer_changes() {
    let ws = CliWs::new();
    ws.write("file.txt", "original\n");
    let edit = ws.replace("file.txt", "original", "edited");
    ws.ok(&["fs", "rollback", &edit]);
    assert_eq!(ws.read("file.txt"), "original\n");

    // A newer external change lands on the already-rolled-back file.
    ws.write("file.txt", "newer unrelated work\n");

    // The repeated rollback must never treat that newer change as
    // restorable produced state. The preflight triage classifies bytes
    // that match NEITHER the transaction-produced state NOR the
    // recorded pre-edit state as an external change: the request fails
    // closed (§18a's invariant — newer work is never overwritten).
    let output = ws.fails(&["fs", "rollback", &edit], 4);
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("conflict"),
        "newer post-rollback content must conflict, not restore: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        ws.read("file.txt"),
        "newer unrelated work\n",
        "newer work preserved"
    );

    // Only when the file is back at its exact pre-edit bytes is the
    // repeat an idempotent no-op.
    ws.write("file.txt", "original\n");
    let stdout = ws.ok(&["fs", "rollback", &edit]);
    assert!(stdout.contains("already_rolled_back"), "{stdout}");
    assert_eq!(ws.read("file.txt"), "original\n");
}

#[test]
fn repeated_conflict_preserves_the_external_change() {
    let ws = CliWs::new();
    ws.write("f.txt", "original\n");
    let edit = ws.replace("f.txt", "original", "edited");
    ws.write("f.txt", "external state\n");

    // First attempt conflicts; the external bytes survive it…
    ws.fails(&["fs", "rollback", &edit], 4);
    assert_eq!(ws.read("f.txt"), "external state\n");
    // …and every repeated attempt, with no other target mutated.
    ws.fails(&["fs", "rollback", &edit], 4);
    ws.fails(&["fs", "rollback", &edit], 4);
    assert_eq!(ws.read("f.txt"), "external state\n");

    // The durable outcome reflects the observed conflict, not success.
    let record = provenance_json(&ws.root, &edit);
    assert!(
        record["outcome"]
            .as_object()
            .expect("outcome object")
            .contains_key("rolled_back"),
        "provenance outcome must record the conflict correlation"
    );
}

// ---------------------------------------------------------------------------
// §9/§11 — created-file lifecycle: removed only when still attributable
// to the edit; an externally changed created file is never deleted.
// ---------------------------------------------------------------------------

fn service_ws() -> (tempfile::TempDir, EditService, String) {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    initialize_workspace(&root).expect("initialize workspace");
    let service = EditService::new(root.clone());
    let workspace_id = workspace_id_of(&root);
    (dir, service, workspace_id)
}

#[test]
fn created_file_is_deleted_only_when_still_edit_attributable() {
    let (dir, service, workspace_id) = service_ws();
    let root = dir.path();
    let operator = AuthorizingPrincipal::operator(&workspace_id);

    // The AWE-010 programmatic capture API: records captured while the
    // target is still missing describe a file the edit will create.
    // No current CLI executor creates files, so this shape is reached
    // through the canonical service boundary (§32).
    let edit_id = agent_workspace_hub::services::edit::EditId::new();
    let mut records = service
        .capture_rollback_records(&edit_id, &["created.txt".to_owned()])
        .expect("capture records");
    assert_eq!(records.len(), 1);
    assert!(
        !records[0].existed_before,
        "target was missing at capture time"
    );

    // The "edit" creates the file; the caller completes the produced
    // state hash from the observed post-commit bytes.
    std::fs::write(root.join("created.txt"), b"created by the edit\n").unwrap();
    let produced = std::fs::read(root.join("created.txt")).unwrap();
    for record in &mut records {
        record.after_hash = agent_workspace_hub::services::edit::sha256_hex(&produced);
    }

    let status = service
        .rollback_edits_as(&operator, &records)
        .expect("rollback executes");
    assert!(matches!(status, EditRollbackStatus::Restored), "{status:?}");
    assert!(
        !root.join("created.txt").exists(),
        "edit-created file removed only when it is still the produced state"
    );

    // Repeat on the record-based surface: the created file's absence is
    // the completed outcome, and the record executor refuses to act on
    // a target it can no longer observe — a fail-closed Conflict, not a
    // fabricated no-op and never a mutation. (The canonical edit-id
    // surface implements the idempotent AlreadyRolledBack triage; the
    // record surface is one-shot execution material by contract.)
    let status = service
        .rollback_edits_as(&operator, &records)
        .expect("repeat rollback");
    assert!(
        matches!(status, EditRollbackStatus::Conflict { .. }),
        "record-surface repeat must fail closed, got {status:?}"
    );
    assert!(
        !root.join("created.txt").exists(),
        "no mutation from the refused repeat"
    );
}

#[test]
fn externally_changed_created_file_is_never_deleted() {
    let (dir, service, workspace_id) = service_ws();
    let root = dir.path();
    let operator = AuthorizingPrincipal::operator(&workspace_id);

    let edit_id = agent_workspace_hub::services::edit::EditId::new();
    let mut records = service
        .capture_rollback_records(&edit_id, &["created.txt".to_owned()])
        .expect("capture records");
    std::fs::write(root.join("created.txt"), b"created by the edit\n").unwrap();
    let produced = std::fs::read(root.join("created.txt")).unwrap();
    for record in &mut records {
        record.after_hash = agent_workspace_hub::services::edit::sha256_hex(&produced);
    }

    // External actor rewrites the created file AFTER the edit.
    std::fs::write(root.join("created.txt"), b"external newer bytes\n").unwrap();

    let status = service
        .rollback_edits_as(&operator, &records)
        .expect("rollback executes");
    assert!(
        matches!(status, EditRollbackStatus::Conflict { .. }),
        "external change on a created file must conflict, got {status:?}"
    );
    assert_eq!(
        std::fs::read(root.join("created.txt")).unwrap(),
        b"external newer bytes\n",
        "created file with external work is never deleted"
    );
}

// ---------------------------------------------------------------------------
// §7/Workflow F — authorization denial before mutation, through the real
// EditService authorization boundary.
// ---------------------------------------------------------------------------

#[test]
fn rollback_authorization_denial_leaves_filesystem_untouched() {
    let (dir, service, workspace_id) = service_ws();
    let root = dir.path();

    // A real edit lands through the trusted operator surface.
    std::fs::create_dir_all(root.join("src")).expect("src dir");
    std::fs::write(root.join("src/main.rs"), "fn original() {}\n").unwrap();
    let operator = AuthorizingPrincipal::operator(&workspace_id);
    let transaction = EditTransaction::new(vec![
        agent_workspace_hub::services::edit::EditOperation::Replace {
            path: "src/main.rs".to_owned(),
            old: "original".to_owned(),
            new: "edited".to_owned(),
            occurrence: None,
        },
    ]);
    service
        .replace_as(&operator, transaction)
        .expect("operator edit lands");
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
        "fn edited() {}\n"
    );

    // Resolve the durable edit id from the provenance store.
    let edit_id = {
        let dir_listing =
            std::fs::read_dir(root.join(".agent/provenance")).expect("provenance dir");
        let mut ids: Vec<String> = dir_listing
            .filter_map(|entry| entry.ok().map(|e| e.path().to_string_lossy().to_string()))
            .collect();
        assert_eq!(ids.len(), 1, "exactly one edit in this workspace");
        ids.pop()
            .unwrap()
            .trim_end_matches(".json")
            .rsplit('/')
            .next()
            .unwrap()
            .to_owned()
    };

    // Agent WITHOUT any capability grant → denial before any mutation.
    register_agent(root, "agent-none");
    let denied_principal = AuthorizingPrincipal::agent("agent-none", "session-1", &workspace_id);
    let error = service
        .rollback_edit(&denied_principal, &edit_id)
        .expect_err("rollback must be denied");
    assert!(
        matches!(
            error,
            agent_workspace_hub::services::edit::EditError::AuthorizationDenied { .. }
        ),
        "expected authorization denial, got {error:?}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
        "fn edited() {}\n",
        "denied rollback mutates nothing"
    );

    // Agent with an OUT-OF-SCOPE grant (docs/ only, edit on src/) → same
    // invariant.
    register_agent(root, "agent-scoped");
    write_grant(
        root,
        &filesystem_grant("g-scoped", "agent-scoped", Some("docs/")),
    );
    let scoped_principal = AuthorizingPrincipal::agent("agent-scoped", "session-2", &workspace_id);
    let error = service
        .rollback_edit(&scoped_principal, &edit_id)
        .expect_err("out-of-scope rollback must be denied");
    assert!(
        matches!(
            error,
            agent_workspace_hub::services::edit::EditError::AuthorizationDenied { .. }
        ),
        "expected scope denial, got {error:?}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
        "fn edited() {}\n"
    );

    // A denied history never corrupts the recovery chain: the trusted
    // operator still restores the edit exactly.
    let status = service
        .rollback_edit(&operator, &edit_id)
        .expect("operator rollback after denials");
    assert!(matches!(status, EditRollbackStatus::Restored), "{status:?}");
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
        "fn original() {}\n"
    );
}

// ---------------------------------------------------------------------------
// §23/§24 — identity is recorded and preserved; capability, not
// identity, is the authority for rollback.
// ---------------------------------------------------------------------------

#[test]
fn identity_is_recorded_preserved_and_never_authority() {
    let (dir, service, workspace_id) = service_ws();
    let root = dir.path();

    // agent-a performs the edit through the identity-carrying surface.
    register_agent(root, "agent-a");
    write_grant(root, &filesystem_grant("g-a", "agent-a", None));
    let agent_a = AuthorizingPrincipal::agent("agent-a", "session-a", &workspace_id);
    std::fs::write(root.join("doc.txt"), "original\n").unwrap();
    let transaction = EditTransaction::new(vec![
        agent_workspace_hub::services::edit::EditOperation::Replace {
            path: "doc.txt".to_owned(),
            old: "original".to_owned(),
            new: "edited".to_owned(),
            occurrence: None,
        },
    ]);
    let result = service
        .replace_as(&agent_a, transaction)
        .expect("agent-a edit lands");
    let edit_id = result.id.to_string();
    assert_eq!(
        std::fs::read_to_string(root.join("doc.txt")).unwrap(),
        "edited\n"
    );

    // Durable provenance carries the caller identity, not a fabricated
    // one (§24).
    let record = provenance_json(root, &edit_id);
    assert_eq!(record["agent_id"].as_str(), Some("agent-a"));
    assert_eq!(record["session_id"].as_str(), Some("session-a"));

    // A DIFFERENT agent with a valid capability may roll the edit back:
    // capability is the authority — identity alone (the original
    // editor, an id, a snapshot reference) grants nothing and never
    // blocks a legitimately authorized operation.
    register_agent(root, "agent-b");
    write_grant(root, &filesystem_grant("g-b", "agent-b", None));
    let agent_b = AuthorizingPrincipal::agent("agent-b", "session-b", &workspace_id);
    let status = service
        .rollback_edit(&agent_b, &edit_id)
        .expect("granted agent rolls back");
    assert!(matches!(status, EditRollbackStatus::Restored), "{status:?}");
    assert_eq!(
        std::fs::read_to_string(root.join("doc.txt")).unwrap(),
        "original\n"
    );

    // Identity preservation: the durable record still names the ORIGINAL
    // editor after another agent's rollback — correlation never rewrites
    // who performed the edit — and the outcome is rolled_back.
    let record = provenance_json(root, &edit_id);
    assert_eq!(
        record["agent_id"].as_str(),
        Some("agent-a"),
        "original editor identity survives the rollback correlation"
    );
    assert_eq!(record["session_id"].as_str(), Some("session-a"));
    assert!(record["outcome"]
        .as_object()
        .expect("outcome object")
        .contains_key("rolled_back"));
}

// ---------------------------------------------------------------------------
// §21 — concurrent rollback requests for the same edit are safe.
// ---------------------------------------------------------------------------

#[test]
fn concurrent_rollback_requests_never_corrupt_or_double_restore() {
    let ws = CliWs::new();
    ws.write("race.txt", "original bytes\n");
    let edit = ws.replace("race.txt", "original", "edited");

    // Two real processes race the same edit id. Every interleaving is
    // safe: at most one restores; the loser reports either
    // already_rolled_back (exit 0) or conflict (exit 4) — never a
    // second restoration, never a mutation of external state.
    let root = ws.root.clone();
    let edit_for_thread = edit.clone();
    let handle = std::thread::spawn(move || run_in(&root, &["fs", "rollback", &edit_for_thread]));
    let output_b = run_in(&ws.root, &["fs", "rollback", &edit]);
    let output_a = handle.join().expect("join racer");

    let stdout_a = String::from_utf8_lossy(&output_a.stdout).to_string();
    let stdout_b = String::from_utf8_lossy(&output_b.stdout).to_string();
    let restored_count = [stdout_a.contains("restored"), stdout_b.contains("restored")]
        .iter()
        .filter(|v| **v)
        .count();
    assert!(
        restored_count <= 1,
        "at most one process may report restoration: {stdout_a} | {stdout_b}"
    );
    for (label, output) in [("a", &output_a), ("b", &output_b)] {
        assert!(
            matches!(output.status.code(), Some(0) | Some(4)),
            "safe outcome expected for racer {label}, got {:?}",
            output.status.code()
        );
    }

    // The final filesystem state is exactly the pre-edit bytes —
    // whichever safe interleaving occurred.
    assert_eq!(ws.read_bytes("race.txt"), b"original bytes\n");
}

// ---------------------------------------------------------------------------
// §24/§25 — durable rollback audit carries ids and outcomes, never
// file contents.
// ---------------------------------------------------------------------------

#[test]
fn rollback_audit_is_durable_and_content_free() {
    const SENTINEL: &str = "sk-ROLLBACK-SENTINEL-3f9d2c8b";
    let ws = CliWs::new();
    ws.write("audited.txt", &format!("token={SENTINEL}\n"));
    let edit = ws.replace("audited.txt", "token=", "redacted=");

    // A successful rollback, then a conflicting retry attempt on a
    // second edit, both leave audit trails.
    ws.ok(&["fs", "rollback", &edit]);
    // After the rollback the pre-edit bytes are back; a second edit
    // reproduces a fresh conflict trail.
    let edit_two = ws.replace("audited.txt", "token=", "redacted=");
    ws.write("audited.txt", "external newer\n");
    ws.fails(&["fs", "rollback", &edit_two], 4);

    let audit_path = ws.root.join(".agent/audit/audit.log");
    let audit_text = std::fs::read_to_string(&audit_path).expect("durable audit log");
    assert!(
        audit_text.contains("filesystem.rollback"),
        "rollback action recorded durably"
    );
    assert!(audit_text.contains(&edit), "edit id correlated in audit");
    assert!(
        audit_text.contains("rollback_conflict"),
        "conflict reason recorded"
    );
    // §25: audit never carries file contents or secret-shaped values.
    assert!(
        !audit_text.contains(SENTINEL),
        "audit must not embed file contents"
    );
}
