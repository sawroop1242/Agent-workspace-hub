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

**None.** No production defect surfaced in this pass. Two near-miss
observations worth keeping in the record:

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
  behavior changed (the only diff outside tests/ is none).
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
  corrupts the **last** hex character (a prefix-checking guard would
  still reject a first-character corruption), and a **truncated
  63-char digest** is asserted rejected (a `starts_with` guard would
  accept it) — together these pin exact, full-length equality.
- **Mis-citation fixed**: the path-normalization comment cited §34;
  the actual rule is the repo's cross-platform path-assertion gotcha.
- **Prior-suite counts corrected** (16/24/13/26 → 15/8/3/20 = 46).

Two review comments (`src/mcp/providers.rs` provider-hang timeout,
`tests/workspace_runtime_cli.rs` hermeticity/poisoning) concern files in
PR #138's diff, not this PR's — flagged there instead.
