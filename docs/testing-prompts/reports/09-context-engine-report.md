# TP09 Report — Context Engine Verification

**Prompt:** `docs/testing-prompts/09-context-engine.md`
**Branch:** `tp09-context-engine-verification` (from `rust` @ bc46a94)
**Date:** 2026-10-04
**Suite:** `tests/context_engine_boundaries.rs` — 16 tests, plus 3 new production regression unit tests in `src/context/engine.rs` (and 3 prior unit tests updated to corrected contracts)
**Result:** 2 production defects found and fixed (§3); 1512 passed / 0 failed workspace-wide; fmt + clippy clean.

---

## 1. Methodology

Forensics first: every context-engine module was read at source —
`src/context/engine.rs` (the single entry point: window state,
insert/remove/protect/offload/restore/compress/snapshot/optimize/
select/get_context), `budget.rs` (`usable_input_tokens` = max −
reserved − margin, saturating), `tokens.rs` (whitespace-word counter,
`4 chars → 1 token` floor per word), `selector.rs` (score-ordered
greedy packing, protected always kept, rejects reported),
`scoring.rs` (relevance·priority·recency composite, deterministic),
`compressor.rs` (dup-line removal + structural extraction + bounded
truncation, `Ok(None)` when nothing saves), `offload.rs` (`OffloadStore`
— durable records under `.agent/context-engine/offloads/`),
`snapshot.rs` (items by value under `snapshots/`), `policy.rs`
(DeterministicContextPolicy), `window.rs` (temp+rename atomic
publish), `item.rs` (id validation: filename-safe charset, no
traversal), `planner.rs`. The two real interfaces were traced end to
end: the `awh context` CLI (`src/cli/context.rs` — the same
`ContextEngine::new(root, config.with_env_overrides())` construction
the MCP dispatcher uses, so semantics are identical by construction)
and the MCP `context.*` handlers (`src/mcp/dispatcher.rs:2324-2448`,
with per-tool `authorize_tool` gates and Phase-12 schema validation).

Pre-existing coverage was inventoried so this suite adds only
context-owned gaps: every module has unit tests; `tests/context_cli.rs`
(7 tests) pins CLI lifecycle basics, update-requires-existing-id,
audit-identifiers-never-content, and retrieval-never-becomes-memory-
mutation. **No test anywhere drove the engine across a process
restart with a raw-file oracle, drove the MCP `context.*` tools over
the real stdio wire, exercised corruption of window/offload/snapshot
bytes, probed the insert budget gate with the CALLER-SUPPLIED item
shape (token_count = 0) both real callers use, or checked whether a
restored item's durable offload record is retired.** Those are
exactly the gaps this suite owns.

Every test drives the REAL boundary: the compiled `awh` binary
across separate processes (CLI plane and MCP stdio plane — real
JSON-RPC on stdin, real framing, real audit), the canonical
`ContextEngine` in-process over real `.agent/context-engine/` state
(labeled engine-coverage, the same object the CLI/MCP construct),
and raw persisted bytes as the independent oracle. No second engine,
window, offload store, snapshot store, or token counter is
reimplemented; no mocks; environment overrides are passed per-child
in isolated `Command`s, never by mutating process env.

## 2. Established implementation map (forensics)

