//! TP06 — Snapshots & Provenance boundary verification suite
//! (`docs/testing-prompts/06-snapshots-and-provenance.md`).
//!
//! Scope: the durable snapshot/provenance recovery boundary ONLY —
//! not edit execution, rollback authorization, filesystem containment,
//! Git, or audit (owned by TP03/TP05/TP04/TP08 and the per-layer
//! suites). Every test drives a REAL boundary: the compiled `awh`
//! binary across separate processes, or the canonical
//! `SnapshotStore` over real temporary workspaces with durable state
//! inspected directly on disk. No store is mocked, no snapshot or
//! integrity implementation is reimplemented — the only in-file
//! reimplementation is the INDEPENDENT SHA-256 oracle that §25
//! explicitly requires (self-checked against the FIPS-180 "abc" test
//! vector so the oracle itself is proven correct before it is trusted).
//!
//! The store's cross-process contract is pinned at the CLI plane where
//! every invocation is a fresh process reading only durable state.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

use agent_workspace_hub::services::snapshot::{
    FileSnapshot, ProvenanceOutcome, ProvenanceRecord, SnapshotError, SnapshotFile, SnapshotId,
    SnapshotStore, MAX_SNAPSHOT_CONTENT_BYTES,
};

// ---------------------------------------------------------------------------
// Independent SHA-256 oracle (§25)
// ---------------------------------------------------------------------------

/// A compact, independent SHA-256 implementation used ONLY as an oracle:
/// expected integrity values are computed here, never by calling the
/// production `sha256_hex` helper. Self-validated against the first
/// FIPS 180-4 example vector before any assertion trusts it.
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
    // First FIPS 180-4 example: the oracle must be proven correct
    // before any other test trusts it.
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
// CLI-plane harness: every invocation is a fresh process reading only
// durable state, so sequencing invocations IS restart evidence (§16).
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

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.join(path)).expect("read result")
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
}

fn run_in(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_awh"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("run awh binary")
}

/// Walks a directory tree and returns every file's path.
fn tree_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

fn provenance_path(root: &Path, edit_id: &str) -> PathBuf {
    root.join(".agent")
        .join("provenance")
        .join(format!("{edit_id}.json"))
}

// ---------------------------------------------------------------------------
// Store-level helpers
// ---------------------------------------------------------------------------

fn store_at(dir: &Path) -> SnapshotStore {
    SnapshotStore::new(dir.to_path_buf())
}

fn record_for(edit_id: &str, manifest: &FileSnapshot) -> ProvenanceRecord {
    ProvenanceRecord {
        edit_id: edit_id.to_string(),
        snapshot_id: manifest.id.clone(),
        operations: vec!["Replace".into()],
        paths: manifest.entries.iter().map(|e| e.path.clone()).collect(),
        agent_id: None,
        session_id: None,
        workspace_id: None,
        hash_edges: vec![],
        outcome: ProvenanceOutcome::Committed,
        created_at: "2026-10-04T00:00:00Z".to_string(),
    }
}

// ---------------------------------------------------------------------------
// §26 Workflow C — two edits, per-edit recovery (CLI, real processes)
// ---------------------------------------------------------------------------

#[test]
fn two_edits_recover_independently_across_processes() {
    let ws = CliWs::new();
    ws.write("x.txt", "X-original\n");
    ws.write("y.txt", "Y-original\n");

    let stdout = ws.ok(&["fs", "replace", "x.txt", "X-original", "X-edited", "--json"]);
    let edit_x = ws.edit_id_of(&stdout);
    let stdout = ws.ok(&["fs", "replace", "y.txt", "Y-original", "Y-edited", "--json"]);
    let edit_y = ws.edit_id_of(&stdout);
    assert_ne!(edit_x, edit_y, "distinct edits must have distinct ids");
    assert_eq!(ws.read("x.txt"), "X-edited\n");
    assert_eq!(ws.read("y.txt"), "Y-edited\n");

    // Each edit's recovery material exists and is bound to exactly it.
    ws.ok(&["fs", "verify", &edit_x]);
    ws.ok(&["fs", "verify", &edit_y]);

    // Rolling back X restores X only; Y keeps its edited bytes and Y's
    // own recovery chain stays intact and usable afterwards.
    ws.ok(&["fs", "rollback", &edit_x]);
    assert_eq!(ws.read("x.txt"), "X-original\n");
    assert_eq!(ws.read("y.txt"), "Y-edited\n");
    ws.ok(&["fs", "verify", &edit_y]);
    ws.ok(&["fs", "rollback", &edit_y]);
    assert_eq!(ws.read("y.txt"), "Y-original\n");
    assert_eq!(ws.read("x.txt"), "X-original\n");
}

