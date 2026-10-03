# TP03 Test Report — Agent-Grade Filesystem Editing

**Date:** 2026-10-02
**Prompt:** `docs/testing-prompts/03-agent-grade-filesystem-editing.md`
**Branch:** `testing-tp03-filesystem-editing`
**Base:** `rust` @ `d186abe` (post-TP01-merge, includes #138's connector fix)
**Binary under test:** real `awh` release/debug binary via `CARGO_BIN_EXE_awh`, real in-process `StdioMcpServer` (the same stdio dispatcher the CLI/MCP binary serves), real Control API router via `build_router(...).oneshot(...)`.

---

## 1. Implementation inventory (§2, §36)

| Surface | Status |
|---|---|
| `awh fs replace/insert/delete-range/patch/apply-diff/history/verify/rollback` | Implemented (CLI family, `src/cli/fs_edit.rs` over canonical `EditService`) |
| MCP `filesystem.{replace,insert,delete_range,patch,apply_diff,rollback}` | Implemented |
| MCP `workspace.read_file` / `write_file` / `list_files` / `delete_file` | Implemented |
| Control API `files`, `files/content`, `files/search`, `files/entry` | Implemented |
| `FilesService::{read, read_bytes, write, write_atomic, delete, list, search, meta}` | Implemented (canonical basic-ops service) |
| **`awh fs read`** | **Not implemented** (CLI doc: basic ops are a separate milestone; reads live at the MCP/API/service boundaries) |
| **`awh fs write`** | **Not implemented** (same; writes live at MCP/API/service boundaries) |
| **`awh fs stat`** | **Not implemented** (metadata via Control API `files/entry` and `FilesService::meta`) |
| **`awh fs search`** | **Not implemented** (search via Control API `files/search` and `FilesService::search`) |
| **`awh fs hash`** | **Not implemented** (SHA-256 hashing is exercised as the edit plane's `--expected-hash` stale guard and in `FileState`) |

Prior suites already covering the edit family (inspected, reused, not duplicated):
`tests/cli_fs_edit.rs` (15 CLI tests), `tests/acceptance_editing.rs` (8),
`tests/acceptance_edit_gates.rs` (3), `tests/fs_coordination.rs` (20),
plus `src/services/edit.rs` unit tests and `src/mcp/workspace.rs` unit tests.

## 2. New evidence suite

`tests/fs_basic_boundaries.rs` — 13 executable tests covering the
**basic-operations boundary** (the increment the prompt demands beyond the
existing edit-family suites). Every mutation is proven by reading real
filesystem bytes back with `std::fs`; every rejection is proven to leave
the tree byte-identical.

| # | Test | TP03 § | Boundary |
|---|---|---|---|
| 1 | `mcp_read_file_returns_exact_bytes_without_normalization` | 6, 19 | MCP |
| 2 | `mcp_read_file_handles_empty_large_missing_directory_and_overcap` | 6 | MCP |
| 3 | `mcp_read_is_text_only_while_service_read_bytes_is_binary_safe` | 6 | MCP + service |
| 4 | `mcp_write_file_creates_overwrites_and_is_deterministic` | 7 | MCP |
| 5 | `mcp_write_file_rejects_traversal_without_touching_the_outside_file` | 20 | MCP |
| 6 | `mcp_write_file_enforces_the_5_mib_cap_with_no_partial_file` | 7, 28–29 | MCP |
| 7 | `mcp_write_file_directory_target_fails_and_leaves_the_tree_unchanged` | 7 | MCP |
| 8 | `mcp_list_files_reports_only_files_sorted_with_sizes` | 8 | MCP |
| 9 | `service_meta_classifies_kind_reports_size_and_tracks_mutation` | 8 | Service (stat) |
| 10 | `service_search_matrix_zero_one_many_repeated_case_unicode_limit` | 9 | Service (the layer `/files/search` serves) |
| 11 | `cli_expected_hash_guard_matches_independent_published_vectors` | 10 | CLI + real binary |
| 12 | `parity_write_produces_identical_bytes_across_mcp_service_and_control_api` | 22 | MCP + service + Control API |
| 13 | `deep_nesting_and_many_files_stay_bounded_and_correct` | 29 | MCP + service |

## 3. Contract-by-contract classification

### §6 Read — **Passed**
- Exact bytes (CRLF, no-trailing-newline, multiple blank lines, tabs, Unicode
  emoji/multibyte) survive the round trip with **zero normalization** (test 1).
- Empty file → empty string, not an error/placeholder (test 2).
- 1.5 MiB file → complete content returned, **no silent truncation** (test 2).
- Missing file → documented `workspace file not found` error (test 2).
- Directory target → error `-32603` naming the failed `read` op, never a
  directory listing through the read path (test 2).
- 2 MiB+1 file → rejected with the documented `exceeds 2 MiB limit`; response
  contains no file bytes (test 2).
- Binary (invalid UTF-8) → MCP read is **text-only by contract**: clean error,
  no mojibake (`U+FFFD` never appears); the canonical service
  `read_bytes` round-trips the identical 5 bytes (test 3).

### §7 Write — **Passed**
- New file + nested missing parents auto-created; overwrite fully replaces
  (zero residue); empty content → genuinely empty file; Unicode exact; repeat
  writes deterministic (test 4).
- **Traversal** (`../`, `a/../../`, absolute path) all rejected; a sentinel
  file outside the workspace is byte-for-byte unchanged; no residue inside
  the root apart from the server's own `.agent/` state dir (test 5).
- **Cap boundary**: exactly 5 MiB accepted (the contract is `>` cap), 5 MiB+1
  rejected with `exceeds`, and the target **does not exist at all** after the
  rejected write — no partial file, since the cap check precedes any fs
  mutation (test 6).
- Directory target → error; directory still a directory; member file intact
  (test 7).

### §8 Stat/list — **Passed**
- `workspace.list_files`: files only (dirs excluded), workspace-prefixed
  sorted paths, exact sizes; missing dir → error rather than empty-list
  ambiguity (test 8).
- `FilesService::meta`: `TextFile`/`Directory`/`BinaryFile` classification,
  exact sizes, missing → error, size tracks mutation, nested
  space+Unicode paths (test 9).

### §9 Search — **Passed** (test 10)
Realistic tree (`src/main.rs`, `src/parser.rs`, `tests/parser_test.rs`,
`README.md`, plus a binary file):
- Zero matches → empty result (not error, not null).
- Many matches across nested dirs; binary file skipped silently.
- Repeated matches in one file → distinct hits with ascending line numbers
  and the matching line text.
- Case-insensitive substring semantics (no regex): `PARSER` matches, `fn(`
  matches literally.
- Unicode needle; limit bounding (`limit=1` → exactly 1; `limit=0` → empty
  by contract).
- No ordering assumption is asserted beyond the documented contract.

### §10 Hash — **Passed via independent vectors** (test 11)
- **FIPS 180-4 published vectors** (never the production `sha256_hex`
  helper): `sha256("abc") = ba7816bf…20015ad` presented via
  `awh fs replace --expected-hash` on a file containing exactly `abc`
  (no trailing newline) → edit **commits**, proving algorithm (SHA-256),
  encoding (lowercase hex), and boundary (pre-edit full content).
- One hex character corrupted in the same vector → **exit 4 conflict**,
  bytes unchanged — proving the guard compares the full digest, not a
  prefix.
- `sha256("") = e3b0c442…b855` authorizes an insert into a genuinely empty
  file → commits; resulting bytes `seeded\n`.

### §11 Verify — **Passed (edit-plane semantics)**
The standalone `fs verify <file>` basic op is **Not implemented**; the
implemented `awh fs verify <edit_id>` recovery-chain verification and the
expected-state guard are covered by `tests/cli_fs_edit.rs`
(`fs_history_and_verify_failures`, `fs_expected_state_conflict_blocks_stale_overwrite`)
and `tests/acceptance_editing.rs` (verified/failed distinguishability,
including rollback-after-restart chains via
`mcp_restart_before_and_after_rollback`). No new duplicates added.

### §12–§18 Controlled edits, stale-state, atomicity — **Passed (existing suites, inspected)**
`replace/insert/delete-range/patch/apply-diff` exact bytes, occurrence
semantics, ambiguous-match refusal, UTF-8 line boundaries, multi-hunk
transactional failure, stale expected-state (hash/size/lines/context)
rejection, crash/recovery, rollback idempotence + conflict: covered by the
four prior suites listed in §1 (46 tests total), re-run green in this
session as part of the full `--all-targets` gate (1412 passed / 0 failed).

### §20 Path safety — **Passed**
Traversal + absolute + symlink escape (prior suites:
`fs_traversal_and_unsafe_paths_fail_closed`,
`symlink_escape_rejected_by_cli_edit_plane`,
`final_component_symlink_substitution_is_refused`,
`write_file_cannot_escape_root_through_a_symlinked_directory`) plus the new
outside-sentinel negative proof (test 5). Never used a sensitive host path.

### §22 Parity — **Passed** (test 12)
The same logical write through **three real boundaries** — MCP
`workspace.write_file` (tool plane), `FilesService::write_atomic` (canonical
service), and Control API `PUT /api/v1/files/content` (real router, bearer
auth) — produces **byte-identical files**. Cross-boundary read parity also
asserted (what one boundary wrote, the others read identically).

### §28–§29 Limits and resources — **Passed (bounded, CI-practical)**
- 5 MiB write cap: at-cap accepted, above-cap rejected without partial file
  (test 6).
- 2 MiB read cap: one byte over rejected, body never returned (test 2).
- 12-level nested write/read round trip; 40 sibling files listed completely
  (test 13).
- `AWH_MAX_MCP_LINE_BYTES` — config-layer resolution is covered by
  `src/mcp/config.rs` unit tests (TP01 §28 evidence); the fs-plane caps are
  the material limits tested above.

### §30 Workflows — mapping
- **A (safe edit)**: tests 4, 11 + prior `fs_replace_exact_bytes_and_unicode`.
- **B (stale edit)**: test 11 (wrong-vector conflict) + prior
  `fs_expected_state_conflict_blocks_stale_overwrite`,
  `stale_expected_state_conflicts_not_overwrites`.
- **C (failed multi-step)**: prior `fs_patch_and_apply_diff` multi-hunk
  failure, `required_snapshot_failure_still_blocks_mutation`,
  `failed_coordinated_write_leaves_no_leftovers`.
- **D (restart)**: prior `fs_state_survives_process_restart`,
  `mcp_restart_before_and_after_rollback`.
- **E (concurrent)**: prior `fs_coordination.rs` (20 tests: interleave-free
  concurrent writes, torn-state prevention, lock timeout fail-closed,
  deadlock-freedom for overlapping multi-file sets).

### §33 Security regressions — **Passed**
Traversal, stale overwrite, malformed-request safety, denied-mutation
immutability, symlink model: all pinned by the existing permanent tests plus
tests 5/7/11 of the new suite. No test bypasses were introduced.

## 4. Defects found

**One production defect, fixed in this PR (carried from the TP02 Kilo
review, shipped here because the fix lives on this branch):**

0. **`ProviderRegistry::aggregate_tools` could stall or drop the whole
   dynamic tool catalog on one unhealthy provider.** Two failure modes:
   (a) a provider whose listing *errors* poisoned the entire
   `tools/list` advertisement (the old `?` propagated the first error);
   (b) a provider whose backend *hangs* stalled the advertisement for
   the duration of the HTTP client timeout — serially across providers,
   so N hung backends cost N × timeout. Fix (in `src/mcp/providers.rs`):
   per-provider isolation — listings run **concurrently** under a 20 s
   per-provider cap (`PROVIDER_LIST_TIMEOUT`); failing providers are
   skipped with a `dynamic_provider_rejected`/`list_failed` audit event,
   hanging ones with `list_timeout` (reasons kept under the 16-char
   redaction threshold so they persist verbatim in the audit record).
   Regression tests: `aggregate_tools_isolates_failing_providers`,
   `aggregate_tools_isolates_hanging_providers`,
   `aggregate_tools_bounds_n_hanging_providers_to_the_budget` (the
   whole listing phase is bounded by the aggregate budget — one cap +
   slack — whatever N is; see §11 for the evolution of this bound).

Two near-miss observations worth keeping in the record:

1. **MCP read is text-only by contract** (invalid UTF-8 → error, not
   replacement). This is documented and intentional; the binary-safe
   boundary is `FilesService::read_bytes`. Pinned as a permanent test
   (test 3) so a future "helpful" mojibake regression cannot slip in.
2. **The 5 MiB write cap is checked before any filesystem mutation**
   (verified: target absent after a rejected over-cap write — no partial
   file). Pinned as a permanent test (test 6) so ordering regressions
   (write-then-check) cannot leak partial state.

## 5. Not implemented / Unproven

- **Not implemented** (honestly classified, per §36): `awh fs read`,
  `fs write`, `fs stat`, `fs search`, `fs hash` as CLI commands. The
  equivalent capabilities exist at the MCP tool plane, Control API, and
  canonical service — and are now covered there. No fake CLI tests were
  fabricated.
- **Unproven**: none within the implemented surface. Windows/macOS
  filesystem-specific behavior (ACL permission-denied paths, ADS) is
  platform-dependent and covered by CI on all three platforms for the
  assertions that are portable; permission-denied injection is not
  reproducible cross-platform in this environment, so those specific
  cases remain in the platform matrix rather than asserted here.

## 6. Gates

```
cargo fmt --all -- --check        clean
cargo clippy --all-targets -- -D warnings   EXIT 0 (clean)
env -u COMPOSIO_API_KEY cargo test --all-targets
  → 1412 passed, 0 failed (1399 pre-existing + 13 new)
```

## 7. Test-theater self-audit (§34)

- Mutations verified by reading real bytes with `std::fs` — never by
  trusting the success envelope.
- Rejections verified byte-for-byte unchanged + outside sentinel negative
  proof.
- Hash evidence uses published FIPS vectors — not the production helper.
- No mocks of the filesystem; no hard-coded developer paths (all
  `tempfile::tempdir`); no env-var mutation; no unrelated production
  behavior changed. Production diffs in this PR are exactly the
  `src/mcp/providers.rs` provider-isolation fix documented in §4 (plus
  its regression tests).
- Ordering assumptions asserted only where the public contract guarantees
  them (MCP list sorts; service list order explicitly not assumed).


## 8. Kilo review round 1 — addressed

The first Kilo review of this PR raised test-hardening points, all fixed in
this revision:

- **Unique sentinel + cleanup** (was: fixed `/tmp/outside-sentinel.txt`
  shared by parallel runs): the outside sentinel is now
  `outside-sentinel-<pid>.txt` and is removed at test end.
- **Traversal rejects now pin the failure reason** ("path traversal is not
  allowed" / "absolute paths are not allowed") instead of any-error, plus
  a **positive control**: a benign interior write through the same tool
  and session proves the rejects are containment, not blanket denial.
- **Over-cap leak check now inspects the whole serialized JSON-RPC error**
  (code/message/data) for file-byte canaries, not just `message`.
- **`big.bin` now asserted**: the NUL-byte boundary — a 1024-byte NUL
  file is valid UTF-8 yet classifies `BinaryFile` with exact size 1024
  (the honest boundary next to `blob.bin`).
- **`awh init` output asserted** (success + stderr), not discarded.
- **Hermetic child processes**: all spawned `awh` children strip the
  `AWH_*` and provider-variable ambient environment via the same
  enumerated set as `tests/foundation_cli.rs`.
- **Full-digest comparison actually pinned**: the corrupted vector now
  corrupts the **last** hex character — the value still passes the
  structural 64-hex gate, so it reaches the digest comparison itself,
  and a guard comparing fewer than all 64 characters would accept the
  file (the difference is beyond its cutoff). Only a comparison through
  the final character rejects it. (An earlier revision also asserted a
  truncated 63-char digest; review correctly pointed out that case is
  rejected structurally by `is_valid_hash` *before* any comparison, so
  it proved nothing about full-length equality and duplicated the
  in-crate malformed-hash coverage — removed.)
- **Mis-citation fixed**: the path-normalization comment cited §34;
  the actual rule is the repo's cross-platform path-assertion gotcha.
- **Prior-suite counts corrected** (16/24/13/26 → 15/8/3/20 = 46).

## 9. Kilo review round 2 — addressed

All points from the second review round are fixed in this revision:

- **Report honesty (§4/§7)**: "no production defect" and "tests-only
  diff" were factually wrong — the PR ships the `src/mcp/providers.rs`
  isolation fix (the base branch still has the old `?`). Both statements
  corrected to name the defect and its tests.
- **Truncated-digest case removed**: it was rejected structurally by
  `is_valid_hash` before any digest comparison, so it added no evidence
  about full-length equality (§8 bullet rewritten accordingly). The
  tail-corruption case remains the full-digest pin.
- **Sentinel cleanup made panic-safe**: the outside sentinel now lives
  in its own sibling `tempdir()` — cleanup is TempDir's Drop (runs even
  when a test panics) and an unlink failure can no longer fail a test
  whose traversal proof already succeeded.
- **Sanitized-variable triplication removed**: the strip list now has a
  single source, `tests/common/mod.rs` (`#[path]`-included by
  `foundation_cli.rs`, `workspace_runtime_cli.rs`, and
  `fs_basic_boundaries.rs`) — a new `AWH_*` read added to src/ needs one
  edit, and every suite inherits it.
- **Test-only env override eliminated from production code**: the
  `AWH_TEST_PROVIDER_LIST_TIMEOUT_MS` env var (live in release builds,
  unclamped — a stray `0` would silently empty the dynamic catalog) is
  gone. The hang-isolation tests now use a `#[cfg(test)]` atomic
  override with an RAII guard (`ListTimeoutOverride`): no release-build
  surface, no process-global env mutation, panic-safe restore.
- **Serial N × cap stall removed**: round 2 ran listings with
  `futures_util::future::join_all` (already a production dependency),
  each under its own cap — N hanging providers cost one cap of wall
  time, not N. (That `join_all` was itself replaced in rounds 3–4 by
  the bounded window + aggregate budget; the regression test is now
  `aggregate_tools_bounds_n_hanging_providers_to_the_budget`.)
- **Audit reasons survive redaction**: `provider_list_failed`/
  `provider_list_timeout` were ≥16-char base62 runs and persisted as
  `[redacted]` — the ring could not distinguish a hang from a failure.
  Reasons shortened to `list_failed`/`list_timeout` (<16 chars, persist
  verbatim); both provider tests now assert the reason in the record.

The two review comments that landed on PR #138's copy of the shared
commits (the providers.rs hang timeout and workspace-runtime hermeticity
points) are addressed HERE, on this branch — nothing is deferred to or
"belonging to" #138; whichever PR merges first carries the fixes.

## 10. Kilo review round 3 — addressed

Round 2's own fixes were re-reviewed; four substantive findings, all
fixed:

- **Unbounded outbound burst** (WARNING-class): round 2's `join_all`
  started ALL provider listings at once — a registry of N providers
  burst N simultaneous outbound HTTP requests on every `tools/list`.
  Now bounded: `futures_util::stream::…buffer_unordered(
  PROVIDER_LIST_CONCURRENCY)` (window of 8). The window alone would
  still allow ⌈N/8⌉ sequential waves of hangs, so the listing phase is
  ALSO wrapped in an aggregate budget (the effective per-provider cap
  plus a small collection slack): when the budget fires, providers
  not yet listed are skipped and audited with the distinct reason
  `list_budget`, so the total cost of a full aggregation is ~one cap
  regardless of N. Results are re-sorted by provider id so the
  advertised catalog keeps a stable order.
- **Process-global test override** (WARNING-class): the round-2
  `#[cfg(test)]` atomic + RAII guard was still process-global — two
  parallel hang tests could clobber each other's cap (guard drop
  zeroes the shared slot mid-test). Replaced by a per-instance
  override: `ProviderRegistry { list_timeout_override: Option<Duration> }`
  + `#[cfg(test)] with_list_timeout(cap)` constructor. No global state,
  no release-build surface (the field is `None` under `Default`), no
  races; each test's registry carries its own cap.
- **Timeout log lied under override**: `timeout_secs =
  PROVIDER_LIST_TIMEOUT.as_secs()` logged the constant even when the
  effective cap was the test override. Now logs the effective cap.
- **Sentinel no longer targeted by the escapes** (WARNING-class, my
  round-2 fix was wrong): moving the sentinel into a *sibling* tempdir
  broke the proof — the escapes `../<name>` resolve against the
  workspace root's parent, i.e. the OS temp dir, not that sibling. A
  traversal regression would have overwritten a file the test never
  checked. Fixed layout: the workspace root is now `<parent TempDir>/
  workspace-root/` and the sentinel sits directly in `<parent>/`, so
  `../sentinel` and `a/../../sentinel` resolve to EXACTLY the asserted
  sentinel (path math re-verified). Drop-based cleanup retained.

## 11. Kilo review round 4 — addressed

Round 4 reviewed the round-3 code itself; five new findings, all fixed:

- **(4172077715, WARNING) stale invariant doc**: the `PROVIDER_LIST_
  CONCURRENCY` doc still claimed "N slow providers cost one cap", which
  the round-3 change itself made false (a window of 8 turns N hangs
  into ⌈N/8⌉ waves). Doc rewritten to state the window's actual
  purpose (outbound-request bounding) and to reference the aggregate
  budget for the latency bound.
- **(4172077725, SUGGESTION) unbounded aggregate worst case**: the
  bounded window reintroduced a multiplicative worst case — ⌈N/8⌉ ×
  20 s ≈ 140 s at 50 providers — and nothing bounded the aggregate
  (`dispatcher.rs` awaits it holding the registry read guard; stdio
  `handle()` has no outer request deadline, unlike the HTTP plane's
  30 s `TimeoutLayer`). Fixed structurally in `aggregate_tools`: the
  whole listing phase is now wrapped in `tokio::time::timeout(
  effective_cap + slack, …)`; on exhaustion, providers not yet listed
  are skipped and audited `list_budget` (distinct from `list_timeout`),
  so `tools/list` always completes in ~one cap regardless of N. The
  N-hangs test now pins exactly this: 10 hangs under a 200 ms cap
  finish in ~one budget (not 2 waves ≈ 400 ms, not the 2 s serial
  sum) and leave a `list_budget` audit event.
- **(4172077717, SUGGESTION) "Zero selects the default" was wrong**:
  `effective_list_timeout()` is `unwrap_or`, so `Some(ZERO)` honors
  zero (instant timeout for every provider — empty catalog), not the
  default. The field doc now states the real semantics (`None` =
  default, `Some` used verbatim) and the `#[cfg(test)]
  with_list_timeout` constructor now rejects a zero cap with an
  explicit assert so no test can silently empty the catalog.
- **(4172077722, SUGGESTION) redundant clone**: `providers` (an owned
  `Vec<String>` returned by `self.providers()`) was cloned into
  `provider_ids` although nothing borrows it past the
  `stream::iter` collect. Clone removed — the Vec is moved.
- **(4172077730, SUGGESTION) stale test name/docs/messages**: the
  10-hang test still promised "ONE cap" (round-2 contract) while
  asserting a 2-wave budget, and its doc/messages described the old
  serial loop. Test renamed to `aggregate_tools_bounds_n_hanging_
  providers_to_the_budget`, comments and the failure message now
  describe the real contract (aggregate budget, N-independent), and
  the test additionally asserts the `list_budget` audit reason.

The remaining round-4 comments (4171575922/5939/5941) are stale
re-posts of already-addressed round-1 items (traversal reason pinning +
positive control, `big.bin` classification asserts, `awh init` success
assertion) — already present in the tree, verified on the pushed diff,
and left as-is; replied on the PR.

## 12. Kilo review round 5 — addressed

Six findings on the round-4 code. Five were fixed complete; the sixth
(the test-narrative misstatement) was fixed at only ONE of its two
sites — the second occurrence, at `src/mcp/providers.rs:712-713`, was
caught by round 6 (§13, finding 4172273839) and fixed there.

- **(4172193269, WARNING) Budget truncation was name-deterministic**:
  `providers()` returns ids sorted, `buffer_unordered` starts futures
  in that order, and the budget pre-empts whatever has not finished —
  so with N > window, the alphabetically LAST ids were deterministically
  starved on EVERY call. Fixed with a per-instance round-robin start
  rotation (`start_rotation: AtomicU64`): each call rotates the start
  order by one, so over N calls every provider gets an early slot
  exactly once — budget truncation is spread fairly across the id
  space instead of always falling on the same names. Empty-registry
  division-by-zero guarded; results still re-sorted (stable catalog);
  the audit loop still walks the sorted list.
- **(4172193280, WARNING) Claimed fixed but wasn't — "five"** in the
  10-hang test's `.expect` message (a leftover from the round-2
  5-provider version). Now says ten.
