# TP04 — Git & First-Class Worktrees: Evidence Report

**Prompt:** `docs/testing-prompts/04-git-and-worktrees.md`
**Base:** `rust` @ `f332ebd` (post-merge of PRs #138, #140, #139)
**Branch:** `tp04-git-worktree-verification`
**Method:** TP01–TP03 methodology — repository forensics, empirical Git
behavior experiments, then a black-box verification suite that treats AWH
as a real product and `git` itself as the independent oracle. Every
mutation is proven by reading real repository/filesystem state back; no
test accepts a zero exit code or a JSON field as its only evidence.

**Suites (all new):**

| Suite | Tests | Scope |
|---|---|---|
| `tests/git_boundaries.rs` | 17 | GitService (service plane) + `git.*` MCP tools (agent plane) + `/api/v1/git/*` Control API (admin plane), each vs the git oracle |
| `tests/worktree_boundaries.rs` | 9 | Managed-worktree gaps beyond `tests/worktree_cli.rs`: file-level isolation, cross-process identity, hostile ids, Missing finalize, concurrent removal, unmanaged adoption, start-point validation, detached HEAD, exit-code categories |
| `tests/git_failure_injection.rs` | 1 (unix) | §27 failure injection: hanging `git` (timeout), missing `git` (spawn failure) |

**Gates:** `cargo fmt` clean, `cargo clippy --all-targets -- -D warnings`
clean, `cargo test --workspace` **1444 passed / 0 failed** (1054 lib + 390
integration/acceptance, includes the 27 new tests).

---

## Production defects found and fixed

### 1. `GitService::run` treated "non-zero exit + empty stderr" as success — HIGH

- **Test:** `commit_without_staged_changes_fails_without_creating_a_commit`
  (`tests/git_boundaries.rs`)
- **Fixture:** real repo, one base commit, clean index.
- **Invocation (service plane):** `GitService::commit("nothing")`.
- **Oracle:** `git rev-parse HEAD` before and after.
- **Expected:** an error; HEAD unchanged.
- **Actual (before fix):** `Ok(GitOutput)` with exit code 1 — a commit
  that created nothing reported SUCCESS. Root cause: `run()` gated the
  error on `!stderr.is_empty() && exit != 0`, but `git commit` with
  nothing staged prints "nothing to commit, working tree clean" to
  **stdout** with empty stderr. The same hole swallowed every other
  stdout-only git failure (e.g. `git push` to some transports, `git
  stash` variants). The method's own doc comment claimed "failing on a
  non-zero exit so git errors are never mistaken for success" — the code
  contradicted it.
- **Fix:** `src/services/git.rs` `run()` now fails on exit≠0 alone; the
  message falls back to stdout when stderr is empty.
- **Impact planes:** all three (service, MCP `git.commit`, Control API
  `/api/v1/git/commit`) share this method — one fix, three planes
  repaired. The Control API plane already mapped the error to 500; the
  MCP plane surfaces it as an `INTERNAL` error result; both now fire.

### 2. `git.branch` registry description promised a mutation it never had — LOW

- **Test class:** forensics + `mcp_git_read_tools_match_the_oracle`
  pins the read-only behavior.
- **Expected:** description of a tool that only resolves
  `rev-parse --abbrev-ref HEAD` must not say "Create or switch a git
  branch" — an agent planning from tool metadata would believe it can
  create branches and would retry a nonexistent plan.
- **Fix:** `src/mcp/tool_registry.rs` → "Show the current git branch name".
  (The `tools/list` wire string was already correct; this fixes the
  in-process registry used for metadata lookups.)

---

## Capability evidence

### Passed

Verified with real AWH operations cross-checked against independent
git/filesystem observation.

**Status**
- `status_matrix_matches_the_git_oracle_exactly` — byte-identical porcelain vs
  oracle across a mixed state (modified, untracked, staged-new,
  staged-deletion, spaced filename); clean tree is empty success; parser
  round-trips git's C-quoted spaced paths verbatim (documented consumer
  contract).