// ---------------------------------------------------------------------------
// §11 — valid-JSON provenance swap between two REAL edits fails closed
// ---------------------------------------------------------------------------

#[test]
fn provenance_snapshot_swap_between_real_edits_fails_closed() {
    let ws = CliWs::new();
    ws.write("x.txt", "X-original\n");
    ws.write("y.txt", "Y-original\n");
    let stdout = ws.ok(&["fs", "replace", "x.txt", "X-original", "X-edited", "--json"]);
    let edit_x = ws.edit_id_of(&stdout);
    let stdout = ws.ok(&["fs", "replace", "y.txt", "Y-original", "Y-edited", "--json"]);
    let edit_y = ws.edit_id_of(&stdout);

    // Both durable records exist and point at distinct snapshots.
    let px = std::fs::read_to_string(provenance_path(&ws.root, &edit_x)).unwrap();
    let py = std::fs::read_to_string(provenance_path(&ws.root, &edit_y)).unwrap();
    let mut jx: serde_json::Value = serde_json::from_str(&px).unwrap();
    let mut jy: serde_json::Value = serde_json::from_str(&py).unwrap();
    let snap_x = jx["snapshot_id"].as_str().expect("snapshot_id").to_owned();
    let snap_y = jy["snapshot_id"].as_str().expect("snapshot_id").to_owned();
    assert_ne!(snap_x, snap_y, "edits must not share a snapshot");

    // Tamper: swap the edit→snapshot bindings. Both records stay valid
    // JSON referencing real, hash-valid snapshots — "plausible bytes".
    std::mem::swap(&mut jx["snapshot_id"], &mut jy["snapshot_id"]);
    std::fs::write(
        provenance_path(&ws.root, &edit_x),
        serde_json::to_vec_pretty(&jx).unwrap(),
    )
    .unwrap();
    std::fs::write(
        provenance_path(&ws.root, &edit_y),
        serde_json::to_vec_pretty(&jy).unwrap(),
    )
    .unwrap();

    // A valid-looking snapshot must not become usable merely because
    // its bytes verify: both recovery reads fail closed (exit 5) and
    // neither edit's recovery material crosses to the other file.
    ws.fails(&["fs", "verify", &edit_x], 5);
    ws.fails(&["fs", "verify", &edit_y], 5);
    ws.fails(&["fs", "rollback", &edit_x], 5);
    ws.fails(&["fs", "rollback", &edit_y], 5);
    assert_eq!(ws.read("x.txt"), "X-edited\n");
    assert_eq!(ws.read("y.txt"), "Y-edited\n");
}

// ---------------------------------------------------------------------------
// §12 — identical edit ids + identical paths in two workspaces never collide
// ---------------------------------------------------------------------------

#[test]
fn identical_edit_ids_and_paths_never_cross_workspaces() {
    let dir_a = tempdir().unwrap();
    let dir_b = tempdir().unwrap();
    let store_a = store_at(dir_a.path());
    let store_b = store_at(dir_b.path());

    // The adversarial shape: SAME edit id, SAME path, DIFFERENT bytes,
    // in two independent workspaces.
    let same_edit = "shared-edit-001";
    let same_path = "src/shared.txt";
    let manifest_a = store_a
        .create(
            same_edit,
            &[(same_path.into(), b"workspace A bytes\n".to_vec())],
            None,
        )
        .unwrap();
    let manifest_b = store_b
        .create(
            same_edit,
            &[(same_path.into(), b"workspace B DIFFERENT\n".to_vec())],
            None,
        )
        .unwrap();
    store_a
        .record_provenance(&record_for(same_edit, &manifest_a))
        .unwrap();
    store_b
        .record_provenance(&record_for(same_edit, &manifest_b))
        .unwrap();

    // Each workspace resolves its own material for the identical id.
    let view_a = store_a.recovery_view(same_edit).unwrap();
    let view_b = store_b.recovery_view(same_edit).unwrap();
    assert_eq!(
        view_a,
        vec![(same_path.to_string(), Some(b"workspace A bytes\n".to_vec()))]
    );
    assert_eq!(
        view_b,
        vec![(
            same_path.to_string(),
            Some(b"workspace B DIFFERENT\n".to_vec())
        )]
    );

    // A's snapshot id cannot be loaded or recovered through B's store
    // and vice versa: all lookup is workspace-root relative.
    assert!(matches!(
        store_b.load(&manifest_a.id),
        Err(SnapshotError::NotFound(_))
    ));
    assert!(matches!(
        store_a.load(&manifest_b.id),
        Err(SnapshotError::NotFound(_))
    ));

    // Byte-equality oracles: neither view ever contains the other
    // workspace's bytes.
    for (_, bytes) in view_a {
        assert_ne!(bytes, Some(b"workspace B DIFFERENT\n".to_vec()));
    }
    for (_, bytes) in view_b {
        assert_ne!(bytes, Some(b"workspace A bytes\n".to_vec()));
    }
}