- **(4172193271) `budget_ms` under-reported the deadline**: it logged
  the cap, not the budget that actually fired (cap + slack) — the same
  misreport class as the round-3 `timeout_secs` bug. Now logs
  `cap + PROVIDER_LIST_BUDGET_SLACK`.
- **(4172193279) Test narrative misstated the budget**: "the aggregate
  budget is also 200ms" was wrong — it fires at cap+slack = 300 ms;
  wave 2 is skipped at 300 ms, not left to resolve at 400 ms. Comment
  corrected (and it now explains why wave 1 is *collected* as
  `list_timeout` while wave 2 is *skipped* as `list_budget`).
- **(4172193282) Orphaned fragment** in this report: §11's insertion
  consumed the subject sentence of the §10 closing paragraph, leaving
  a dangling "(traversal reason pinning + …)" fragment. Merged back
  into one sentence.
- **(4172193284) Stale test name** in the defect table (§4) and the
  02-report: both cited the pre-rename
  `…_to_one_cap` name/contract. Updated to `…_to_the_budget` with the
  real N-independent contract; the round-2 history note now flags the
  bound's evolution explicitly.

## 13. Kilo review round 6 — addressed

Five findings on the round-5 code; the substantive one is a design
fix, the rest are honesty corrections:

- **(4172273831, WARNING) Rotation alone made the advertised catalog
  call-dependent**: two identical `tools/list` calls against an
  unchanged registry could advertise different tool sets, and MCP
  clients cache `tools/list` to build their tool prompt — a cached
  tool could vanish on the next listing. Accepted the reviewer's
  suggested fix: `ProviderRegistry` now keeps a per-provider
  `last_good` listing (std Mutex, never held across an await). On
  budget truncation the provider's LAST GOOD listing is served and
  audited with the new reason `list_stale` — only a provider with no
  cached listing is actually skipped (`list_budget`). Membership is
  now sticky: once a tool has been advertised, it stays advertised
  (until unregister). Hung/failed providers are still dropped
  (`list_timeout`/`list_failed`) — truncation is OUR scheduling
  artifact, a hang is a provider-health signal, and the distinction
  is documented at the cache field. Rotation is retained: it gives
  never-yet-listed providers a fair first shot at the early wave.
  Pinned by the new
  `aggregate_tools_serves_last_good_listing_when_budget_truncates`
  (deterministic rotation arithmetic: call 1 offset 0 → wave 1 →
  cached; call 2 offset 1 → wave 2 → truncated → served, `list_stale`
  asserted). The two "stable catalog" comments were reworded to the
  precise claim: stable ORDER always; stable membership once listed.
