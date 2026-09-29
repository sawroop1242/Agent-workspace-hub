# Prompt 16 — Before-State Acceptance Map (AWE-015 / AWE-017 / TW-008)

Written BEFORE any new tests, per `docs/implementation-prompts/16-testing-and-acceptance.md` §7.
Evidence classifications: **implemented-and-verified**, **implemented-but-unverified**,
**partial**, **declared-but-disconnected**, **not-implemented**, **test-harness defect**,
**environment limitation**, **security regression**.

| Contract | Production owner | Interfaces | Status | Existing evidence | Missing evidence |
|---|---|---|---|---|---|
| Workspace containment (traversal/symlink escape) | `services::files::FilesService::resolve_checked` (canonicalize deepest existing ancestor; reject escape); `services::edit::validate_path` (form) | MCP/CLI/API/TUI | implemented-and-verified | files.rs unit `symlink_escape_is_rejected` (unix); `cli_fs_edit.rs::fs_traversal_and_unsafe_paths_fail_closed`; mcp_security traversal tests | Symlink escape **through the edit plane** (CLI `fs replace` / MCP `filesystem.*` on an in-root symlink pointing outside) is not pinned anywhere; symlinked-parent variant untested |
| Caller identity | `core::identity` (SessionIdentity, IdentityRelation) + `services::agent_runtime::resolve_session`; MCP `/{agent}` routes bind callers | MCP/API/CLI | implemented-and-verified | `tests/mcp_agent_routes.rs` (38: unknown/disabled agent, session binding, workspace mismatch, no migration) | Edit-plane-specific identity evidence via agent routes (all 38 use `workspace.write_file`/`tools/list`) |
| Capability/policy | TW-003 `authorize_caller_capability` (`CapabilityGrantStore`, scope/expiry) + SEC-001 `authorize_builtin` (`awh.builtin` trust, `PersistentTrustStore`) + workspace deny-rules (`PolicyStore`, exactly write_file/delete_file/terminal.run) | MCP/CLI/API | implemented-and-verified | `tests/mcp_builtin_tool_gate.rs` (74: default deny, grants, revoke, fail-closed corruption, CLI trust/block parity); `tests/mcp_policy_gate.rs` (40) | **Zero-side-effect denial on the edit plane** (`filesystem.*` are Medium/behind the trust gate and capability-gated for agent routes): no test asserts bytes + snapshot/provenance material + audit stay untouched when `filesystem.*` is denied |
| Edit transaction (Replace/Insert/DeleteRange/Patch/ApplyDiff) | `services::edit::EditService` (single owner; `*_as` take `AuthorizingPrincipal`) | MCP/CLI/TUI-backend | implemented-and-verified | `src/services/edit.rs` unit tests; `tests/mcp_executable.rs::editing_full_matrix_through_real_binary` (stdio MCP); `tests/cli_fs_edit.rs` (29, real binary) | Nothing structural; parity below |
| ExpectedState conflict (hash/size/lines/context, missing-vs-existing) | `FileState::check`/`ExpectedState` in edit model; enforced pre-snapshot in EditService | CLI + service (MCP tool schemas do NOT expose expected-state args — documented interface difference) | implemented-and-verified (CLI+service); **not exposed by MCP tool schema** | edit.rs unit conflict table; `cli_fs_edit.rs::fs_expected_state_conflict_blocks_stale_overwrite` (exit 4, bytes intact) | Malformed/half-wrong ExpectedState negative matrix (wrong size, wrong lines, wrong context, missing-vs-existing) is unit-level only; conflict must also be shown to leave **no snapshot/provenance residue** |
| Atomic mutation | `FilesService::write_atomic` (same-dir temp+fsync+rename) + `StoreLock` | all | implemented-and-verified | files.rs unit tests; rollback_edits reverse-order restoration tests | Documented final-component TOCTOU limitation (files.rs header) — must be reported, not claimed impossible |
| Snapshot/provenance | `services::snapshot::SnapshotStore` (.agent/snapshots, .agent/snapshot-contents/<snap>/entry-NNNNNN.bin, .agent/provenance/<edit_id>.json; SHA-256 verify on load/create) | service (CLI/MCP ride it) | implemented-and-verified | snapshot.rs unit tests (round-trip, forged id, missing material, per-snapshot blob dirs) | **Corrupted recovery blob at the CLI/MCP plane** (rollback must fail closed and preserve current bytes) untested end-to-end |
| Rollback (by exact EditId, produced-state guard, mixed-state retry, idempotence) | `EditService::rollback_edit` (provenance→manifest binding incl. workspace binding → authorize → triage → restore → correlate+audit) | MCP/CLI | implemented-and-verified | mcp_executable rollback cases; cli_fs_edit rollback/idempotent/conflict | **Foreign-workspace recovery material** (copy `.agent` into a differently-initialized workspace → rollback must fail "bound to a different workspace") untested; restart-after-rollback re-verify untested in a fresh MCP process |
| Audit (persistent AWE-013) | `services::audit::AuditLog` (.agent/audit/audit.log JSONL, monotonic sequence, rotation; ring mirror; correlation for_edit; redaction at choke point) | MCP/API (init_global) | implemented-and-verified for MCP/API planes | audit.rs unit tests; Phase-10 redaction tests | **`awh fs` CLI never calls `init_global`** → CLI edit/rollback/conflict events land only in the process-local ring and are lost on exit → **test-harness-visible gap** (durable-audit contract violated for the CLI plane). Fix in this prompt (one line, mirrors serve arms) + regression test. Also: edit_id correlation readback from audit.log untested |
| MCP/CLI/service parity | all ride `EditService` | MCP/CLI/API | partial | P15 `tests/store_convergence.rs` proved convergence for memory/tasks/files write paths, **not** for the edit plane | One parity suite: identical Replace success + stale-conflict + rollback across EditService (in-proc), CLI (real binary), MCP (real binary stdio) with byte-identical outcomes |
| Worktree isolation | **not-implemented** (Prompt 13 roadmap: Session/Worktree design not started; no worktree code exists) | — | not-implemented | — | Per §18: report **unproven**; no fake tests. Session isolation (the implemented half) is verified by mcp_agent_routes |
| Session isolation | `AgentSessionRecord` lifecycle + route binding | MCP | implemented-and-verified | mcp_agent_routes sessions_never_migrate / wrong-session / workspace mismatch | — |
| Restart persistence | provenance/snapshot/history under `.agent/`; audit.log | CLI/MCP | implemented-and-verified (CLI) / **implemented-but-unverified (MCP)** | `cli_fs_edit.rs::fs_state_survives_process_restart` (verify+history+rollback in a fresh process) | No MCP-plane restart test: server A commits, server B (fresh process, same workspace) rolls back |
| Concurrency (same-file competing ExpectedState; different files) | `StoreLock` + pre-commit ExpectedState validation in EditService | service | **implemented-but-unverified** | Only init/session-creation races tested elsewhere | No competing-writers edit test with barriers + bounded joins: must serialize-or-conflict with no corruption, exactly one committed result |
| Exact-byte matrix | EditService/FilesService byte semantics | CLI/MCP | partial | `fs_replace_exact_bytes_and_unicode` covers ASCII+Devanagari; mcp_executable covers LF cases | Emoji, CRLF, mixed newline, no-final-newline, empty, **zero-byte**, binary-like bytes, and oversized (>8 MiB cap) rejection — each with exact-byte readback AND rollback round-trip |
| Interface bypass / duplicate engines (§30) | P15 store convergence | all | implemented-and-verified | `tests/store_convergence.rs` + architecture tests | Add edit-plane pin: MCP `filesystem.*` arms must construct `EditTransaction` and call `EditService` (no local mutation) — already enforced by review; no new grep tests |
| MCP transport/auth | `mcp::auth` bearer; SSE session ids; protocol hardening (P12/15) | MCP | implemented-and-verified | mcp_protocol (80+), mcp_http, sec_002 TLS | — |
| Control API | `/api/v1` bearer + rate limit + sanitized errors | API | implemented-and-verified | api tests in lib (control router oneshot), rate-limit tests | No edit-plane routes exist on Control API (documented interface surface; not a gap) |

