# TP06 Report — Snapshots & Provenance Verification

**Prompt:** `docs/testing-prompts/06-snapshots-and-provenance.md`
**Branch:** `tp06-snapshot-provenance-verification` (from `rust` @ 0689bb6)
**Date:** 2026-10-04
**Suite:** `tests/snapshot_provenance_boundaries.rs` — 13 tests (12 feature + 1 oracle self-check)

---

## 1. Methodology

Forensics first: the durable snapshot/provenance subsystem was read at
source (`src/services/snapshot.rs`, 1483 lines; the `capture_durable_recovery`
/ `record_edit_provenance` / `correlate_rollback_outcome` writers in
`src/services/edit.rs`; the `awh fs edit|verify|history|rollback` CLI
plane in `src/cli/fs_edit.rs`), then all pre-existing coverage was
inventoried (23 unit tests inside `snapshot.rs`, 15 in
`tests/cli_fs_edit.rs`, 8 in `tests/acceptance_editing.rs`) so the new
suite adds only what those leave open. Every new test drives a real
boundary — the compiled `awh` binary across separate processes, or the
canonical `SnapshotStore` over real temporary workspaces with durable
state inspected directly on disk. No store is mocked and no production
hash helper is used as the integrity oracle: an independent SHA-256
implementation lives in the test file, self-checked against the FIPS
180-4 "abc" vector before any assertion trusts it (§25).

## 2. Established implementation map (forensics)

| Concept | Status | Source |
|---|---|---|
| `SnapshotId` (`snap-<nanos>-<pid>-<seq>`, validated) | Implemented | `snapshot.rs:62-118` |
| `SnapshotEntryId` (`entry-NNNNNN`, per-snapshot namespace) | Implemented | `snapshot.rs:121-152` |
| `FileSnapshot` manifest (schema v1, path-ascending entries) | Implemented | `snapshot.rs:199-217` |
| `FileSnapshotEntry` (hash/size/line-count/content_len/existed_before) | Implemented | `snapshot.rs:175-197` |
| `SnapshotFile::Missing` vs `Existing` (missing ≠ empty) | Implemented | `snapshot.rs:155-171` |
| `SnapshotStore::{create, create_snapshot, load, entry_bytes}` | Implemented | `snapshot.rs:356-627` |
| Blob per snapshot dir + read-back verify before manifest publish | Implemented | `snapshot.rs:431-458` |
| `record_provenance` / `provenance` / `list_provenance` (bounded, newest-first) | Implemented | `snapshot.rs:629-702` |
| `recovery_view(edit_id)` canonical recovery read | Implemented | `snapshot.rs:736-760` |
| `ProvenanceRecord` (edit/snapshot/workspace/agent/session correlation, hash_edges) | Implemented | `snapshot.rs:219-243` |
| `ProvenanceOutcome` {Committed, RolledBack{reason}, Failed{reason}} | Implemented (snake_case wire) | `snapshot.rs:245-251` |
| Content-addressed dedup / shared blobs | **Not implemented** (v1 layout is per-snapshot isolation by design, §22 comment) | `snapshot.rs:322-337` |
| Snapshot GC / orphan reclamation | **Not implemented** (not required; residue proven inert) | — |

Production provenance writers: `record_edit_provenance` (Committed,
with agent/session/workspace identity), `correlate_rollback_outcome`
(RolledBack{reason: restored|already-rolled-back|conflict|failed},
best-effort §25). `Failed{reason}` is vocabulary-only — never emitted
by the current writer; readers still handle it (pinned).

## 3. Defects found and fixed

### D1 (production, fixed): `awh fs verify` did not verify the edit→snapshot binding

`fs verify` loaded the provenance record, then called
`SnapshotStore::load(provenance.snapshot_id)` — schema/id/blob integrity
only. A provenance record whose `snapshot_id` was swapped to *another
edit's valid snapshot* still printed
`edit <id> recovery material verified`, because `load` does not know
which edit the caller asked about. The canonical `recovery_view`
boundary *does* enforce `manifest.edit_id == edit_id` (and the rollback
owner uses verified recovery material), so the defect was verify-only:
the read-only diagnostic claimed "verified" for a chain whose central
link was broken — exactly the "a valid snapshot must not become usable
merely because its bytes look plausible" failure §11 forbids.

