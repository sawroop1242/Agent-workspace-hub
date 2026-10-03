# Master Test Prompt 02 — Workspace Runtime: Completion Report

- **Feature:** Workspace Runtime (workspace lifecycle and identity boundary: create/init, discovery, open/resolution, info, removal, root validation, durable manifest/state, identity, canonical-root binding, idempotent initialization, restart/process-boundary behavior, isolation, malformed/foreign/corrupt state, path handling, concurrent lifecycle operations)
- **Branch:** `testing-tp02-workspace-runtime` (from `rust` @ d186abe)
- **Date:** 2026-09-27
- **Suite:** `tests/workspace_runtime_cli.rs` (11 new executable black-box tests against the real compiled `awh` binary), complementing pre-existing coverage in `tests/init_cli.rs` (11 tests) and `src/services/init.rs` unit tests (11 tests), all re-run green on this branch.

## Implementation status

The final-target CLI surface for this family is `awh workspace create|list|open|info|remove`. **None of those subcommands exist in the binary on this branch** (`src/main.rs` `Command` enum). What exists — and what this prompt therefore tests — is the TW-001 workspace-runtime contract:

- `awh init [--path DIR]` — the documented create/initialize entry point (idempotent; the final-target `workspace create` equivalent).
- Durable manifest `.agent/workspace.json` = `{version, workspace_id, workspace_root, created_at}` (MANIFEST_VERSION=1), written under `StoreLock` with atomic rename.
- `WorkspaceId` typed identity (`ws-` prefix at generation).
- Canonical workspace-root binding (identity is bound to the root; moves/copies to other roots are rejected).
- Fail-closed handling of corrupt JSON, unsupported versions, missing required fields, invalid ids, and file-as-root.
- `awh status` — a bootstrap banner only (see Observations).

### Final-target command classification

| Final-target command | Classification | Evidence |
|---|---|---|
| `workspace create` | **Implemented and tested** as `awh init [--path DIR]` | this suite + `tests/init_cli.rs` |
| `workspace list` (discovery across directories) | **Not implemented** — no registry of workspaces exists; a workspace is any directory carrying a valid `.agent/workspace.json` | `src/main.rs` command enum |
| `workspace open` | **Not implemented** as a command — resolution is implicit: AWH binds to the CWD (or `--path`); reopen = invoke any AWH command in that directory | this suite (`init_is_idempotent_across_processes_with_full_state_compare`) |
| `workspace info` | **Not implemented** — `awh status` is a state-independent bootstrap banner, not a workspace-inspection surface | `status_banner_is_state_independent_per_documented_contract` |
| `workspace remove` | **Not implemented** — no CLI removes workspace state (Workflow F not executable; manually deleting the directory is the only path) | `src/main.rs` command enum |

## Real interfaces exercised

- Real compiled binary via `CARGO_BIN_EXE_awh` subprocess for every assertion (never in-process mocks for CLI behavior).
- Real filesystem: `tempfile` roots; real `.agent/workspace.json` bytes; byte-for-byte tree snapshots of the whole `.agent` state; `fs::rename` for real move scenarios.
- Fresh-process boundaries: each `run()` spawns a new process; identity persistence proven across 3+ fresh invocations per test.
- No environment mutation: no `HOME`/`USERPROFILE` writes anywhere; tests are parallel-safe and cross-platform (path assertions compare canonical-to-canonical strings and manifest-contains forms, never raw-vs-canonicalize).
- No network, no credentials.

## Results

### Passed (direct evidence)

