# TP08 Report — Audit & Observability Verification

**Prompt:** `docs/testing-prompts/08-audit-and-observability.md`
**Branch:** `tp08-audit-observability-verification` (from `rust` @ 7b31ac1)
**Date:** 2026-10-04
**Suite:** `tests/audit_observability_boundaries.rs` — 9 tests, plus 1 production regression unit test in `src/services/audit.rs`
**Result:** 1 production defect found and fixed (§3); 1493 passed / 0 failed workspace-wide; fmt + clippy clean.

---

## 1. Methodology

Forensics first: the durable audit store was read at source —
`src/services/audit.rs` (`DurableStore::open` startup scan/validation
+ torn-tail repair, `append` (lock/allocator), `rotate`, `recent`
(reverse-parse bounded reads), `parse_line` (checksum envelope
verification), `redact_token_like` (≥16-char base62 runs →
`[redacted]`), `record_correlated` (workspace/agent/session/edit/
snapshot correlation), `init_global` (startup durability +
buffered degraded mode), and the trusted-identity shape guards).
The producers were traced at their real boundaries: the edit service
(`audit_edit_outcome` — `filesystem.replace` with edit-id
correlation, `replace_with_identity` in `edit.rs`), the CLI arms
that durably initialize audit before mutation (`fs`, `memory`,
`task`, `terminal`, `collaboration` in `main.rs`), the collaboration
service (`record_correlated`, kind `"collab"`, owner/revision/state
details), and the bounded readback surface (`collaboration events`).

Pre-existing coverage was inventoried so this suite adds only
audit-owned gaps: `audit.rs` unit tests already pin schema
validation, checksum integrity, duplicate-id/sequence-regression
corruption, rotation, redaction on disk, and in-process concurrent
writers; `tests/acceptance_edit_gates.rs` pins the Control API
`GET /api/v1/audit` (auth reads, kind/limit filters); TP06/TP07 pin
snapshot/provenance/rollback audit correlation and the `fs` CLI
durable-audit initialization defect.

Every test here drives the REAL boundary: the compiled `awh`
binary across separate processes (each invocation is a store
restart), the durable `.agent/audit/audit.log` bytes as the oracle —
parsed independently here with serde_json, every checksum verified
with an independent `sha2` SHA-256 over the exact serialized payload
embedded in each envelope line — and the real consequential
producers (edit, terminal, memory, task, collaboration) through
their public CLIs. No second audit store, no test-only event model,
no fake persistence.

## 2. Established implementation map (forensics)

| Concern | Owner (source of truth) |
| --- | --- |
| Durable audit events | `services::audit::DurableStore` — `.agent/audit/audit.log`, JSONL envelopes `{"checksum","event"}`, append-only, `audit.lock` (StoreLock) |
| Event schema | `AuditEntry` — ts_ms/kind/action/subject/detail/schema_version=1/event_id/sequence + optional workspace/agent/session/edit/snapshot/reason |
| Ingestion choke point | `AuditLog::record` / `record_correlated` → ring mirror + durable append; `init_global(cwd)` at every mutating CLI arm |
| Integrity | SHA-256 checksum per line; startup validation fails closed on schema mismatch, bad checksum, duplicate event id, sequence regression; torn tail repaired (truncated) |
| Redaction | `redact_token_like` at the store boundary — every field, every caller; trusted generated identity shapes (`ws-`/`agent-`/`sess-`/`edit-`/`snap-` ids) preserved verbatim |
| Retention | rotation at 10,000 events → `audit.log.1`, audible `audit.rotation` marker |
| Correlation producers | edit service (`filesystem.replace` + rollback outcomes), collaboration (`kind="collab"` transitions), task/memory/terminal CLI mutations, MCP `tool_invoke` + gate denials |
| Bounded readback | `collaboration events` (kind filter + action filter, oldest→newest); Control API `GET /api/v1/audit?kind=&limit=` |
| Diagnostic surface | stdout of commands; tracing to stderr; the in-memory ring (1000 entries) is process-local and never substitutes for durable evidence |

## 3. Defects found

### D1 (Failed → fixed): cross-process sequence race permanently bricks the durable audit store

