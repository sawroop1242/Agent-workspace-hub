# TP10 Report — Developer Memory Verification

**Prompt:** `docs/testing-prompts/10-developer-memory.md`
**Branch:** `tp10-developer-memory-verification` (from `rust` @ bc5d319)
**Date:** 2026-10-07
**Suite:** `tests/memory_boundaries.rs` — 14 tests, plus 1 new production unit test in `src/core/memory.rs`
**Result:** 1 production defect found and fixed (§3); 1527 passed / 0 failed workspace-wide; fmt + clippy clean.

---

## 1. Methodology

Forensics first: the canonical store was read at source —
`src/core/memory.rs` (`MemoryStore`: `.agent/memory.json`, `store`
upsert, `update_existing` full-replace, `append`/`append_scoped` id
minting, `update_partial` partial update, `list_all`/`read_all`,
`search`, `get`, `delete`, `migrate_legacy_jsonl`,
`validate_memory_input`, the five `MAX_MEMORY_*` constants, and the
`StoreLock`-guarded load-modify-save with temp+rename atomic publish).
Every adapter was traced at its real boundary: the CLI
(`src/cli/memory.rs`, `src/main.rs` Memory arm with `init_global`), the
MCP dispatcher arms + schemas (`src/mcp/dispatcher.rs:2063-2125`,
`tool_registry.rs` risk levels, `execution_gate.rs` gating), the Control
API (`src/api/control.rs` `GET/POST /api/v1/memory`), the TUI backend
(`src/tui/backend.rs` `list_memory`/`append_memory`), and the Context
Engine's memory field (`src/context/engine.rs:198,896` — status
`memories` count; `planner.rs:207` constructs the same store). The
MCP module `src/mcp/memory.rs` is a 9-line re-export, not a second store.

Pre-existing coverage was inventoried so this suite adds only
memory-owned gaps: `src/core/memory.rs` unit tests (validation bounds,
entry-count limit, migration determinism/idempotency/blank-lines/
corrupt-line/merge, `update_partial` field semantics, project isolation,
id-minting concurrency, corruption fail-closed), `tests/memory_cli.rs`
(4 CLI lifecycle tests), `tests/store_convergence.rs` (3 cross-plane
convergence tests). **No test anywhere drove the CLI/MCP planes across
fresh processes with a raw-bytes oracle, probed the MCP `memory.update`
partial-vs-full semantics, exercised cross-process concurrent writers
against the store lock, pinned the migration replay-id-collision
contract, verified content never reaches the durable audit, or proved
the Context Engine's memory view is the canonical store and not a
second one.**

Every test drives the REAL boundary: the compiled `awh` binary across
separate processes (CLI plane and MCP stdio plane — real JSON-RPC), the
canonical `MemoryStore` in-process over real `.agent/memory.json` state,
the real Control API router via `oneshot`, and raw persisted bytes as
the independent oracle. No second store, lock, migration, search, or
authorization/audit implementation is built here.

## 2. Established implementation map (forensics)

| Concern | Owner (source of truth) |
| --- | --- |
| Canonical store | `core::memory::MemoryStore` — `.agent/memory.json`, `{entries:[…]}` |
| Record | `MemoryEntry` — id/scope/content/tags/created_at/updated_at |
| Scope | `MemoryScope` Session/Project/Global — a FIELD on the entry, never a second file |
| Limits | `MAX_MEMORY_ENTRIES`=10_000, `MAX_MEMORY_CONTENT_BYTES`=1 MiB, `MAX_MEMORY_ID_LEN`=256, `MAX_MEMORY_TAGS`=64, `MAX_MEMORY_TAG_LEN`=128 |
| Mutations | `store` (upsert), `update_existing` (full replace), `update_partial` (partial), `append`/`append_scoped` (mint id), `delete` |
| Atomicity | `StoreLock` over load-modify-save; `NamedTempFile` + `persist` rename; `sync_all` |
| Migration | `migrate_legacy_jsonl` — `.agent/memory.jsonl` → index-keyed `legacy-jsonl-<n>` ids, blank lines skipped, archived to `.migrated`, fail-closed on a malformed line |
| CLI | `awh memory list/get/search/add/update/delete` — `init_global` for durable audit; `bound_limit` clamps output 1..=500 |
| MCP | `memory.store/search/get/delete/update` — Medium-risk mutations gated by `authorize_tool`; Low-risk reads ungated; Phase-12 schema `-32602` |
| Control API | `GET/POST /api/v1/memory` — bearer auth; `append` (Project scope); audited `api_memory_append` |
| TUI | `list_memory`/`append_memory` — `append` (Project); audited `tui_memory_append` |
| Context | `ContextEngine.memory` = `MemoryStore`; status `memories` count via `search("",None).len()`; planner searches the same store |
| Audit | CLI mutations `cli_memory_*` (id subject — redacted when token-shaped; scope detail); reads not audited (documented) |

## 3. Defect found

### D1 — MCP `memory.update` was a silent full replace: a content-only update reset scope to Project and wiped tags (HIGH, fixed)