| # | Contract | Evidence |
|---|---|---|
| 1 | Fresh init creates durable identity + empty policy, no implicit authority (agents/grants) | pre-existing `tests/init_cli.rs::cli_fresh_init_creates_workspace_identity_without_authority`, re-run green |
| 2 | Re-init is idempotent: identity stable, manifest byte-stable, no state reset, correct "already initialized" report | pre-existing `cli_reinit_is_idempotent_and_preserves_state` + new `init_is_idempotent_across_processes_with_full_state_compare` (extends to the ENTIRE `.agent` tree byte-compare across fresh processes) |
| 3 | Unrelated files (text + binary, nested dirs) survive init byte-for-byte | new `init_preserves_unrelated_files_byte_for_byte` |
| 4 | `--path a/b/c` with missing parents: parents created, state only at target, manifest records the canonical TARGET root, no leakage to CWD/intermediates | new `init_creates_missing_parent_directories_and_binds_that_root` |
| 5 | `--path` normalization: trailing slash and `.` forms bind the canonical root | new `init_accepts_trailing_slash_and_dot_path_forms` |
| 6 | `--path` explicit-root no-CWD-leak contract | pre-existing `cli_init_with_explicit_path_creates_state_only_there` |
| 7 | File-as-root rejected | pre-existing `cli_init_rejects_file_as_root`, `root_that_is_a_file_is_rejected`, `root_under_a_regular_file_is_rejected` |
| 8 | `.agent` exists as a regular FILE → fail closed, no state smuggled elsewhere, pre-existing file untouched | new `init_fails_closed_when_agent_dir_is_a_regular_file` |
| 9 | Corrupt manifest (invalid JSON) → explicit error, no reset | pre-existing `cli_init_rejects_corrupt_manifest_without_resetting_state` |
| 10 | Missing required field (`workspace_root`) → serde "missing field" error, manifest untouched | new `manifest_with_missing_required_field_fails_closed_at_cli` |
| 11 | Unsupported manifest version → explicit error, manifest untouched | pre-existing `cli_init_rejects_unsupported_manifest_version` |
| 12 | Foreign manifest (copied to another root) → "different root" rejection, non-zero exit, manifest byte-identical | pre-existing `cli_init_rejects_foreign_manifest_with_nonzero_exit` |
| 13 | Moved workspace (old root gone) → "recorded workspace root does not resolve", non-zero exit, no re-bind | new `moved_workspace_is_rejected_without_reset` |
| 14 | Moved workspace with the old path re-created (old root resolves to unrelated content) → still "different root", no adoption | new `moved_workspace_with_replaced_old_root_reports_foreign_root` |
| 15 | Traversal-id manifest (`../evil`) rejected without mutation | pre-existing `manifest_with_invalid_workspace_id_is_rejected_without_mutation` |
| 16 | Two workspaces A/B: distinct `ws-` ids, each manifest records its own canonical root and identity, zero cross-references in either state tree | new `two_workspaces_have_distinct_identities_and_roots` (Workflow B) |
| 17 | Restart/process-boundary: fresh process reconstructs identical identity (id/root/created_at) with no implicit authority | pre-existing `service_restart_reconstructs_same_identity_and_no_new_authority` (Workflows A/C service leg) |
| 18 | Concurrency: 8-thread barrier race → exactly one `Created`, one canonical identity, byte-stable post-race re-init, no lock residue | pre-existing `parallel_inits_produce_exactly_one_canonical_identity` |
| 19 | `awh status` banner: exit 0 and byte-stable output in uninitialized, initialized, and corrupt-workspace directories (the documented bootstrap contract, docs/error.md §6) | new `status_banner_is_state_independent_per_documented_contract` + TP01 `status_succeeds_on_uninitialized_and_initialized_directories` |
| 20 | `load_workspace_manifest` on an uninitialized existing root fails closed | pre-existing `load_manifest_on_uninitialized_root_fails_closed` |
| 21 | Corrupt `policy.json` blocks init without mutation | pre-existing `corrupt_policy_store_blocks_init_without_mutation` |
| 22 | Manifest serde round-trip preserves all fields | pre-existing `manifest_serde_round_trip` |

### Failed (defects exposed and fixed on this branch)

| # | Defect | Root cause | Fix | Regression test |
|---|---|---|---|---|
| 1 | **MCP tool plane dies when one connector is unhealthy.** With an invalid/expired `COMPOSIO_API_KEY` in the environment, `tools/list` returned the upstream 401 and the ENTIRE catalog (73 tools) vanished; opencode clients showed "Failed to get tools" and lost all AWH tooling. | `ProviderRegistry::aggregate_tools` (`src/mcp/providers.rs`) propagated one provider's `list_tools` failure via `?`, poisoning the whole advertisement — despite the same function's per-tool schema gate explicitly promising "fail closed per tool, not per provider". | Isolate per provider: on `list_tools` error, `tracing::warn` + audit `dynamic_provider_rejected` deny naming the provider + skip it. Healthy providers keep advertising; the direct per-provider path (`connector.tools {provider}`) still surfaces the real upstream error for diagnosis. Found while executing the user-requested opencode↔AWH end-to-end validation with a live (rejected) Composio key; before the fix `tools/list` = Composio 401, after: full 73-tool catalog and opencode reports "awh connected" again. | `mcp::providers::tests::aggregate_tools_isolates_failing_providers` (healthy + failing provider → Ok listing, healthy tools present, failing provider absent, deny audited) |

### Blocked

None. All tests run on the local Linux container against the real binary; no platform-specific workspace feature was left unverified on this platform. CI runs the same suites on macOS and Windows; the suite was written cross-platform-safe (no raw-vs-canonicalize path comparisons, no env mutation).

### Unproven