// ---------------------------------------------------------------------------
// §17/§18 — crash residue: orphan blobs and stolen manifests are never
// valid recovery material
// ---------------------------------------------------------------------------

#[test]
fn crash_residue_orphan_blobs_and_stolen_manifests_are_never_valid() {
    let dir = tempdir().unwrap();
    let store = store_at(dir.path());
    let manifest = store
        .create(
            "edit-real",
            &[("f.txt".into(), b"real bytes\n".to_vec())],
            None,
        )
        .unwrap();
    store
        .record_provenance(&record_for("edit-real", &manifest))
        .unwrap();

    // Crash shape 1: content blobs committed, manifest never did. Copy
    // the real snapshot's blob dir under an id that has no manifest.
    let contents = dir.path().join(".agent/snapshot-contents");
    let orphan_id = SnapshotId("snap-orphan-no-manifest".into());
    let orphan_dir = contents.join(orphan_id.as_str());
    std::fs::create_dir_all(&orphan_dir).unwrap();
    std::fs::copy(
        contents.join(manifest.id.as_str()).join("entry-000000.bin"),
        orphan_dir.join("entry-000000.bin"),
    )
    .unwrap();

    // The orphan is invisible as recovery material: listing shows only
    // the real snapshot, loading it is not-found, and no edit's
    // recovery view can reach it (blobs require a committed manifest).
    let listed = store.list().unwrap();
    assert_eq!(
        listed,
        vec![manifest.id.clone()],
        "orphan must not be listed"
    );
    assert!(matches!(
        store.load(&orphan_id),
        Err(SnapshotError::NotFound(_))
    ));

    // Crash shape 2: an orphan manifest copied verbatim under another
    // id — internally consistent JSON, but the declared id disagrees
    // with the requested one, so it can never masquerade as the
    // target's recovery material.
    let stolen_id = SnapshotId("snap-stolen-copy".into());
    std::fs::copy(
        dir.path()
            .join(".agent/snapshots")
            .join(format!("{}.json", manifest.id.as_str())),
        dir.path()
            .join(".agent/snapshots")
            .join(format!("{}.json", stolen_id.as_str())),
    )
    .unwrap();
    assert!(matches!(
        store.load(&stolen_id),
        Err(SnapshotError::Corruption(_))
    ));

    // Crash shape 3: provenance committed for an edit whose snapshot
    // never published a manifest (blob dir exists, manifest does not).
    let missing = SnapshotId("snap-never-published".into());
    std::fs::create_dir_all(contents.join(missing.as_str())).unwrap();
    let mut forged = record_for("edit-unpublished", &manifest);
    forged.snapshot_id = missing.clone();
    store.record_provenance(&forged).unwrap();
    assert!(matches!(
        store.recovery_view("edit-unpublished"),
        Err(SnapshotError::NotFound(_) | SnapshotError::Corruption(_))
    ));

    // The real edit's material is untouched by every residue shape.
    let view = store.recovery_view("edit-real").unwrap();
    assert_eq!(
        view,
        vec![("f.txt".to_string(), Some(b"real bytes\n".to_vec()))]
    );
}

// ---------------------------------------------------------------------------
// §23 — durable metadata never embeds file content or secrets
// ---------------------------------------------------------------------------