**Reproduction (real boundary):** 4 concurrent `awh fs replace`
processes against one workspace. Every process snapshotted
`next_sequence=1` at startup; each then allocated `sequence=1` for
its own event while holding only the *write* lock. Result on disk:
four events, all `sequence: 1` — a duplicate-sequence file. Because
startup validation treats sequence regression as hard corruption,
**every subsequent process** logged
`audit_init_failed: sequence regression at record 2` and ran in
degraded buffered mode forever: the workspace's durable audit was
permanently bricked and all later evidence silently tail-lost.
Live probe: 48 concurrent attempts across 12 rounds persisted only 4
lines, all `sequence: 1`.

**Root cause:** the allocator state (`next_sequence`) was a
process-startup snapshot; the StoreLock serialized the file write
but not the allocation. The in-process unit test
(`concurrent_writers_are_safe_and_ordered`) shared one `AuditLog`
and one snapshot, so the cross-process window was invisible to it.

**Fix (`src/services/audit.rs`):** sequence allocation now derives
from the durable tail *under the append lock*. A bounded O(1) tail
read (`tail_sequence`, 64 KiB window — larger than any bounded
record, torn-tail aware, full-scan fallback) reads the last valid
sequence of both the active generation and `audit.log.1`, so
concurrent processes can never publish a duplicate and a rotation
marker can never regress below the rotated generation. Rotation
(rename + marker) is now one serialized unit under the same lock.

**Regression tests (§26):**
`services::audit::persistent_audit_tests::independent_store_handles_never_duplicate_sequences`
(two store handles on one root — the deterministic, in-process
reproduction of the snapshot race) and
`audit_observability_boundaries::concurrent_processes_append_cleanly`
(8 real concurrent CLI processes × 2 rounds; unique ids, strictly
monotonic file-order sequences, then a fresh process appends
cleanly — the bricked-vs-healthy discriminator). Live probe after
the fix: 48/48 events, 48 unique ids, 0 duplicates, store healthy.

**No existing assertion was weakened.** All 23 pre-existing audit
unit tests pass unchanged (the rotation test included).

### D2 (verified non-defect): degraded-mode tail loss is documented behavior

When the durable store cannot initialize (corruption/blocked
storage), the CLI proceeds best-effort: the operation stands, its
event stays in the dying process's buffer, and no evidence is
fabricated. Pinned by test as contract behavior
(`audit_storage_failure_is_best_effort_and_evidence_survives`,
`middle_corruption_fails_closed_and_preserves_evidence`), not
changed in production.

## 4. Test-by-test evidence

| # | Test | Verifies (prompt §) |
| --- | --- | --- |
| 1 | `durable_events_survive_restart_with_independent_oracle` | §6 unique event identity (prefix + ≥16 chars, unique set); §7 strictly monotonic sequence in file order; §11 restart durability across 2 processes; §23 workspace correlation on every event; §10 stdout is a diagnostic surface (no envelope/checksum/sequence/kind in command output); §25 operation+state+audit agree |
| 2 | `workspace_isolation_at_the_durable_boundary` | §16 two real workspaces, identical relative paths: every event claims only its own workspace id; the foreign edit id never appears in the other workspace's bytes |
| 3 | `concurrent_processes_append_cleanly` | §20 + §26 regression: 8 real concurrent CLI processes on ONE store — all events land, no duplicate sequence/id, file order strictly monotonic, and the store still initializes cleanly afterwards (D1) |
| 4 | `identity_correlated_operations_reconstruct_from_durable_audit` | §9 security-investigation workflow: real agent/session lifecycle (own processes each), collab `assign` carries agent_id/session_id/workspace_id/reason; reconstruction via the real `collaboration events` readback; identity fields record who, never contents |
| 5 | `canaries_never_reach_durable_audit` | §14 secret protection: token-shaped canary in edited file content, credential-shaped memory content, sentinel terminal argument — raw durable bytes swept; operations' real state independently confirmed (file content, memory list) |
| 6 | `audit_storage_failure_is_best_effort_and_evidence_survives` | §19 + §28: `.agent/audit` blocked as a file — op succeeds (exit 0, file edited), blocked path never re-created, prior evidence byte-identical across the outage, degraded event not fabricated, repair resumes without sequence reuse or regression |
| 7 | `middle_corruption_fails_closed_and_preserves_evidence` | §13: corrupted middle checksum — corrupt bytes never overwritten, valid records still readable, degraded op proceeds, repair restores exact original line and new events continue with prior identities stable |
| 8 | `torn_tail_is_repaired_with_sequence_continuity` | §12: partial final line (interrupted append) — startup repair truncates it, prior history byte-identical, next append continues at exactly max_sequence+1 |
| 9 | `bounded_readback_filters_and_never_mutates` | §17: `collaboration events` action filter — exact match narrows, unknown/path-shaped values are data (no path/shell interpretation), explicit empty result, and reads never rewrite history (byte-identical file) |