## Platform limitations (recorded, per §33)

- Final-component symlink swap between resolve and rename is a documented TOCTOU limitation of `FilesService` (header comment); tests must not claim universal race impossibility.
- Windows symlink planting requires privileges; existing `symlink_escape_is_rejected` is `#[cfg(unix)]`; my new tests mirror that.
- True mid-stage crash injection (kill between snapshot publication and commit) has no deterministic production seam; restart tests cover process boundaries (before/after commit, after rollback) and are labeled as such — not as crash-safety proofs.

## Planned new evidence (tests/acceptance_editing.rs + one smallest-boundary fix)

1. `parity_replace_conflict_rollback_across_service_cli_and_mcp` — identical op through EditService, `awh fs` binary, MCP stdio binary; byte-identical success bytes; stale-hash replace: service Err(conflict)/CLI exit 4/MCP error, no mutation on any plane.
2. `exact_byte_matrix_round_trip` — emoji/CRLF/mixed/no-final-newline/empty/zero-byte/binary-like/oversized through the real CLI with **byte** readback + rollback to original bytes.
3. `mcp_restart_before_and_after_rollback` — server A commits; fresh server B rolls back; repeat rollback in server C shows `already_rolled_back` (durable provenance, principal fallback for global route).
4. `trust_denial_leaves_edit_plane_untouched` — empty `awh.builtin` trust → `filesystem.replace`/`rollback` denied; bytes, snapshot/provenance dirs, and audit (deny events) pinned; positive control with full trust.
5. `capability_gate_zero_side_effect_for_edit_tools` — agent route without `Filesystem` grant → `filesystem.replace` denied, no mutation; with grant → committed (TW-003 edit-plane evidence).
6. `concurrent_competing_writers_serialize_or_conflict` — 2 threads, same base ExpectedState, barrier start, bounded join: exactly one commit, one conflict, winner's bytes, one snapshot manifest.
7. `concurrent_independent_files_progress` — 2 threads, 2 files, barrier: both commit, both snapshots.
8. `rollback_fails_closed_on_corrupted_recovery_blob` — CLI: tamper `entry-000000.bin`; rollback exits nonzero, current bytes preserved, provenance outcome untouched.
9. `copied_recovery_material_is_rejected_across_workspaces` — copy `.agent` into a freshly-initialized different workspace; rollback fails "different workspace", file untouched.
10. `symlink_escape_rejected_by_edit_plane` (unix) — CLI `fs replace` + MCP `filesystem.replace` on in-root symlink → rejected; outside target unchanged; symlinked-parent variant.
11. `cli_edit_events_are_durably_audited` — regression for the found defect: `awh fs` CLI must init the persistent audit store; assert `.agent/audit/audit.log` gains `allow`/`rollback_outcome` events correlated with the edit id, and no raw file content appears.