#[test]
fn durable_metadata_never_embeds_file_content_or_secrets() {
    const SENTINEL: &str = "sk-TEST-sentinel-DO-NOT-LEAK-0123456789";
    let ws = CliWs::new();
    ws.write("creds.txt", &format!("token={SENTINEL}\n"));
    let pre_edit = format!("token={SENTINEL}\n");

    let stdout = ws.ok(&[
        "fs",
        "replace",
        "creds.txt",
        "token=",
        "redacted=",
        "--json",
    ]);
    let edit_id = ws.edit_id_of(&stdout);

    // Metadata planes (manifests + provenance records) must carry only
    // ids, hashes, sizes and statuses — never raw content. The blob is
    // the recovery payload and is the ONLY durable file that may hold
    // the sentinel bytes.
    let mut json_files_with_sentinel = Vec::new();
    let mut blobs_with_sentinel = 0;
    for path in tree_files(&ws.root.join(".agent")) {
        let bytes = std::fs::read(&path).unwrap();
        if !bytes
            .windows(SENTINEL.len())
            .any(|w| w == SENTINEL.as_bytes())
        {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            json_files_with_sentinel.push(path);
        } else if path.extension().and_then(|e| e.to_str()) == Some("bin") {
            blobs_with_sentinel += 1;
        }
    }
    assert!(
        json_files_with_sentinel.is_empty(),
        "metadata must not embed content: {json_files_with_sentinel:?}"
    );
    assert_eq!(
        blobs_with_sentinel, 1,
        "exactly the one pre-edit blob carries the payload"
    );

    // The manifest's recorded hash is the INDEPENDENT sha256 of the
    // exact pre-edit bytes (§25 oracle), not a self-reported value.
    let snapshots_dir = ws.root.join(".agent/snapshots");
    let manifest_file = tree_files(&snapshots_dir)
        .into_iter()
        .find(|p| {
            let text = std::fs::read_to_string(p).unwrap();
            text.contains(&edit_id)
        })
        .expect("manifest for the edit");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_file).unwrap()).unwrap();
    let entry = &manifest["entries"][0];
    assert_eq!(
        entry["hash_before"].as_str().unwrap(),
        oracle_sha256(pre_edit.as_bytes()),
        "manifest hash must match the independent oracle"
    );
    assert_eq!(entry["size_before"].as_u64(), Some(pre_edit.len() as u64));

    // Human-facing recovery reads surface ids/statuses only.
    let history = ws.ok(&["fs", "history"]);
    assert!(
        !history.contains(SENTINEL),
        "history must not leak content: {history}"
    );
    let verify = ws.ok(&["fs", "verify", &edit_id]);
    assert!(
        !verify.contains(SENTINEL),
        "verify must not leak content: {verify}"
    );
}

// ---------------------------------------------------------------------------
// §16 — cross-process recovery chain with repeated reads after restart
// ---------------------------------------------------------------------------

#[test]
fn cross_process_recovery_chain_with_repeated_reads() {
    let ws = CliWs::new();
    ws.write("chain.txt", "original-bytes\n");

    // Every invocation below is a separate process: the chain proves
    // durable ids, manifests, blobs and provenance all survive real
    // process boundaries and tolerate repeated recovery reads.
    let stdout = ws.ok(&[
        "fs",
        "replace",
        "chain.txt",
        "original",
        "modified",
        "--json",
    ]);
    let edit_id = ws.edit_id_of(&stdout);
    assert_eq!(ws.read("chain.txt"), "modified-bytes\n");

    ws.ok(&["fs", "verify", &edit_id]); // fresh process read 1
    ws.ok(&["fs", "verify", &edit_id]); // repeated read, same answer
    ws.ok(&["fs", "rollback", &edit_id]); // fresh process mutation
    assert_eq!(ws.read("chain.txt"), "original-bytes\n");

    // Recovery material outlives the rollback it served: repeated reads
    // after restart still resolve, and a second rollback reports the
    // already-rolled-back state instead of erroring or re-restoring.
    ws.ok(&["fs", "verify", &edit_id]);
    let again = ws.ok(&["fs", "rollback", &edit_id]);
    assert!(again.contains("already_rolled_back"), "{again}");
    assert_eq!(ws.read("chain.txt"), "original-bytes\n");
    ws.ok(&["fs", "verify", &edit_id]);
}

// ---------------------------------------------------------------------------
// §13/§14 — provenance correlation fields + outcome lifecycle
// ---------------------------------------------------------------------------