- `control_api_git_reads_match_the_oracle` — GET `/api/v1/git/status` body
  equals oracle porcelain.

**Diff / staged diff**
- `diff_and_staged_diff_separate_the_two_index_stages` — a file with both a
  staged and a further unstaged change produces two different diffs, each
  equal to its oracle (`diff` vs `diff --cached`); path filter excludes
  other files.
- `diff_shows_renames_and_unicode_like_the_oracle` — unicode content and
  `git mv` renames match the oracle; the rename (staged by `git mv`)
  appears in the staged diff, exactly where git puts it.
- `mcp_git_read_tools_match_the_oracle`, Control API `/git/diff?staged=true`
  — same separation on the other two planes.

**Log / branch / branches**
- `log_ordering_limit_and_content_match_the_oracle` — limits 1/3/5/9 equal
  the oracle; hashes in output resolve via `rev-parse --verify`; unicode
  subjects survive.
- `branch_and_branches_reflect_real_git_state` — current branch, branch
  lists, and post-checkout change all match git.
- `mcp_git_log_limit_clamping_is_deterministic` — 0→1, 99999→200, negative→20
  clamping each equal a direct oracle run at the clamped value.

**Stage / unstage**
- `stage_then_unstage_round_trips_the_index_exactly` — per-file and `.`
  staging advance the index exactly as `git add` does; unstage reverts the
  index only (working bytes byte-identical); unstaging a never-staged path
  is a no-op success (git semantics); unstaging `.` empties every staged
  index column.
- `stage_refuses_paths_that_escape_the_repository` — relative traversal
  (`../../x`) and absolute paths are refused by git's pathspec rules, with
  zero index mutation and the outside sentinel untouched.
- MCP + Control API stage/unstage repeat the matrix cross-plane
  (`mcp_git_mutations_match_the_oracle_and_refuse_traversal`,
  `control_api_git_mutations_and_push_pull_match_the_oracle`).

**Commit**
- `commit_persists_the_message_verbatim_and_stages_only_the_index` — a
  hostile message (leading dash, `$( )`, backticks, quotes, newlines,
  emoji) lands verbatim in `%B`, executes nothing (no `pwned` file), and
  commits exactly the index. Blank messages are refused before git runs
  with HEAD provably stable.
- `commit_without_staged_changes_fails_without_creating_a_commit` — **was
  the production defect**; now fails closed (see above).
- Cross-plane commit assertions in the MCP and Control API suites (same
  hostile message; `api-pwned` never materializes).

**Push / pull (disposable local bare remotes only — §2)**
- `push_and_pull_round_trip_against_a_disposable_bare_remote` — push
  advances the bare remote's ref to exactly local HEAD; idempotent re-push
  succeeds; a genuinely divergent push is rejected (non-fast-forward) and
  the remote ref is untouched by the rejection; `pull --no-rebase` merges
  remote work in (both histories present, peer file materialized); unknown
  remote errors.
- `control_api_git_mutations_and_push_pull_match_the_oracle` — the Control
  API plane moves the same bare remote to local HEAD; missing remote →
  structured 500 with no credential-shaped material in the body.

