# TP07 Report — Rollback & Recovery Verification

**Prompt:** `docs/testing-prompts/07-rollback-and-recovery.md`
**Branch:** `tp07-rollback-recovery-verification` (from `rust` @ abc6192)
**Date:** 2026-10-04
**Suite:** `tests/rollback_recovery_boundaries.rs` — 14 tests (13 feature + 1 oracle self-check)

---

## 1. Methodology

Forensics first: the rollback engine was read at source —
`EditService::rollback_edit` (canonical CLI/MCP entry, `edit.rs:2178`),
`resolve_recovery_records` (exact-id eligibility + every binding,
`edit.rs:2028`), `rollback_edits_as`/`rollback_edits_inner`
(authorization, FS-001 coordination, two-phase conflict-check/restore
with post-mutation re-read verification, `edit.rs:3568`),
`capture_rollback_records` (AWE-010 programmatic surface), and the CLI
`fs rollback` plane (`fs_edit.rs`). Pre-existing coverage was inventoried
so this suite adds only rollback-owned gaps: `edit.rs` unit tests
(rollback restores exact bytes / deletes created files / refuses external
change), `tests/cli_fs_edit.rs` (rollback + idempotent repeat, conflict
exit 4, traversal, workspace isolation), `tests/acceptance_editing.rs`
(cross-plane parity, exact-byte matrix incl. rollback, MCP restart,
corrupt-blob fail-closed, copied-`.agent` rejection, symlink escape),
TP06's `snapshot_provenance_boundaries.rs` (provenance swap → rollback
exit 5, cross-process chain, outcome lifecycle), and the TP05-era
authorization unit tests (`rollback_obeys_policy_deny` — decision-level).

Every test here drives a REAL boundary: the compiled `awh` binary
across separate processes for human CLI workflows, or the canonical
`EditService`/`EditAuthorizer` over real initialized workspaces for the
authorization/identity shapes. Filesystem truth is verified with raw
byte reads plus an in-file independent SHA-256 oracle self-checked
against FIPS 180-4 (§27). Nothing is mocked and no second rollback
engine, snapshot store, policy evaluator, path resolver or audit log is
reimplemented.

## 2. Established implementation map (forensics)

| Concept | Status | Source |
|---|---|---|
| `rollback_edit(edit_id)` canonical entry (eligibility → material → authorize → preflight → restore → verify → correlate) | Implemented | `edit.rs:2178-2260` |
| Exact-id eligibility: `is_safe_edit_id`, provenance edit-binding, workspace binding, manifest edit-binding, path revalidation at rollback time (§27) | Implemented | `edit.rs:2028-2153` |
| Authorization BEFORE any mutation (`EditAction::Rollback` = `filesystem.rollback` over the resolved resource set) | Implemented | `edit.rs:2190`, `authorization.rs` |
| Produced-state conflict guard (`after_hash`) + already-at-pre-edit triage (§K mixed-state retry) | Implemented | `edit.rs:2218-2250` |
| Multi-file complete preflight before any restoration | Implemented | preflight triage over all records; phase-1 re-check in `rollback_edits_inner` |
| FS-001 coordination locks over all targets across conflict-check + restore | Implemented | `edit.rs:3578-3583` |
| Exact-byte restoration via `write_atomic_locked` + re-read byte compare; created-file `delete_locked` + verified absence | Implemented | `edit.rs:3603-3645` |
| `EditRollbackStatus` {Restored, AlreadyRolledBack, Conflict, Failed} | Implemented | CLI renders restored / already_rolled_back / conflict / failed; exit codes 0 / 0 / 4 / 4+ |
| Durable outcome correlation (`Committed → RolledBack{reason}`) + audit (`filesystem.rollback`, reasons incl. `rollback_conflict`) | Implemented | `correlate_rollback_outcome`, `record_rollback_audit` |
| Created-file derivation from durable material (provenance path ∉ manifest → existed_before=false) | Implemented; **unreachable by current writers** — no production executor creates files (capture docs say "the canonical executors edit existing files"); exercised here via the canonical `capture_rollback_records` + `rollback_edits_as` service surface | `edit.rs:2131-2148` |
| TOCTOU re-check between preflight and commit | Implemented (phase-1 re-check under coordination locks); a true mid-flight writer race is owned by the FS-001 coordination suite | `edit.rs:3586-3602` |
| Partial-rollback representation (restore some files, report Failed honestly) | Implemented as best-effort with honest `Failed{reason}`; mixed-state retry completes safely (pinned) | `edit.rs:3612-3644` |

## 3. Defects found

**None.** This round found zero production defects — the two TP06
fixes (verify binding, duplicate-path rejection) already protect the
rollback consumption boundary. Three sharp behaviors were pinned as
designed contracts rather than "fixed", because they are the safe
answer, not a bypass:

- **Repeat-after-success with newer content conflicts, not no-ops**
  (§18a): after a completed rollback, bytes that match neither the
  transaction-produced state nor the exact pre-edit state are an
  external change → `Conflict` exit 4, bytes preserved. Only an exact
  pre-edit match yields the idempotent `already_rolled_back` no-op.
  Both outcomes satisfy §18a's invariant ("never overwrite newer
  work"); the stricter choice is pinned as the contract.
- **Record-surface repeat reports `Conflict` ("target disappeared")**,
  not `AlreadyRolledBack`: the AWE-010 record executor is one-shot
  execution material; a repeat on absent targets fails closed without
  mutation. The canonical edit-id surface owns idempotent repeat.
  Pinned honestly rather than papered over.
- **Malformed edit ids exit 1** (generic service error via
  `PatchValidationFailure`) while unknown-but-well-formed ids exit 5
  (recovery): the CLI's documented taxonomy maps malformed input to
  the catch-all, not the recovery category. Deterministic and
  fail-closed; recorded, not changed.

## 4. Test-by-test evidence

| # | Test | Boundary | Prompt § |
|---|---|---|---|
| 0 | `oracle_sha256_matches_fips_vector` | oracle self-check | §27 |
| 1 | `eligibility_id_shapes_fail_deterministically_without_fallback` | CLI | §6 |
| 2 | `existing_empty_and_boundary_size_rollback_restore_exact_bytes` | CLI | §9/§10/§26 |
| 3 | `multi_file_patch_rollback_restores_every_file_exactly` | CLI (`fs patch`) | §10/§12 |
| 4 | `multi_file_preflight_conflict_blocks_every_restoration` | CLI | §12/Workflow E |
| 5 | `mixed_state_retry_completes_without_touching_restored_targets` | CLI | §18/§K |
| 6 | `repeat_after_success_preserves_newer_changes` | CLI | §18a/Workflow I |
| 7 | `repeated_conflict_preserves_the_external_change` | CLI + durable provenance | §18b/§17 |
| 8 | `created_file_is_deleted_only_when_still_edit_attributable` | EditService (AWE-010 surface) | §9 |
| 9 | `externally_changed_created_file_is_never_deleted` | EditService | §11 |
| 10 | `rollback_authorization_denial_leaves_filesystem_untouched` | EditService + real authorizer | §7/Workflow F |
| 11 | `identity_is_recorded_preserved_and_never_authority` | EditService (`*_as` surfaces) | §23/§24 |
| 12 | `concurrent_rollback_requests_never_corrupt_or_double_restore` | CLI × 2 real processes | §21 |
| 13 | `rollback_audit_is_durable_and_content_free` | CLI + durable `.agent/audit/audit.log` | §24/§25 |

Highlights:

- **Exact identity/eligibility (§6):** traversal-shaped, path-shaped,
  empty, and over-long ids all fail deterministically with zero
  mutation; a *path that a real edit touched* is never interpreted as
  an edit selector; no fallback to latest/timestamp/path ever fires —
  the real edit's produced state and recovery chain stay usable.
- **Workflow matrix (§9/§10):** empty-existing stays an existing empty
  file after rollback (never deleted, never confused with missing);
  one-byte files restore exactly; non-empty → emptied → restored; an
  exactly-8 MiB file (edit-plane boundary, equal-length token) rolls
  back byte-exact against the independent oracle. Multi-file patch
  rollback restores CRLF, emoji and nested files byte-exactly and
  leaves unrelated files untouched.
- **Complete preflight (§12):** with C externally changed, the
  conflict fires before A or B is restored (both remain in produced
  state); clearing the conflict lets the SAME edit id complete every
  restoration. **Mixed-state retry (§K):** with A already back at
  pre-edit bytes and C conflicting, the retry skips A (never
  re-restores, never conflicts on it), refuses on C, and after C is
  restored completes only B+C — the designed deadlock-avoidance,
  pinned end-to-end.
- **Created-file lifecycle (§9/§11):** an edit-created file is deleted
  only while it is still the exact produced state; once externally
  changed it is never deleted (`Conflict`, external bytes kept); a
  completed deletion never fabricates a second success.
- **Authorization before mutation (§7):** an agent with NO grant and an
  agent with an out-of-scope grant (`docs/` vs `src/`) are both denied
  with `AuthorizationDenied` through the real authorizer, the target
  file stays byte-identical in produced state, and the trusted operator
  can still roll the edit back afterwards — denial never corrupts
  recovery material.
- **Identity (§23/§24):** the durable provenance record carries the
  editor's agent/session identity; a different legitimately-granted
  agent may perform the rollback (capability — not identity, not
  EditId possession — is the authority), and the correlation never
  rewrites the original editor identity while recording the
  `rolled_back` outcome.