#[test]
fn provenance_correlation_and_outcome_lifecycle() {
    let ws = CliWs::new();
    ws.write("tracked.txt", "before\n");
    let stdout = ws.ok(&["fs", "replace", "tracked.txt", "before", "after", "--json"]);
    let edit_id = ws.edit_id_of(&stdout);

    // Durable record correlates the edit with the workspace identity
    // from the workspace manifest and the affected path set.
    let manifest_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ws.root.join(".agent/workspace.json")).unwrap(),
    )
    .unwrap();
    let workspace_id = manifest_json["workspace_id"]
        .as_str()
        .expect("workspace_id")
        .to_owned();
    let record: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(provenance_path(&ws.root, &edit_id)).unwrap(),
    )
    .unwrap();
    assert_eq!(record["workspace_id"].as_str(), Some(workspace_id.as_str()));
    assert_eq!(record["paths"], serde_json::json!(["tracked.txt"]));
    assert_eq!(record["outcome"], serde_json::json!("committed"));

    // history renders the committed state on the read plane.
    let history: serde_json::Value =
        serde_json::from_str(&ws.ok(&["fs", "history", "--json"])).unwrap();
    assert_eq!(
        history["edits"][0]["status"],
        serde_json::json!("committed")
    );
    assert_eq!(
        history["edits"][0]["workspace_id"],
        serde_json::json!(workspace_id)
    );

    // The legal lifecycle transition Committed → RolledBack is written
    // durably by the rollback owner and visible on the same read plane.
    ws.ok(&["fs", "rollback", &edit_id]);
    let record: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(provenance_path(&ws.root, &edit_id)).unwrap(),
    )
    .unwrap();
    // Struct variants serialize as {"rolled_back": {"reason": ...}}.
    let outcome = record["outcome"]
        .as_object()
        .expect("RolledBack is a struct variant");
    let reason = outcome
        .get("rolled_back")
        .and_then(|v| v.get("reason"))
        .and_then(|v| v.as_str())
        .expect("rolled_back outcome with reason");
    assert_eq!(reason, "restored");
    let history: serde_json::Value =
        serde_json::from_str(&ws.ok(&["fs", "history", "--json"])).unwrap();
    assert_eq!(
        history["edits"][0]["status"],
        serde_json::json!("rolled_back")
    );
    assert_eq!(history["edits"][0]["edit_id"], serde_json::json!(edit_id));
}

#[test]
fn provenance_outcome_vocabulary_is_durable_and_distinguishable() {
    let dir = tempdir().unwrap();
    let store = store_at(dir.path());
    let manifest = store
        .create("edit-vocab", &[("v.txt".into(), b"v\n".to_vec())], None)
        .unwrap();

    // All three implemented outcomes persist and read back
    // distinguishably through the store's durable contract, including
    // the Failed variant the current production writer never emits
    // (readers must still handle it safely, §24).
    let variants: Vec<(&str, ProvenanceOutcome)> = vec![
        ("edit-committed", ProvenanceOutcome::Committed),
        (
            "edit-rolled-back",
            ProvenanceOutcome::RolledBack {
                reason: Some("restored".into()),
            },
        ),
        (
            "edit-failed",
            ProvenanceOutcome::Failed {
                reason: "executor refused".into(),
            },
        ),
    ];
    for (edit_id, outcome) in &variants {
        let mut record = record_for(edit_id, &manifest);
        record.outcome = outcome.clone();
        store.record_provenance(&record).unwrap();
        let back = store.provenance(edit_id).unwrap();
        assert_eq!(
            &back.outcome, outcome,
            "{edit_id} must round-trip its outcome"
        );
    }

    // Serialized outcomes stay distinct on the wire (snake_case), so a
    // future reader can never confuse them.
    let distinct: HashSet<String> = variants
        .iter()
        .map(|(id, _)| {
            let record = store.provenance(id).unwrap();
            let value = serde_json::to_value(&record).unwrap();
            serde_json::to_string(&value["outcome"]).unwrap()
        })
        .collect();
    assert_eq!(
        distinct.len(),
        3,
        "outcomes must serialize distinctly: {distinct:?}"
    );
}

// ---------------------------------------------------------------------------
// §13/AWE-014 — history is bounded and newest-first
// ---------------------------------------------------------------------------