**High-risk operations (explicit service methods, no ambient callers)**
- `high_risk_operations_are_explicit_and_behavioral` —
  `discard_file` restores one file from HEAD and leaves others; refuses
  traversal upstream of git; `hard_reset` discards tracked
  modifications, moves no ref, and does NOT remove untracked files (that
  is `clean`'s job — pinned); `clean` removes untracked dirs, never
  tracked files; `delete_branch` removes the branch; `force_push`
  (--force-with-lease) advances the remote to local HEAD on a valid
  lease.

**Failure injection (§27)**
- `git_executable_failure_and_timeout_fail_closed` (own binary: PATH is
  process-global) — a `git` wrapper sleeping 30s is cut off by the
  service's 300ms budget with a deterministic "timed out" error; a PATH
  with no git makes every operation class fail closed with a spawn error
  that never masquerades as a timeout; repository state untouched; PATH
  restored via guard even on assertion failure.

**Worktrees (gap increment over `tests/worktree_cli.rs`)**
- `worktree_commits_are_isolated_at_file_level` — a commit in worktree A is
  in A's `git log`, absent from B's log and the main checkout's log; the
  file never appears in B, the main tree, or the main `status --porcelain`;
  A and B sit on distinct branches.
- `worktree_identity_persists_across_independent_processes` — id/branch/
  state/path identical across 3 fresh `awh` processes; git's own
  `worktree list --porcelain` lists the checkout on the recorded branch;
  the record stores a workspace-RELATIVE path (no host-absolute leak).
- `hostile_worktree_ids_are_refused_with_zero_side_effects` — 10 hostile
  shapes (traversal, separators, `..`, option-lookalikes, control chars,
  non-ASCII, oversize) all refused as usage-class (2) on both inspect and
  remove with zero record/checkout mutation; a well-formed unknown id is
  NotFound (5) with no host-path leak.
- `missing_worktree_finalizes_as_removed_by_its_owner` — out-of-band
  `git worktree remove --force` → inspect classifies `missing`; the OWNER
  finalizes to `removed` (terminal); re-removal is NotFound; a non-owner
  is refused even in Missing state (possession is never authority).
- `concurrent_removal_of_one_worktree_has_exactly_one_winner` — two
  independent processes race removal: exactly one exit 0, exactly one
  NotFound 5, record ends terminal `removed`, checkout gone.
- `unmanaged_git_worktrees_are_never_adopted` — a raw `git worktree add`
  inside the managed area never appears in AWH list; git still lists it;
  raw cleanup works.
- `worktree_start_points_are_validated_before_any_mutation` — a
  nonexistent `--from` ref fails (store-class 1) with no checkout, no
  half-written or corrupt records; a valid non-HEAD start point lands the
  worktree exactly at that commit.
- `worktree_creation_on_a_detached_head_repository` — after
  `git checkout --detach`, create still works, the worktree gets its own
  branch (not detached) at the detached commit.
- `worktree_exit_codes_are_stable_categories` — uninitialized workspace
  (2), non-git initialized workspace (2, nothing created), unknown id
  (5), same-session duplicate (4), invalid agent/session shapes (2,
  nothing persisted).

**Security regressions with permanent coverage**
- argv-only invocation: every hostile-message/branch/metachar test above
  would fail loudly if a shell string ever entered the spawn path.
- Command-injection sentinels: `pwned` / `api-pwned` files must never
  exist; `$(rm -rf /)` never executed.
- Traversal refusals at every plane with zero-side-effect proof.
- Missing-remote 500 body contains no credential-shaped material.
- Worktree record paths stay workspace-relative.

### Not implemented (honestly classified — §36)

Verified absent in the binary; classified rather than invented:

- **CLI `awh git` family** — `docs/CLI.md` documents
  status/diff/stage/commit/push/pull/reset/clean/validate, but NO `git`
  command family exists in `src/main.rs`. The documented subtree is
  ROADMAP. (Doc-vs-binary discrepancy reported in Prompt 17's status
  audit; unchanged here.)
- **`awh worktree merge` / `awh worktree status`** — roadmap only;
  the four implemented verbs (create/list/inspect/remove) are the
  complete surface, per `src/cli/worktree.rs`'s clap definition.
- **MCP push/pull tools** — the MCP plane deliberately does not expose
  push/pull (they exist only on the service and Control API planes).
- **High-risk ops on MCP/API planes** — reset/clean/force-push/
  branch-delete/discard have no ambient callers by design (deliberate
  methods only); verified again by grep this round.

### Unproven

None. Every implemented surface of the Git/worktree family has at least
one oracle-equality or behavioral-state test this round, or was already
pinned by the sibling suites (`tests/worktree_cli.rs` 12 tests,
`tests/acceptance_editing.rs` worktree isolation, `tests/mcp_builtin_
tool_gate.rs` trust gating, control.rs in-file tests) whose areas were
checked for overlap before this suite was designed (no duplication — this
suite is the gap increment).

### Failed → Fixed → Passed

Both defects above were found by this suite, fixed in the same branch,
and are now pinned by permanent tests:
- `commit_without_staged_changes_fails_without_creating_a_commit`
- (registry description corrected; read-only behavior pinned by
  `mcp_git_read_tools_match_the_oracle`)

### Blocked

None. Git 2.47+ present; all prerequisites available.

---

## Contract facts discovered while testing (pinned, do not "fix")

1. **git porcelain C-quotes paths with special characters**
   (`"staged new.txt"`). AWH's `porcelain_entries()` passes them through
   verbatim — byte-identical to git's own output. Consumers that join
   these paths must unquote; the contract is pinned as-is.
2. **`reset --hard` does not remove untracked files.** The suite pins the
   real git semantics (that is `clean`); the original draft of this suite
   mis-predicted it — the oracle corrected the test, not the product.
3. **`git status --porcelain` `??` is both columns**; an unstaged-only
   modification is ` M`. "Index empty" assertions must accept both.
4. **`git commit` exit-1 diagnostics can arrive on stdout with empty
   stderr** — the class of failure that hid defect 1. Any future
   exit-code gating must key on the exit code alone.
5. **`worktree list --porcelain` reports `worktree <path>` + `branch
   refs/heads/<name>` lines** — the persistence test matches these
   exactly, so a path-format change in the store breaks loudly.
6. **Well-formed unknown worktree ids are NotFound (5), hostile shapes
   are Invalid (2)** — the classification boundary is `is_safe_worktree_id`
   (prefix `wt-`, `[A-Za-z0-9-]`, ≤128 chars), and it runs BEFORE the
   record lookup in both inspect and remove.

## Test-engineering notes

- `git_failure_injection.rs` is its own test binary because PATH is
  process-global; a sibling test in the same binary could observe the
  mutated PATH (the same discipline AGENTS.md records for HOME). A
  `PathGuard` restores PATH even on assertion failure. Unix-only and
  honestly labeled: the wrapper is a shell script (§30 cross-platform
  honesty); the timeout code under test is platform-independent tokio.
- The oracle `git()` helper sets `GIT_CONFIG_NOSYSTEM=1` so runner-level
  system config cannot change what the oracle says.
- Branch names are resolved from git (`rev-parse --abbrev-ref HEAD`),
  never hard-coded `master`, so runner `init.defaultBranch` settings
  cannot break the suites.
- `Repo::call` runs each service call on a dedicated current-thread
  runtime with the service MOVED into the future (all GitService methods
  borrow `&self`).
- No test touches a network remote, a credential, or a path outside its
  temp dirs; push/pull use disposable bare repositories only.

## Completion criteria (§37) — status

| Criterion | Status |
|---|---|
| Implementation inspected | Yes (forensics at f332ebd; § sources cited in suite headers) |
| Every implemented Git operation meaningfully tested | Yes (status, diff, staged-diff, log, branch, branches, stage, unstage, commit, push, pull, + high-risk five) |
| Every implemented worktree lifecycle op tested | Yes (12 sibling + 9 gap tests) |
| Real repositories for acceptance | Yes (every test) |
| AWH vs independent git state | Yes (oracle in every test) |
| Branch/ref behavior | Yes |
| Stage/unstage/commit | Yes |
| Push/pull w/ disposable local remote | Yes |
| Destructive reset/clean safely tested | Yes (temp dirs, behavioral proof) |
| Containment & ownership | Yes (file-level isolation + ownership refusals) |
| create/list/inspect/remove | Yes |
| Crash/reconciliation where supported | Missing-state reconcile + concurrent finalize |
| Filesystem isolation between worktrees | Yes (file-level) |
| Concurrency where applicable | Yes (concurrent removal single-winner) |
| Failure side effects inspected | Yes (zero-side-effect loops; record well-formedness) |
| Config & limits | Yes (log clamp, timeout budget, id caps) |
| Security regressions covered permanently | Yes |
| Unimplemented functionality honestly classified | Yes (CLI git family, worktree merge/status, MCP push/pull) |
| No unrelated test master prompt modified | Yes |