- **(4172273836, WARNING) 02-report still taught the pre-budget cost
  model** ("N slow providers cost ~⌈N/8⌉ caps") that the aggregate
  budget invalidated — contradicting this report. Fixed: the cost is
  now stated as one cap + slack, independent of N, and the new
  membership-stability test is listed.
- **(4172193279 second site, 4172273839/4172273843)**: the round-5
  fix corrected only ONE of two identical "budget = cap" misstatements
  in the same test; the survivor (`providers.rs:712-713`) is fixed,
  and §12 above no longer claims the item was fully fixed. The
  elapsed bound's comment now also says what actually discriminates
  the budget (the `list_budget` audit event, not the loose 1200 ms
  wall-clock bound, which only pins the serial-loop regression).
- **(4172273848)**: the round-2 history bullet was present tense about
  a `join_all` implementation the tree no longer contains (grep finds
  nothing) and split the test name across lines. Rewritten in past
  tense with the identifier on one line.

## 14. Kilo review round 7 — re-anchored re-posts + one new item

Round 7 re-anchored six earlier findings to `974fa23`. Verified against
the tree, each is stale:

- **4171575922** (traversal proof): already fixed — the escapes assert
  `message.contains("traversal is not allowed")`, the absolute-path
  rejection asserts its own reason, and a positive control (benign
  interior write + read-back) anchors the negatives. `tests/fs_basic_
  boundaries.rs:331-359`.