#[test]
fn history_is_bounded_and_newest_first() {
    let ws = CliWs::new();
    ws.write("one.txt", "one\n");
    ws.write("two.txt", "two\n");
    let stdout = ws.ok(&["fs", "replace", "one.txt", "one", "uno", "--json"]);
    let first = ws.edit_id_of(&stdout);
    let stdout = ws.ok(&["fs", "replace", "two.txt", "two", "dos", "--json"]);
    let second = ws.edit_id_of(&stdout);

    // Edit ids carry a monotonically increasing prefix, so newest-first
    // is deterministic and observable through the read plane.
    let history: serde_json::Value =
        serde_json::from_str(&ws.ok(&["fs", "history", "--limit", "2", "--json"])).unwrap();
    let edits = history["edits"].as_array().expect("edits array");
    assert_eq!(edits.len(), 2);
    assert_eq!(
        edits[0]["edit_id"].as_str(),
        Some(second.as_str()),
        "newest first"
    );
    assert_eq!(edits[1]["edit_id"].as_str(), Some(first.as_str()));

    // The bounded read returns only the newest record.
    let history: serde_json::Value =
        serde_json::from_str(&ws.ok(&["fs", "history", "--limit", "1", "--json"])).unwrap();
    let edits = history["edits"].as_array().expect("edits array");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["edit_id"].as_str(), Some(second.as_str()));
}

// ---------------------------------------------------------------------------
// §20 — content limits at the exact boundary
// ---------------------------------------------------------------------------

#[test]
fn content_limits_round_trip_and_fail_closed_just_over() {
    let dir = tempdir().unwrap();
    let store = store_at(dir.path());

    // Exactly at the configured maximum: accepted, and the recovery
    // round-trip is byte- and hash-exact against the independent oracle.
    let max_bytes = MAX_SNAPSHOT_CONTENT_BYTES as usize;
    let at_limit = vec![0xA5u8; max_bytes];
    let manifest = store
        .create(
            "edit-at-limit",
            &[("big.bin".into(), at_limit.clone())],
            None,
        )
        .unwrap();
    store
        .record_provenance(&record_for("edit-at-limit", &manifest))
        .unwrap();
    let view = store.recovery_view("edit-at-limit").unwrap();
    assert_eq!(view.len(), 1);
    let (path, bytes) = &view[0];
    assert_eq!(path, "big.bin");
    let bytes = bytes.as_ref().expect("existing file yields bytes");
    assert_eq!(bytes.len(), max_bytes);
    assert_eq!(&bytes[..], &at_limit[..]);
    assert_eq!(manifest.entries[0].hash_before, oracle_sha256(&at_limit));
    assert_eq!(manifest.entries[0].size_before, max_bytes as u64);

    // One byte over the maximum: rejected BEFORE any persistence — the
    // published set is unchanged and no partial state exists.
    let before: Vec<SnapshotId> = store.list().unwrap();
    let over = vec![0x5Au8; max_bytes + 1];
    assert!(matches!(
        store.create("edit-over-limit", &[("over.bin".into(), over)], None),
        Err(SnapshotError::LimitExceeded(_))
    ));
    assert_eq!(
        store.list().unwrap(),
        before,
        "rejected input must publish nothing"
    );
}

// ---------------------------------------------------------------------------
// §7 — duplicate logical paths are rejected before persistence; sibling
// prefixes remain distinct entries
// ---------------------------------------------------------------------------

#[test]
fn duplicate_paths_are_rejected_and_sibling_prefixes_stay_distinct() {
    let dir = tempdir().unwrap();
    let store = store_at(dir.path());

    // Two entries for one logical path would make recovery ambiguous
    // (which bytes win?), so the boundary rejects the input loudly.
    let dup = vec![
        SnapshotFile::Existing {
            path: "same.txt".into(),
            bytes: b"first\n".to_vec(),
        },
        SnapshotFile::Existing {
            path: "same.txt".into(),
            bytes: b"second\n".to_vec(),
        },
    ];
    assert!(matches!(
        store.create_snapshot("edit-dup", &dup, None),
        Err(SnapshotError::InvalidId(_))
    ));
    // Validation precedes any filesystem mutation.
    assert!(!dir.path().join(".agent/snapshots").exists());

    // Sibling-prefix paths are DIFFERENT resources and must both appear.
    let manifest = store
        .create(
            "edit-siblings",
            &[
                ("src/foo".to_string(), b"foo\n".to_vec()),
                ("src/foobar.txt".to_string(), b"foobar\n".to_vec()),
            ],
            None,
        )
        .unwrap();
    let paths: Vec<&str> = manifest.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, vec!["src/foo", "src/foobar.txt"]);
    assert!(store.load(&manifest.id).is_ok());
}

// ---------------------------------------------------------------------------
// §6/§25 — recovery_view on difficult bytes against the independent oracle
// ---------------------------------------------------------------------------