**Fix:** verify now resolves the whole chain through
`SnapshotStore::recovery_view(&edit_id)` (provenance → manifest
integrity → binding → per-entry hash) and reports `entries` from the
authoritative manifest rather than the observational provenance
`paths`. Exit codes and the unknown-id hint message are unchanged.
Pinned by `provenance_snapshot_swap_between_real_edits_fails_closed`:
after swapping the two `snapshot_id` values between two real edits'
durable records (valid JSON, both snapshots hash-valid), verify AND
rollback exit 5 for both edits and neither file's bytes cross.

### D2 (boundary hardening, fixed): duplicate logical paths were silently accepted

`create_snapshot` accepted two entries for the same path, producing an
ambiguous manifest (which entry's bytes win on recovery?) — forbidden
by §7 ("a snapshot must never silently contain two ambiguous
representations of one logical path"). Production callers deduplicate
upstream (`affected` is path-keyed in `edit.rs:2440-2482`), so no
legitimate path is affected. **Fix:** duplicate paths now fail closed
with `InvalidId` before any filesystem mutation (validation still
precedes `ensure_dirs`). Pinned by
`duplicate_paths_are_rejected_and_sibling_prefixes_stay_distinct`,
which also pins that sibling-prefix paths (`src/foo` vs
`src/foobar.txt`) remain distinct valid entries.

## 4. Test-by-test evidence

| # | Test | Boundary | Prompt § |
|---|---|---|---|
| 0 | `oracle_sha256_matches_fips_vector` | oracle self-check | §25 |
| 1 | `two_edits_recover_independently_across_processes` | CLI × real processes | §26-C |
| 2 | `provenance_snapshot_swap_between_real_edits_fails_closed` | CLI + on-disk tamper | §11 |
| 3 | `identical_edit_ids_and_paths_never_cross_workspaces` | store, two roots | §12 |
| 4 | `crash_residue_orphan_blobs_and_stolen_manifests_are_never_valid` | store + disk residue | §17/§18 |
| 5 | `durable_metadata_never_embeds_file_content_or_secrets` | CLI + `.agent` tree scan | §23 |
| 6 | `cross_process_recovery_chain_with_repeated_reads` | CLI × 8 processes | §16 |
| 7 | `provenance_correlation_and_outcome_lifecycle` | CLI + durable JSON | §13/§14 |
| 8 | `provenance_outcome_vocabulary_is_durable_and_distinguishable` | store | §14/§24 |
| 9 | `history_is_bounded_and_newest_first` | CLI | §13 |
| 10 | `content_limits_round_trip_and_fail_closed_just_over` | store, 8 MiB boundary | §20 |
| 11 | `duplicate_paths_are_rejected_and_sibling_prefixes_stay_distinct` | store (fix regression) | §7 |
| 12 | `recovery_view_round_trips_difficult_bytes_against_independent_oracle` | store + fresh instance | §6/§16/§25 |

Highlights:

- **Exact-byte fidelity** (§6): empty, CRLF, no-final-newline, embedded
  NULs, Devanagari, emoji, tabs/multi-space, full 0-255 binary cycle —
  every byte sequence recovers exactly through `recovery_view` after a
  store re-instantiation, with the manifest's `hash_before`/`size_before`
  matching the INDEPENDENT oracle over the fixture bytes, and a
  pre-edit-missing file returning `None` (never an empty file).
- **Cross-process persistence** (§16): a fresh `awh` process per
  invocation chains edit → verify ×2 → rollback → verify → rollback
  (already_rolled_back, exit 0) → verify, with exact bytes at every
  step — durable ids, manifests, blobs and provenance all survive real
  process boundaries.
- **Workspace isolation** (§12): the adversarial shape — identical edit
  id, identical path, different bytes in two workspaces — each resolves
  its own material; a snapshot id from A is `NotFound` in B's store.
- **Crash residue** (§17/§18): orphan blob dir (blobs committed, no
  manifest) is never listed/loaded/recoverable; a manifest copied
  verbatim under a different id fails the declared-id check; provenance
  pointing at an unpublished snapshot fails closed; the real edit's
  material stays usable throughout.
- **Secret-safe durable state** (§23): a sentinel token in the edited
  file appears in exactly one durable file — the recovery blob (the
  contract) — and in NO manifest/provenance JSON; `fs history`/`fs
  verify` output contains no content.
- **Provenance lifecycle** (§13/§14): durable record carries the
  workspace identity from `.agent/workspace.json`, the affected paths,
  and the snake_case outcome; rollback rewrites Committed →
  `rolled_back{reason:"restored"}` durably and `fs history --json`
  renders the transition; all three vocabulary outcomes round-trip and
  serialize distinctly, including `failed`, which today's writer never
  emits.
- **Limits** (§20): exactly `MAX_SNAPSHOT_CONTENT_BYTES` (8 MiB)
  round-trips byte- and hash-exact; one byte over fails `LimitExceeded`
  with the published set unchanged.

## 5. Classification (§32)

### Passed (direct evidence through the real boundary)
- Exact-byte snapshot fidelity incl. difficult bytes; missing ≠ empty.
- Manifest/blob integrity: length + SHA-256 verified at write
  (read-back before publish) and at every read; corruption matrix
  (truncated/altered blobs, malformed manifest, unsupported schema,
  foreign id) fails closed — existing unit suite plus §16-fresh
  re-reads here.
- Edit→snapshot binding: forged/cross-edit records fail closed at
  `recovery_view`, on the CLI verify/rollback planes, AND for
  valid-JSON swaps between two real edits (D1 regression).
- Workspace isolation incl. identical edit ids and paths.
- Persistence/restart across real processes with repeated reads.
- Publication atomicity (blobs-before-manifest ordering, lock, atomic
  rename) — residue shapes proven inert (§17).
- Provenance correlation fields and outcome lifecycle
  (Committed→RolledBack durable and observable; Failed readable).
- Bounded newest-first history.
- Content limit boundary; file-count limit (unit-pinned, 256).
- Path safety at the snapshot boundary (canonical `validate_path`,
  rejected before persistence; sibling prefixes distinct; duplicates
  now fail closed).
- No content/secret embedding in metadata planes or CLI recovery
  reads.

### Failed → Fixed this round
- D1: `fs verify` skipped the edit→snapshot binding (now uses the
  canonical `recovery_view`).
- D2: duplicate-path ambiguity accepted at the store boundary (now
  rejected before persistence).

### Not implemented (honest classification, not defects)
- Content-addressed blob deduplication / cross-snapshot content reuse
  (v1 layout deliberately isolates blobs per snapshot; §22).
- Snapshot garbage collection / orphan reclamation (residue proven
  inert without it).
- `ProvenanceOutcome::Failed` is never written by current production
  code (vocabulary + CLI rendering only).
- No `awh fs` snapshot-listing/provenance-inspection commands beyond
  `verify`/`history` (e.g. no `fs snapshots list`); store APIs cover it.

### Unproven / residuals (documented, not fixed)
- Repeated-separator and `./`-component path strings (e.g. `a//b`)
  pass the canonical `validate_path` untouched; the raw string is
  stored, so one logical file could in principle appear under two
  distinct raw aliases *within one snapshot*. Containment is not at
  risk (paths are revalidated at rollback time and never escape the
  root), and production callers never emit such shapes. Recorded as a
  normalization residual of the canonical path rules, which this
  prompt forbids re-implementing at a second layer (§20 boundary note).
- Provenance-write failure during rollback is best-effort by contract
  (§25 of the snapshot spec): the filesystem outcome is never rewritten
  to "fix" provenance; a broken record surfaces as a warn + later
  fail-closed reads. Pinned by existing unit
  `provenance_failure_does_not_invalidate_the_snapshot`.

### Blocked
- None.

## 6. Gates

- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --workspace` — full suite green incl. the 13 new tests
  (final count in the PR body); the two production fixes re-verified
  green against the pre-existing `cli_fs_edit` (15) and
  `acceptance_editing` (8) suites that pin the CLI edit plane.

## 7. No theater checklist (§29)

- No mocked `SnapshotStore`; no second store/integrity implementation
  in test code (the §25-sanctioned independent SHA-256 oracle is
  self-proven against FIPS vectors and used only as an oracle).
- No normalized-string comparisons: every byte assertion is raw
  `Vec<u8>` equality.
- Persisted state inspected directly on disk (manifest/provenance JSON
  parsed in-test; `.agent` tree walked for the secret scan).
- Restart evidence is cross-process (fresh `awh` per invocation), not
  in-memory object survival.
- Corrupted state is asserted to fail closed, never deleted to claim
  recovery success; unrelated valid snapshots asserted usable after
  every injected failure.
- No snapshot id was used as proof of authorization (TP05 owns that).

_Maintained by an AI agent (OpenHands) on behalf of the repository
owner, per the TP01–TP06 master-test-prompt methodology._