Defect found during mapping (§39 smallest-boundary fix): `main.rs` `Command::Fs` arm does not call `services::audit::init_global(&root)` (both `serve` arms and the Control API do) — every `awh fs` invocation silently loses its audit events. Fix mirrors the serve arms.

## After-state corrections (recorded after tests ran - the rows above are the pre-implementation map)

1. **Worktree row was accurate at write time but stale mid-prompt**: origin/rust advanced past
   the P15 branch base and now carries GIT-001 (`src/services/worktree.rs` WorktreeStore,
   `src/cli/worktree.rs`, `tests/worktree_cli.rs` x12). Worktree isolation moved from
   **not-implemented** to implemented-and-verified for lifecycle; this prompt adds only the
   edit-plane end-to-end pin (`worktree_edit_isolation_end_to_end`): an edit inside a managed
   worktree is invisible to the sibling worktree and the parent checkout, and rolls back exactly.
2. **Test-harness defect fixed (the one production change this prompt)**: the `Fs` arm of
   `src/main.rs` never called `audit::init_global`, so `awh fs` audit events stayed in the
   process-local ring and were lost on exit. The Fs arm now initializes the durable store (same
   degraded-mode discipline as the serve arms) and `cli_edit_events_are_durably_audited` pins
   correlated replace/rollback/conflict events in `.agent/audit/audit.log`.
3. **Planned-evidence de-duplication (trimmed per "existing evidence wins")**: concurrency
   items #6/#7 (competing writers, independent files) are already covered - and better, with
   barriers and deadlock-freedom - by `tests/fs_coordination.rs` (FS-001, 20 tests: competing
   writers, stale ExpectedState, symlink substitution, deadlock-freedom, lock timeouts). Not
   re-implemented here.
4. **Contract discoveries surfaced by running the suite (now pinned as-is)**:
   - The editing plane is a **text plane**: `fs replace` on a non-UTF-8 file is refused cleanly
     with bytes preserved and no residue (`exact_byte_matrix_round_trip_with_rollback`);
     binary-like but valid-UTF-8 content (NUL/control bytes) IS in-contract and round-trips
     byte-exactly.
   - Durable audit records are **checksummed envelopes** `{"checksum", "event"}`; the event
     inside carries `workspace_id` + `edit_id` correlation (readers must unwrap, not parse raw).
   - The built-in trust gate pins version `"local"` (`BUILTIN_TOOL_TRUST_VERSION`); an approval
     with any other version is a trust-level/version denial, not an allow.
   - A git worktree checkout that carries a parent's `.agent/workspace.json` is (correctly)
     refused by `awh init` foreign-root binding - worktree test fixtures must commit the base
     BEFORE `awh init` at the parent so HEAD does not embed the parent manifest. This itself
     is the TW-001 copied-directory containment working end to end.
5. **Zero-side-effect denial framing corrected** (planned #4): with NO `awh.builtin` record,
   Medium-risk `filesystem.*` retains the documented opt-in allow default (pinned as the
   compatibility control); the denial under test is a record that EXCLUDES the Filesystem
   category, plus the per-agent capability denial - both asserted with bytes, snapshot/
   provenance residue, and audit evidence, and both with positive controls.
6. **Final inventory**: `tests/acceptance_editing.rs` (8: parity x3 planes + stale conflict;
   exact-byte matrix incl. UTF-8-binary, invalid-UTF-8 and oversized rejection cells; MCP
   3-process restart; unix symlink escape x2 shapes; corrupted-blob fail-closed rollback;
   cross-workspace recovery rejection; durable correlated CLI audit; worktree edit isolation)
   and `tests/acceptance_edit_gates.rs` (3: trust-gate zero-side-effect + no-record default +
   positive control; capability zero-side-effect + scoped grant + out-of-scope; agent-correlated
   allow audit). MCP-plane trust/capability evidence rides the in-process dispatcher (the real
   transport plane is exercised by acceptance_editing.rs over the real binary).