- **4171575939** (`big.bin` unasserted): already fixed — classified
  `BinaryFile` with `size == 1024` and a comment explaining the NUL
  boundary. `tests/fs_basic_boundaries.rs:503-509`.
- **4171575941** (init result discarded): already fixed — the output's
  `status.success()` is asserted with stderr in the message
  (`tests/fs_basic_boundaries.rs:641-647`); the env-sanitization
  sub-point is satisfied too (`awh_in` → `common::sanitized_command`
  strips `SANITIZED_AWH_VARS`/`SANITIZED_PROVIDER_VARS`).
- **4172077725 / 4172193269 / 4172273831** (aggregate budget, rotation,
  call-dependent catalog): all three are superseded by the round-6
  design — aggregate budget + rotation + last-good cache. The one NEW
  sub-point inside 4172193269 (operators could misread a budget-pressed
  registry as broken because `docs/mcp.md` said nothing about the
  budget) IS real: `docs/mcp.md` now documents the listing budget
  (window + cap + aggregate budget, the `list_stale`/`list_budget`
  serve/skip semantics, audit observability, and the explicit
  completeness-yields-to-liveness trade-off).
- **4172273843** (report claimed complete fix): already fixed — §12
  records the one-site-only round-5 fix and §13 the round-6 completion.