| Concern | Owner (source of truth) |
| --- | --- |
| Engine | `src/context/engine.rs` `ContextEngine` — single entry point, synchronous, transport-agnostic |
| Config/env | `ContextEngineConfig::with_env_overrides` — `AWH_CONTEXT_ENABLED` (1/true/yes/on), `AWH_CONTEXT_MAX_INPUT_TOKENS`, `AWH_CONTEXT_RESERVED_OUTPUT_TOKENS`, `AWH_CONTEXT_SAFETY_MARGIN_TOKENS` (fail-closed parse), `AWH_CONTEXT_AUTO_OFFLOAD/AUTO_COMPRESS/MEMORY_ENABLED`; malformed booleans → fail-safe (disabled) |
| Budget | `budget.rs` — usable = max − reserved − margin (saturating); insert hard cap = `max_input_tokens`; selection/assemble cap = usable (tightened further by a request `token_budget`) |
| Window | `window.rs` `WindowStore` — `.agent/context-engine/active.json`, schema_version=1, items+protected, temp+rename atomic publish, corrupt → `corrupt context window` fail-closed, missing → empty |
| Offload | `offload.rs` `OffloadStore` — `offloads/<id>.json` durable records; `list_ids` from directory metadata; `remove_restored` is the exclusive deletion path (after restore, after explicit removal — D1) |
| Snapshots | `snapshot.rs` — items by value in `snapshots/<id>.json`; restore fits budget, over-budget ids reported as `skipped`, invalid ids resolve `Ok(None)` (no fs touch) |
| Compression | `compressor.rs` — deterministic local transforms; metadata `context_engine.original_tokens/strategies`; idempotent (`Ok(None)` second pass) |
| Audit | every mutation surfaces `cli_context_*` / MCP `tool_invoke` with identifiers/outcomes only — content never reaches the durable audit log |
| CLI | `awh context show/save/update/clear/search` — same engine construction as MCP (identical semantics) |
| MCP | `context.status/get/insert/remove/search/optimize/assemble/protect/unprotect/offload/restore` — Medium-risk tools behind `authorize_tool`, Phase-12 schema validation (`-32602`), unknown get → JSON null |
| Token counter | `tokens.rs` — whitespace-word count with per-word floor; content-derived counts are the only truth (D2) |

## 3. Defects found

### D1 — `restore` left a ghost offload record; `remove` + `restore` resurrected removed content (HIGH, fixed)

**Probe** (real binary, MCP stdio, before fix): `context.insert` →
`context.offload` → durable record written (`offloads/probe-item.json`,
status `offloaded_items: 1`) → `context.restore` **in the same
process** → item active, but the raw record was still on disk; a
fresh server process still reported `offloaded_items: 1` for an
active item; then `context.remove {"id"}` returned `removed: true`,
`context.get` returned null — **and `context.restore` resurrected
the full removed content** from the stale record.

**Root cause:** `Engine::restore`'s in-memory branch returned
`restore_item_to_active(item)` without calling
`offloads.remove_restored()` — only the durable-store fallback branch
did. `remove_item` never touched the offload store at all, so an
explicit removal also leaked its record.

**Impact:** (a) explicit removal was not durable — deleted content
came back through the documented-recoverable restore path
(data-lifecycle violation, §35-adjacent); (b) `context.status`
`offloaded_items` counted ghosts forever, including across restarts;
(c) snapshots could double-count a restored item (active in window +
still listed in `offloaded_item_ids`).