#[test]
fn recovery_view_round_trips_difficult_bytes_against_independent_oracle() {
    let dir = tempdir().unwrap();
    let store = store_at(dir.path());

    let empty: Vec<u8> = Vec::new();
    let crlf = b"line one\r\nline two\r\n".to_vec();
    let no_eol = b"no final newline".to_vec();
    let nul_embedded = b"start\x00mid\x00end\n".to_vec();
    let devanagari = "बीटा परीक्षण\n".as_bytes().to_vec();
    let emoji = "emoji 🦀🚀 tail\n".as_bytes().to_vec();
    let tabs_spaces = b"\tcol\tcol   col   \n".to_vec();
    let binary: Vec<u8> = (0u8..=255).cycle().take(4096).collect();

    let files = vec![
        SnapshotFile::Existing {
            path: "empty.txt".into(),
            bytes: empty.clone(),
        },
        SnapshotFile::Existing {
            path: "crlf.txt".into(),
            bytes: crlf.clone(),
        },
        SnapshotFile::Existing {
            path: "noeol.txt".into(),
            bytes: no_eol.clone(),
        },
        SnapshotFile::Existing {
            path: "nul.txt".into(),
            bytes: nul_embedded.clone(),
        },
        SnapshotFile::Existing {
            path: "deva.txt".into(),
            bytes: devanagari.clone(),
        },
        SnapshotFile::Existing {
            path: "emoji.txt".into(),
            bytes: emoji.clone(),
        },
        SnapshotFile::Existing {
            path: "tabs.txt".into(),
            bytes: tabs_spaces.clone(),
        },
        SnapshotFile::Existing {
            path: "binary.bin".into(),
            bytes: binary.clone(),
        },
        SnapshotFile::Missing {
            path: "created-by-edit.txt".into(),
        },
    ];
    let manifest = store
        .create_snapshot("edit-difficult", &files, None)
        .unwrap();
    store
        .record_provenance(&record_for("edit-difficult", &manifest))
        .unwrap();

    // A fresh store instance over the same root re-reads durable state
    // (§16): nothing below depends on the first instance's memory.
    let fresh = store_at(dir.path());
    let view = fresh.recovery_view("edit-difficult").unwrap();

    let expected: Vec<(&str, Option<&Vec<u8>>)> = vec![
        ("binary.bin", Some(&binary)),
        ("created-by-edit.txt", None),
        ("crlf.txt", Some(&crlf)),
        ("deva.txt", Some(&devanagari)),
        ("emoji.txt", Some(&emoji)),
        ("empty.txt", Some(&empty)),
        ("noeol.txt", Some(&no_eol)),
        ("nul.txt", Some(&nul_embedded)),
        ("tabs.txt", Some(&tabs_spaces)),
    ];
    assert_eq!(
        view.len(),
        expected.len(),
        "recovery covers every manifest entry"
    );
    // Entries come back in the manifest's deterministic path-ascending
    // order; compare each path's bytes exactly (never decoded strings).
    for (idx, (path, bytes)) in expected.iter().enumerate() {
        assert_eq!(
            &view[idx].0,
            path,
            "entry order is path-ascending: {:?}",
            view.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>()
        );
        match bytes {
            Some(want) => {
                let got = view[idx]
                    .1
                    .as_ref()
                    .unwrap_or_else(|| panic!("{path} must yield its exact pre-edit bytes"));
                assert_eq!(got.len(), want.len(), "{path}: exact length");
                assert_eq!(
                    &got[..],
                    &want[..],
                    "{path}: exact bytes, nothing normalized"
                );
            }
            None => {
                assert!(
                    view[idx].1.is_none(),
                    "{path} must stay missing (distinct from an existing empty file)"
                );
            }
        }
    }

    // Independent per-entry integrity: the manifest hash/length for
    // every existing file must equal the oracle over the fixture bytes.
    let reloaded = fresh.load(&manifest.id).unwrap();
    for entry in &reloaded.entries {
        if !entry.existed_before {
            continue;
        }
        let (_, bytes) = view
            .iter()
            .find(|(p, _)| p == &entry.path)
            .expect("view covers every manifest entry");
        let bytes = bytes.as_ref().expect("existing entry yields bytes");
        assert_eq!(entry.size_before, bytes.len() as u64, "{}", entry.path);
        assert_eq!(entry.hash_before, oracle_sha256(bytes), "{}", entry.path);
    }
}