1. **Workspace discovery/listing (`workspace list`)** — no implementation exists to test. A workspace is discoverable only by navigating to its directory.
2. **Workspace info** — no command reports workspace identity/root to the user. `awh status` is a bootstrap banner (pinned; see Observations). Identity is observable today only via `awh init`'s "already initialized" report.
3. **Workspace removal (`workspace remove`)** — not implemented; Workflow F is not executable on this branch.
4. **Workspace open as an explicit command** — resolution is implicit (CWD binding); the restart/re-resolution behavior itself is proven (Passed #17).

## Observations (not defects; flagged for maintainers)

1. **`awh status` is a stub.** It prints `Agent Workspace Hub — Rust / status: bootstrap complete` unconditionally and never reads `.agent/workspace.json`. This IS the documented current contract (docs/error.md §6 pins the exact output; docs/CLI.md shows it in the roadmap section) — so this is recorded as a gap, not a defect: the first user-facing workspace-info surface (`workspace info`) remains unimplemented. Tests pin the current behavior; implementing `workspace info` is feature work outside a verification prompt.
2. **Typed-id prefixes are not enforced at the manifest boundary.** `WorkspaceId::validate` is char-safety-only (length, separators, traversal, control chars); a char-safe wrong-prefix id (`xx-…`) in `.agent/workspace.json` is accepted by `awh init` (reports "already initialized" and echoes it). This is pinned deliberately in `src/core/identity.rs` (`identity_types_are_unambiguous_across_relationships`: "validation is applied per type"), and anti-transfer security is carried by the canonical-root binding rather than id shape. The new test `wrong_prefix_workspace_id_is_accepted_by_documented_design` pins this boundary so it cannot change silently. Hardening opportunity, not a defect: enforcing the `ws-` prefix at the manifest-load boundary would strengthen the typed-identity invariant documented alongside the TW-001 `WorkspaceId` (`ws-` representation) at a cost of rejecting manually-edited manifests that today load fine.

## Workflow acceptance mapping

| Workflow | Result | Where proven |
|---|---|---|
| A — New workspace (create → status → terminate → fresh process → inspect) | Passed (with the caveat that "inspect" = init's already-initialized report; no info command exists) | `init_is_idempotent_across_processes_with_full_state_compare`, `service_restart_reconstructs_same_identity_and_no_new_authority` |
| B — Two workspaces (create A, create B, open A, info A, open B, info B) | Passed | `two_workspaces_have_distinct_identities_and_roots` + `load_workspace_manifest` per-root reload |
| C — Reinitialize (init → capture → init → compare) | Passed | new full-tree byte-compare test + pre-existing re-init test |
| D — Foreign state (copy metadata to B, init B) | Passed | pre-existing copy tests + new move variants (old root gone / old root replaced) |
| E — Corrupt state (corrupt → fresh process → open) | Passed | corrupt JSON, missing field, unsupported version, traversal id, corrupt policy — all fail closed without repair |
| F — Removal | Not executable — `workspace remove` not implemented |

## Gates

- `cargo fmt --all` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo test --all-targets` — full suite green on this branch (including the 11 new tests + the tools/list regression test).
- New suite: `cargo test --test workspace_runtime_cli` — 11/11.
- Pre-existing suites re-verified: `cargo test --test init_cli` — 11/11; `cargo test --lib services::init` — 11/11.

## Reproduction

```bash
cargo test --test workspace_runtime_cli        # the TP02 suite
cargo test --test init_cli                    # pre-existing TW-001 CLI suite
cargo test --lib mcp::providers               # tools/list isolation regression
cargo test --all-targets                      # full workspace
```

Manual opencode↔AWH evidence for the Fixed defect #1 (invalid Composio key in env): before the fix, `tools/list` returned the Composio 401 and `opencode mcp list` reported "awh failed: Failed to get tools"; after the fix, `tools/list` returns the 73-tool catalog (0.4s — it still attempts the provider once) and `opencode mcp list` reports "✓ awh connected" with the same invalid key present, while `connector.tools {provider: "composio"}` still surfaces the real 401 for diagnosis.


## 9. Kilo review — addressed (via PR #140, round 1)

Two review points on this report's test suite
(`tests/workspace_runtime_cli.rs`) were raised during the TP03 review
cycle and are fixed on the same branch:

- **Hermetic child processes**: the suite's `run()` helper now strips
  the ambient `AWH_*` and provider-variable environment from every
  spawned `awh` child (same enumerated `SANITIZED_*` sets as
  `tests/foundation_cli.rs`), so a developer shell or CI runner can no
  longer change what these tests observe. The suite header's
  "no environment mutation" claim now extends to what children
  *inherit*.
- **Manifest poisoning made self-verifying**: the wrong-prefix test's
  string replacement now asserts the manifest bytes actually changed
  before rewriting (`edited != original`), so a serialization-format
  change can no longer turn the poisoning into a silent no-op and the
  test into a tautology.

One review point on `src/mcp/providers.rs` (hang isolation, not just
error isolation) is also fixed on this branch: `aggregate_tools` now
wraps each provider's listing in a 20s per-provider timeout
(`provider_list_timeout()`; test-only override via
`AWH_TEST_PROVIDER_LIST_TIMEOUT_MS`), so a black-holed backend is
skipped and audited (`provider_list_timeout`) instead of stalling the
whole `tools/list` advertisement. Regression test:
`aggregate_tools_isolates_hanging_providers`.