**Fix:** `Engine::restore` now retires the durable record in the
in-memory branch (after the item is durably active — the
"never unrecoverable window" ordering is preserved: persist first,
delete record second); `Engine::remove_item` now retires any offload
record for the removed id (removal is the engine's explicit discard);
`OffloadStore::remove_restored` doc updated to name the two
legitimate callers. One existing unit test
(`offload_then_restore_round_trips_content`) had pinned the ghost as
a "never a window where content is unrecoverable" property — the
property is preserved by ordering (persist-active THEN delete), so
the test now asserts the durable-active window bytes and the retired
record instead of the lingering record.

**Regressions:** `restore_from_memory_retires_the_durable_record`,
`remove_item_retires_offload_records_and_never_resurrects` (engine),
plus wire-level assertions inside
`mcp_context_lifecycle_status_insert_get_search_offload_restore_restart`
(record gone after same-process restore; status counts 0; remove →
restore errors; restart sees no ghosts).

### D2 — the insert budget gate checked the CALLER-SUPPLIED token_count, before recounting (HIGH, fixed)

**Probe** (real binary, CLI): `AWH_CONTEXT_MAX_INPUT_TOKENS=50 awh
context save --id big --content <80 words>` → **accepted, exit 0**,
"saved context item big (80 tokens, scope Project)" — the budget cap
was dead code at both real interfaces.

**Root cause:** `Engine::insert` gated on `item.token_count > max`
BEFORE the defensive recount (`item.token_count =
counter.count(&item.content)`). Both real callers (CLI
`src/cli/context.rs`, MCP dispatcher) construct items with
`token_count: 0`, so the gate always passed and the item was stored
with its true (over-budget) count. The gate only ever fired for
callers that pre-set a truthful count — i.e., unit tests.

**Impact:** budget enforcement was caller-optional on the two
production paths; §9's "engine never exceeds the effective input
budget" contract was unenforced at insert time; a caller could push
unbounded content into the window (bounded only by disk).

**Fix:** the recount now happens FIRST and the gate checks the
recounted value. The hard insert cap remains `max_input_tokens`;
`usable_input_tokens` (max − reserved − margin) is still enforced by
selection/assemble/snapshot-restore (rejected/skipped reporting) —
the two-layer design is intact, the insert layer just stopped
trusting caller input.

**Test-contract updates (pinning corrected behavior, not weakened
assertions):** `insert_rejects_invalid_ids_and_oversized` now feeds
oversized CONTENT (recounted) instead of an impossible
item-state (token_count=999_999 with tiny content — a state the
engine's own invariant forbids); `get_context_respects_small_budgets`
and `snapshot_restore_skips_items_over_budget` keep their original
selection/restore intent but with fixtures that pass the insert cap
and exceed the usable budget (max 250/reserved 100 → usable 150;
max 100/reserved 50 → usable 50), since their old fixtures silently
leaned on the D2 hole to inject over-cap items.

**Regressions:** `budget_gate_uses_recounted_tokens_not_caller_count`
(engine — CLI/MCP-shaped item with token_count=0 and oversized
content is rejected; a lying count on small content is normalized to
the true count) and `cli_budget_boundary_rejects_oversized_items_
cleanly` (black-box: exit non-zero, deterministic error text,
nothing persisted for the rejected id, prior items untouched).

### Non-defects verified live (probe-confirmed correct)

- Corrupt offload record with the id still in the window: restore
  succeeds from the window (the window is the authoritative loaded
  view) and the corrupt bytes are then deleted by the D1 fix.
- `inspect_snapshot` on invalid ids resolves `Ok(None)` (not an
  error, no fs touch) — asserted as the contract, not "fixed".
- `context.get` of an unknown id returns JSON `null`, not an error.
- Empty/whitespace search query → deterministic empty array.
- Malformed `AWH_CONTEXT_ENABLED=banana` → fail-safe (disabled),
  deterministic error text on every op.

## 4. Test-by-test evidence (tests/context_engine_boundaries.rs, 16)

**CLI black-box (real binary, fresh process per invocation):**

1. `cli_restart_persistence_scope_metadata_and_exact_bytes` — 3 items
   (Session/Project/Global scopes, full-unicode content) saved across
   separate processes; raw `active.json` oracle (schema_version=1,
   per-item scope/content verbatim); fresh-process `show`/`show --id`
   returns exact bytes; status observes 3. **Passed** (§7, §23, §28).
2. `cli_budget_boundary_rejects_oversized_items_cleanly` — D2
   black-box regression: 80-token item under a 50-token env cap
   fails closed, deterministic text, rejected id never persisted,
   earlier item untouched. **Passed** (post-fix; failed pre-fix) (§9).
3. `cli_disabled_engine_fails_closed_and_never_mutates_state` —
   save/search both refuse with `context engine is disabled`;
   malformed boolean fails safe; pre-existing window bytes identical
   after disabled runs; re-enable shows intact state. **Passed** (§6).
4. `cli_duplicate_save_replaces_and_clear_targets_only_named_items` —
   duplicate-id save = explicit upsert (exactly one row, no
   duplicates); `clear --id` removes exactly the named item,
   neighbor byte-identical; unknown-id clear fails with no state
   change. **Passed** (§7).
5. `cli_corrupt_window_fails_closed_across_processes` — truncated
   window: every op in a fresh process exits non-zero with
   `corrupt context window`; corrupt bytes surfaced, never silently
   rewritten; deleting the file restores an empty writable engine. 
   **Passed** (§24).
6. `cli_canary_content_stored_but_never_audited` — token-shaped
   canary content: stored in the engine's own store (by design) and
   returned by search (by design), but absent from every durable
   audit byte while the save event and the item identifier are
   present. **Passed** (§22, §27).
7. `cli_workspace_isolation_physical_boundary` — two roots, same
   item id, different content: each `show`/`search` sees only its
   own; raw bytes agree per root. **Passed** (§8).

**Engine integration (canonical ContextEngine over real state):**

8. `selection_and_assembly_are_deterministic_and_budget_bounded` —
   identical fixture/config → identical `select` outcome twice;
   ties break by id; `get_context` total_tokens ≤ usable and ≤ each
   item; explicit request budget only tightens; assemble leaves the
   item set byte-identical. **Passed** (§10, §17).
9. `protected_items_survive_optimize_and_offload_semantics` —
   protect → always Keep through optimize (kept, never
   offloaded/archived, still active); repeated optimize stable;
   explicit offload of a protected item refused; offloaded item's
   raw record verified on disk and restored byte-exact, record
   retired (D1). **Passed** (§11, §16).
10. `offload_restore_persist_and_survive_engine_restart` — offload →
    fresh engine instance: Offloaded state + byte-exact content +
    status 1; search finds offloaded content without mutating state;
    restore from fresh instance exact, no ghost count (D1); an
    orphan corrupt record (id not in window) fails closed while a
    healthy sibling record restores fine. **Passed** (§13, §23, §24).
11. `snapshot_lifecycle_budget_limited_restore_and_isolation` —
    snapshot (task/session metadata in raw file) → mutate window →
    restore brings items back active; a 900-token item (≤1000 max,
    >800 usable) is reported `skipped`, never force-restored;
    cross-root: another engine never sees the snapshot id; corrupt
    snapshot file → inspect fails closed, window untouched; delete
    works; invalid ids → `Ok(None)` no-fs-touch. **Passed** (§14).
12. `compression_is_deterministic_lossless_record_and_idempotent` —
    duplicate-heavy content compresses; metadata records
    `original_tokens` + `strategies` incl. `dedup_lines`; second pass
    `Ok(None)`; incompressible content left untouched. **Passed** (§12).
13. `search_limits_scopes_and_immutability` — limit enforced (6
    matches → 2 with limit 2); case-insensitive; empty/whitespace →
    empty; injection-shaped queries are inert data; search is
    read-only (item set + bytes unchanged). **Passed** (§15).
14. `concurrent_inserts_and_duplicate_id_races_keep_state_valid` —
    8 threads × 10 inserts + interleaved searches: all 80 land, no
    panics; duplicate-id two-thread race leaves exactly one row and
    a valid 81-id window. **Passed** (§25).

**MCP stdio black-box (real JSON-RPC over stdin):**

15. `mcp_context_lifecycle_status_insert_get_search_offload_restore_
    restart` — full lifecycle on the wire: status(0) → insert
    (Session scope, unicode) → get exact bytes → search hit →
    protect → optimize deterministic (kept repeated) → assemble
    budget-bounded (usable computed independently from the three
    serialized budget fields) and non-mutating → offload (raw record
    on disk, status 1) → restore (D1: record retired, status 0) →
    remove → restore ERRORS (never resurrects) → get null → fresh
    server process over the same root: state persisted, exact bytes,
    no ghosts. **Passed** (post-D1-fix; the resurrection step failed
    pre-fix) (§28, §29).
16. `mcp_context_schema_validation_and_unknown_ids` — invalid scope
    string / missing required field / wrong id type → `-32602`;
    traversal id rejected; unknown get → JSON null; empty query →
    empty array over the wire. **Passed** (§29).

## 5. Classification (per prompt evidence classes)

| Requirement area | Class | Evidence |
| --- | --- | --- |
| §6 enable/disable + env overrides (incl. malformed values) | **Passed** | tests 2, 3; `with_overrides_from` unit coverage |
| §7 item model + validation + upsert | **Passed** | tests 1, 4, 11 (invalid ids), 16 |
| §8 workspace/identity isolation | **Passed** | tests 7, 11 (cross-root snapshot) |
| §9 token accounting + budget boundary | **Passed** | D2 fixed; tests 2, 8, 15 + engine regression test |
| §10 selection determinism/scoring | **Passed** | test 8 (unit coverage pre-existing) |
| §11 protected items | **Passed** | test 9 |
| §12 compression (deterministic, bounded, metadata) | **Passed** | test 12 |
| §13 offload/restore (durable, never delete, restore exact) | **Passed** | D1 fixed; tests 9, 10, 15 |
| §14 snapshots (budget-limited restore, isolation, corruption) | **Passed** | test 11 |
| §15 search (limits, empty query, read-only) | **Passed** | test 13, 16 |
| §16 policy (deterministic, Phase 1) | **Passed** | test 9 (optimize decisions) + policy.rs unit tests |
| §17 assemble invariants (budget, determinism, non-mutating) | **Passed** | tests 8, 15 |
| §22/§27 audit identifiers-only, no content leak | **Passed** | test 6 (+ pre-existing context_cli pin) |
| §23 persistence/restart (window, offloads, snapshots) | **Passed** | tests 1, 10, 11, 15 |
| §24 stale/corrupt state (window, offload, snapshot bytes) | **Passed** | tests 5, 10, 11 |
| §25 concurrency | **Passed** | test 14 |
| §26/§28 CLI black-box parity with MCP | **Passed** | tests 1-7 vs 15-16; same construction verified at source |
| §29 MCP tool surface (schema, auth gates, null-get) | **Passed** | tests 15, 16 |
| §35 no cross-workspace restoration | **Passed** | D1 resurrection eliminated; test 11 cross-root; test 7 isolation |
| Not implemented (Phase 2/3 policies: ExternalModelPolicy, LearnedPolicy) | **Not implemented (by design)** | policy.rs Phase-1 only; documented |

No **Blocked** or **Unproven** items remain in the verified scope.

## 6. Gates

- `cargo fmt` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — **1512 passed / 0 failed** across all 43
  test binaries (1493 pre-TP09 baseline + 16 new boundary tests + 3 new
  engine regression tests; 1 prior engine test rewritten in place to pin
  the corrected D1 contract, 1 updated for D2's recount-first gate, and
  2 prior unit fixtures adjusted to stop leaning on the D2 hole — every
  adjusted test still asserts its original intent).
- Environment hygiene note from this round: a shell-exported
  `AWH_CONTEXT_MAX_INPUT_TOKENS` (used while probing D2 manually)
  leaked into a later `cargo test` invocation through the persistent
  terminal and made the MCP child report a 50-token budget — the
  test's independent usable-budget oracle caught it immediately.
  Test env vars must be child-scoped (`Command::env`), never
  shell-exported, in this workspace.

## 7. No theater checklist (§31)

- Every CLI/MCP test drives the compiled `awh` binary; no in-process
  CLI fakes, no re-implemented arg parsing.
- The MCP tests speak real JSON-RPC on the real stdio transport and
  parse the real `content[0].text` envelope.
- Engine-integration tests use the canonical `ContextEngine` — the
  same object the CLI and dispatcher construct — never a parallel
  implementation.
- Raw persisted bytes (`active.json`, `offloads/<id>.json`,
  `snapshots/<id>.json`, `audit.log`) are the oracle; no test trusts
  a getter to verify the same getter's write.
- The token budget oracle in the MCP test is computed independently
  from the three serialized budget fields (max − reserved − margin),
  not from a `usable_input_tokens` echo.
- Both defects were reproduced by REAL probes against the compiled
  binary BEFORE the fix (D1 via MCP stdio probe, D2 via CLI probe)
  and re-verified gone AFTER the fix at the same boundaries.
- No test was written to pass by mirroring an implementation detail:
  the corrupt-offload case documents the window-as-authoritative-view
  behavior as a contract decision (record-only corruption fails
  closed), and `Ok(None)` for invalid snapshot ids was classified as
  intended behavior from source, not "fixed" to an error.