- **Concurrency (§21):** two real processes racing the same edit id
  produce only safe interleavings — at most one reports `restored`, the
  other reports already-rolled-back (exit 0) or conflict (exit 4),
  and the final bytes are exactly the pre-edit state.
- **Audit (§24/§25):** durable `.agent/audit/audit.log` records the
  `filesystem.rollback` action, the exact edit id, and the
  `rollback_conflict` reason — and never the file contents (sentinel
  scan).

## 5. Classification (§34)

### Passed (direct evidence through the real boundary)
- Exact-EditId selection with no path/timestamp/latest fallback.
- Eligibility: malformed/unknown/missing-material edits fail closed
  with zero mutation (corrupt-material shapes inherited from
  TP03/TP06 suites remain green).
- Authorization before mutation; capability scope enforced; denial
  leaves filesystem and recovery material intact.
- Canonical recovery-material consumption through
  `resolve_recovery_records` (provenance→manifest→blob with every
  binding re-verified).
- Produced-state conflict detection: content changed, same-size
  different content, externally-changed created files — all conflict
  with the resource untouched.
- Complete multi-file preflight before any restoration; mixed-state
  retry completion without touching already-restored targets.
- Exact-byte restoration (ASCII/UTF-8/emoji/CRLF/empty/one-byte/8 MiB)
  verified by raw reads + independent oracle.
- Missing-vs-empty preserved; created-file removal only when safely
  attributable; verified absence after deletion.
- Post-rollback verification: engine re-reads and byte-compares every
  restoration (pinned at the source level; injection of write-fault
  mid-restore is not exposed cross-platform — see Unproven).
- Result vocabulary Restored/AlreadyRolledBack/Conflict/Failed rendered
  distinctly with exit codes 0/0/4/4+; conflict distinguishable from
  generic failure.
- Idempotent repeat on exact pre-edit state; newer-content repeat
  conflicts without overwriting.
- Restart/persistence: every CLI invocation is a fresh process
  resolving from durable state (§19), including the mixed-state and
  conflict-clearing sequences.
- Workspace isolation: inherited green from TP06
  (`identical_edit_ids_and_paths_never_cross_workspaces`,
  `provenance_snapshot_swap_between_real_edits_fails_closed`) plus the
  pre-existing CLI isolation test.
- Provenance/audit correlation with ids/outcomes only; no content or
  secrets in user-visible output or durable logs.
- Concurrent same-edit rollback requests are safe (union invariant).

### Not implemented (honest classification, not defects)
- No production edit executor creates files today, so the durable
  created-file derivation (provenance path ∉ manifest) is unreachable
  from real edits; its executable semantics are pinned at the
  canonical service surface (`capture_rollback_records` +
  `rollback_edits_as`).
- `EditRollbackStatus::Failed` restoration-fault paths exist in the
  engine but no CLI/MCP knob injects mid-restore faults (see
  Unproven).

### Unproven / residuals (documented, not fixed)
- **Mid-restoration fault injection (§20):** the engine's honest
  `Failed{reason}` + verified-re-read design is source-verified and
  the conflict/no-mutation paths are pinned, but a real write fault
  between two file restorations cannot be injected cross-platform
  without new production hooks. The mixed-state retry test pins the
  recovery contract such a crash would rely on (§K: already-restored
  targets are skipped, pending ones completed).
- **True mid-flight writer race (§21):** FS-001 coordination locks
  serialize rollback against the canonical write path; the pinned
  process-level race covers the observable CLI contract. The
  lock-level interleavings are owned by `tests/fs_coordination.rs`.
- **proptest property battery (§29):** the critical properties
  (unknown ids never resolve; non-matching bytes always conflict;
  exact bytes always round-trip; repeats never overwrite) are each
  pinned deterministically; a randomized battery would duplicate
  those pins with flaky timing. Recorded as a deliberate scope choice.

### Blocked
- None.

## 6. Gates

- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --workspace` — full suite green incl. the 14 new tests
  (final count in the PR body).

## 7. No theater checklist (§31)

- No test constructs a `RollbackRecord` and calls a pure helper while
  claiming rollback: the created-file tests use the canonical
  capture→commit→`rollback_edits_as` path over real files, and every
  CLI test drives the compiled binary.
- No mocked filesystem, no second engine/store/resolver; the only
  in-file reimplementation is the §25/§27-sanctioned independent
  SHA-256 oracle, self-proven against FIPS vectors.
- Every consequential test asserts the authoritative result AND the
  actual filesystem bytes AND (where applicable) durable
  provenance/audit state.
- Conflict assertions verify the conflicting resource AND unrelated
  resources remain untouched — never deletion-then-claim-success.
- No file contents or secrets printed in assertions or failure
  messages (sentinel values are searched, never echoed).
- Restart evidence is cross-process; no in-memory record survival is
  trusted.

_Maintained by an AI agent (OpenHands) on behalf of the repository
owner, per the TP01–TP07 master-test-prompt methodology._