**Probe** (real binary, MCP stdio, before fix): `memory.store {id:"probe-1",
content:"global note", scope:"Global", tags:["alpha","beta"]}` →
`memory.update {id:"probe-1", content:"updated content only"}` (a call
the schema itself documents as valid: `required:["id","content"]`).
Observed result: `{"content":"updated content only", "scope":"Project",
"tags":[]}` — the entry was **silently downgraded Global→Project and its
tags were erased**. The raw `.agent/memory.json` agreed.

**Root cause:** the dispatcher arm called
`self.memory.update_existing(id, content, parse_scope(scope_opt), strings(tags))`.
`parse_scope(None)` defaults to `Project` and `strings(absent)` yields
`vec![]`; `update_existing` is FULL-REPLACE semantics (it mirrors the
`memory.store` upsert). So any `scope`/`tags` field the caller omitted was
replaced with the default rather than left untouched — the opposite of the
CLI's `awh memory update`, which uses `update_partial` and preserves both.

**Impact:** (a) same-named operation with divergent semantics across two
real interfaces (violates "adapters do not create alternate CRUD
semantics"); (b) silent scope mutation without a request — a `Global`
note becomes `Project`-visible-only, moving visibility the caller never
asked to change; (c) silent tag loss; (d) the API is the agent-tool plane,
so this is the surface agents hit most.

**Fix:** the `memory.update` arm now parses `scope`/`tags` as OPTIONAL and
calls `self.memory.update_partial(&id, Some(content), scope_opt, tags_opt)`
— identical semantics to the CLI. `content` stays schema-required.
Verified live post-fix: the same content-only call now returns
`scope:"Global", tags:["alpha","beta"]`, and an explicit
`scope`/`tags` call still replaces both.

**Regression:** `mcp_update_is_partial_and_matches_cli_semantics`
(black-box MCP stdio) — asserts scope/tags/created_at preserved on a
content-only update, replaced when present, and CLI/MCP field parity.

### Non-defects verified live (probe-confirmed correct)

- `search ""` matches everything (substring `contains("")`); the CLI's
  `--query ""` therefore lists all — pinned as the current contract.
- Search case folding is ASCII-only (`to_ascii_lowercase`); non-ASCII is
  compared verbatim — pinned.
- Memory ids are token-shaped (`mem-<hex>` ≥16 chars) and go through the
  audit redaction choke point → the durable subject is `[redacted]`
  (same documented residual as policy-rule ids); the scope detail and
  action are intact. Classified as correct fail-closed behavior, not a
  defect, and pinned.
- Migration replay with a re-created legacy file whose records collide
  with already-migrated index ids silently SKIPS those records — this is
  the crash-recovery idempotency contract (the same-file re-run must not
  duplicate), so it is correct-by-design; new records at new indexes
  still migrate. Pinned both halves.

## 4. Test-by-test evidence (tests/memory_boundaries.rs, 14)

**CLI black-box (real binary, fresh process per invocation):**

1. `cli_crud_restart_parity_with_raw_file_oracle` — stdin content add,
   unicode/multiline round-trip, raw-file oracle (scope/tags/timestamps
   verbatim, ids are data not paths), fresh-process get/list, scope
   filter, content-only update preserving scope/tags/created_at and
   advancing updated_at, delete-exactly-one, repeated-delete errors.
   **Passed** (§8, §10, §15, §24).
2. `cli_list_limit_clamping_and_bounded_output` — limit 0→1 clamp, huge
   limit shows all, over-max clamps to 500, giant query no-match,
   multiline flattened. **Passed** (§10, §21).
3. `search_semantics_limits_determinism_and_read_only` — content/tag
   case-insensitive match, substring, limit, scope narrows-not-broadens,
   empty-query contract, repeated-search determinism, Unicode exact
   match, search never mutates, missing/path-like id clean error.
   **Passed** (§9, §8).
4. `cli_validation_bounds_fail_closed_without_partial_publish` —
   invalid/lowercase scope rejected, overlong tag rejected, at-limit tag
   accepted, duplicate tags kept, oversized content rejected, nothing
   partially published. **Passed** (§6, §21).
5. `workspace_isolation_identical_ids_and_root_relocation` — two roots,
   generated + explicit identical ids isolated, raw bytes differ, root
   relocation keeps memory readable. **Passed** (§7).
6. `legacy_migration_through_real_cli_process` — migration on first CLI
   touch, stable index ids, blank-line skip, timestamp preservation,
   archive-not-delete, restart idempotency, replay collision contract,
   corrupt-line fail-closed (no fabrication, no archive).
   **Passed** (§14, §22).
7. `audit_records_mutations_without_leaking_content` — token-shaped
   canary stored in the canonical file (allowed), audit carries the
   action + redacted subject + scope, never the content; update/delete
   audited; failed op leaves no false-positive success. **Passed**
   (§19, §20).

**MCP stdio black-box (real JSON-RPC on stdin):**

8. `mcp_update_is_partial_and_matches_cli_semantics` — **D1 regression**:
   content-only update preserves scope/tags/created_at; explicit
   scope/tags replace; CLI parity. **Passed** (post-fix; failed pre-fix).
9. `mcp_validation_isolation_and_no_mutation_on_error` — cross-workspace
   read is null; invalid scope/missing id/wrong type/missing content →
   `-32602`; failed validation never mutates; missing-id update errors;
   delete of missing id → `deleted:false`. **Passed** (§11, §16).

**Corruption, concurrency, integration:**

10. `corrupt_store_fails_closed_and_never_leaks_content` — truncated JSON
    fails every op and is never rewritten; unknown fields tolerated;
    duplicate raw ids → first-match get + both listed; directory-as-store
    fails closed. **Passed** (§17, §26).
11. `concurrent_cross_process_writers_keep_store_valid` — 8 processes ×
    5 adds over the real lock → 41 valid unique entries, no torn writes.
    **Passed** (§16).
12. `concurrent_update_vs_delete_same_id_is_atomic` — update/delete race
    never duplicates identity, store stays well-formed. **Passed** (§16).
13. `context_engine_shares_the_canonical_memory_authority` — engine
    status memory count tracks CLI writes, no second store under the
    context-engine dir, survives restart. **Passed** (§12).
14. `control_api_memory_delegates_to_the_canonical_store` — API append
    visible to CLI + raw file, CLI write visible to API list, identical
    validation (empty → 400, no write), auth enforced. **Passed** (§13).

Plus 1 production unit test in `src/core/memory.rs`:
`updates_remain_possible_at_the_entry_count_limit` — a full store still
allows updates to existing ids while refusing new ones (§21).

## 5. Classification (per prompt evidence classes)

| Requirement area | Class | Evidence |
| --- | --- | --- |
| §6 record validation + limits | **Passed** | tests 4, 10; unit bounds tests |
| §7 scope/workspace isolation | **Passed** | tests 5, 13; store_convergence |
| §8 CRUD semantics/invariants | **Passed** | tests 1, 9; D1 fixed |
| §9 search correctness/determinism | **Passed** | test 3 |
| §10 CLI black-box | **Passed** | tests 1-4, 6 |
| §11 MCP boundary | **Passed** | tests 8, 9 (D1 fixed) |
| §12 Context integration (shared authority) | **Passed** | test 13 |
| §13 Control API | **Passed** | test 14 |
| §13 TUI | **Passed (delegation)** | `tui/backend.rs` uses the same store; not separately re-tested (would duplicate the TUI suite) |
| §14 legacy JSONL migration | **Passed** | test 6; unit migration tests |
| §15 canonical persistence | **Passed** | tests 1, 6; raw-file oracle |
| §16 atomicity/locking/crash recovery | **Passed** | tests 11, 12 |
| §17 corruption/fail-closed | **Passed** | test 10 |
| §18 authorization/identity boundaries | **Passed** | test 9 (cross-workspace), test 14 (bearer); MCP gates pre-existing |
| §19 audit integration | **Passed** | test 7 |
| §20 sensitive-data protection | **Passed** | test 7 (storage allowed, logs/audit clean) |
| §21 resource exhaustion | **Passed** | tests 2, 4; unit boundary test |
| §22 replay/idempotency | **Passed** | tests 1, 6; D1 |
| §23 cross-platform | **Passed (CI)** | runs on ubuntu/macos/windows in CI; no platform-specific logic beyond the shared atomic-publish path |
| §24 property tests | **Passed** | tests 3, 5, 10-12 |

No **Blocked** or **Unproven** items remain in the verified scope. TUI is
classified as covered-by-delegation (the memory-specific delegation is
inspected at source and exercised indirectly through the shared store);
no separate TUI memory suite was written to avoid duplicating the TUI
master suite.

## 6. Gates

- `cargo fmt` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — **1527 passed / 0 failed** (1526 pre-TP10
  baseline + 14 boundary tests + 1 unit test; the D1 fix changed no
  existing test's expectation because no existing test exercised the
  content-only MCP update path).

Environment note: the Rust toolchain was wiped from the container twice
during this round; reinstalled with the AGENTS.md rustup recipe
(`--default-toolchain stable --profile minimal` + `rustup component add
rustfmt clippy`), cargo 1.99.0.

## 7. No theater checklist (§27/§31)

- Every CLI/MCP test drives the compiled `awh` binary; no in-process CLI
  fakes, no re-implemented arg parsing.
- The MCP tests speak real JSON-RPC on the real stdio transport and parse
  the real `content[0].text` envelope.
- Integration tests use the canonical `MemoryStore` / real Control API
  router — never a parallel store or route-local writer.
- Raw `.agent/memory.json` bytes (parsed independently with serde_json)
  are the oracle for every mutation; no test trusts a getter to verify the
  same getter's write.
- The D1 defect was reproduced by a REAL MCP stdio probe against the
  compiled binary BEFORE the fix and re-verified gone AFTER the fix at the
  same boundary.
- No test-only memory format, authorization service, or audit authority
  was introduced; the audit assertions read the real durable log.
- Correct-by-design behaviors (empty-query match-all, ASCII-only case
  folding, audit id redaction, migration index-collision idempotency)
  were classified honestly as contracts and pinned, not "fixed" to make a
  test pass.