Plus the in-source regression
`independent_store_handles_never_duplicate_sequences` (D1).

## 5. Classification (§34)

### Passed (direct evidence through the real boundary)

- Event identity/uniqueness, ordering/monotonic sequence, schema
  version, checksum integrity — independent oracle (§6–§8).
- Restart durability and per-process store lifetimes (§11).
- Correlation: workspace on every event; agent/session/edit on
  identity-carrying operations (§9, §23).
- Secret protection at the durable bytes across edit, memory and
  terminal planes (§14).
- Workspace isolation at the durable boundary (§16).
- Concurrent cross-process appends: unique identities, monotonic
  order, healthy store afterwards (§20) — including the D1
  regression.
- Corruption handling: middle-corruption fail-closed with evidence
  preservation; torn-tail repair with sequence continuity (§12–§13).
- Append failure semantics: best-effort audit, operation stands,
  no fabrication, repair resumes cleanly (§19).
- Bounded readback filters and read-side immutability (§17).
- Audit vs logs separation: command stdout/tracing never stand in
  for the durable record (§10).

### Covered by existing suites (verified, cited)

- Control API audit reads: auth (401 + audited), kind/limit
  filters, structured errors — `tests/acceptance_edit_gates.rs`
  (§23 transport-owned; integration-level audit checks here).
- Schema/duplicate-id/sequence-regression corruption unit tests,
  rotation, on-disk redaction — `src/services/audit.rs` unit tests.
- Snapshot/provenance/rollback audit correlation — TP06/TP07
  suites (`snapshot_provenance_boundaries.rs`,
  `rollback_recovery_boundaries.rs`).
- Ring-buffer/diagnostic log surface — `services::audit` ring
  unit tests + Control API `/api/v1/logs` (Phase 5 tests).

### Not implemented (honest classification, not defects)

- `awh audit list|show|search|export` and `awh logs show|follow|
  clear` CLI commands from the target contract — **roadmap** items
  per `docs/CLI.md` (the §1 feature boundary table marks them
  planned). The durable store, its integrity model, and the
  `collaboration events` readback exist; a general audit CLI does
  not. No tests were fabricated for them.
- MCP/Control API *write* paths for audit events (by design —
  audit records are produced at service boundaries, never by
  client request).

### Unproven / residuals (documented, not fixed)

- Retention overshoot under cross-process rotation is bounded but
  not exactly 10,000 (each process's line-count view can lag
  foreign appends; cap bounds the generation, not the instant).
  Documented in the allocator comment; rotation behavior itself is
  pinned by the existing unit test.
- Degraded-mode (failed store init) events are buffered in the
  dying process and lost at exit — documented tail loss, pinned as
  contract behavior by test 6.
- `audit export` destination security (§18) is moot until the
  export CLI exists.

### Blocked

- None. TUI audit screen (§24) exists and reads the canonical ring
  (Phase 30) with no second store; visual behavior is not re-tested
  here per the prompt's deterministic-backend allowance.

## 6. Gates

- `cargo fmt --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — **1493 passed / 0 failed** (42
  targets; 1483 baseline + 9 boundary tests + 1 regression unit
  test). Rotation test re-verified fast (2.09s) after the
  allocator moved to the O(1) tail read.

## 7. No theater checklist (§31)

- **Black-box:** every test spawns the compiled `awh` binary in
  fresh processes; no test calls an audit API in-process.
- **State verified independently:** durable bytes are the oracle —
  parsed with serde_json here, checksums recomputed with the
  `sha2` crate, never through the store's own readers; file
  contents / memory lists confirm the canary operations really
  happened.
- **Gaps classified, not faked:** the target-contract audit/logs
  CLI verbs are absent from the binary and are reported as
  roadmap, with no fabricated tests.
- **Defect discipline:** D1 reproduced live at the real boundary
  before any fix, root-caused in source, fixed in the canonical
  boundary only, pinned by a deterministic in-process regression
  AND a real-process race test; no existing assertion weakened;
  full pre-existing suite green afterwards.
- **Cross-platform:** corruption probes use checksum-byte flips
  (no platform-specific line assumptions); the storage-failure
  probe uses a file-as-directory (ENOTDIR) block, valid on all
  three CI platforms; no raw-path canonicalization assertions.
