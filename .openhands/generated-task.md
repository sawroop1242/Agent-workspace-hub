Feature ID: AWH-AUTO-003

# Feature

Add AWH CLI smoke-test gate to autonomous delivery (backlog priority 3, status
`backlog`, dependency `AWH-AUTO-001` completed). The autonomous loop must not
merge a feature PR unless the real AWH CLI binary passes a deterministic
smoke-test gate in CI, including MCP stdio initialization and representative
tool calls.

# Goal

Introduce a CI smoke-test job that builds the `awh` CLI and exercises its core
commands (status, skill, registry, agent, policy subcommands) plus MCP stdio
initialization and representative MCP tool calls, and wire this gate into the
autonomous orchestration so Agent 1's autonomous merge (`awh_merge_guard.py` /
merge-checks in the loop workflows) requires it to succeed. The suite must be
deterministic and free of network dependencies: bind nothing external, make no
NVIDIA/GitHub API calls, and leave workspace state clean.

# Issue / Prompt Context

- `.openhands/backlog.json` AWH-AUTO-003 acceptance criteria:
  1. Exercise core CLI commands in CI before autonomous merge.
  2. Include MCP stdio initialization and representative tool calls.
  3. Keep the test suite deterministic and free of network dependencies unless
     explicitly required.
- `.openhands/state.json` is `PLANNING` with no active feature; `AWH-AUTO-003`
  was started once (`7cd53d6 chore(automation): start AWH-AUTO-003`) but never
  landed, so this is a fresh run.
- `docs/autonomous-development.md`: merge requires CI success plus an APPROVE
  review; all three agents share `moonshotai/kimi-k3` keys — no LLM calls
  belong in this gate.
- `docs/CLI.md` and `docs/mcp.md` are authoritative for CLI/MCP behavior.
- `docs/pr-reviews/agent3-v2.md`: deterministic checks establish facts; the
  smoke gate is a deterministic check, not an LLM review input.

# Existing Architecture

- CLI defined in `src/main.rs` (clap): `status`, `tui`, `serve`, `mcp`
  (`McpCommand`, default transport `stdio`), `skill`, `registry`, `agent`,
  `policy`, `tunnel`.
- MCP stdio server entry: `main.rs` → `serve_stdio()` (around `src/main.rs:363`).
- Existing Rust CI: `.github/workflows/rust.yml` (fmt, build-test matrix,
  clippy, audit) — keep intact.
- Autonomous loop: `.github/workflows/awh-*.yml` with scripts
  `scripts/awh_pipeline.py`, `scripts/awh_dispatcher.py`,
  `scripts/awh_merge_guard.py`, `scripts/checkpoint_state.py`,
  `scripts/safe_git.py`.
- Python orchestration state tests: `tests/*_test.py`, gated by
  `.github/workflows/checkpoint-state-tests.yml` (path-filtered).
- Existing smoke-style precedent: `docs/threat-model.md` live smoke sections.

# Files Likely Affected

- New: `.github/workflows/cli-smoke-test.yml` (or a job added to an existing
  gate workflow) implementing the smoke suite.
- New: `tests/cli_smoke_test.py` or a shell script under `scripts/` that runs
  the binary and speaks MCP JSON-RPC over stdio.
- `.github/workflows/awh-merge*.yml` / `scripts/awh_merge_guard.py` /
  orchestration workflows: add the smoke gate as a required check before
  autonomous merge.
- `.github/workflows/checkpoint-state-tests.yml` path list, if new Python
  tests are added for orchestration changes.
- `.openhands/state.json`, `.openhands/backlog.json` status fields per loop
  conventions (checkpoint commits only).

# Required Implementation

1. Add a callable/reusable smoke-test workflow (e.g. `workflow_call` or a job
   in the PR path) that:
   - Checks out the PR ref and builds the release/debug `awh` binary with
     `cargo build` (cache appropriately).
   - Runs `awh --help`/parse checks and core commands that are safe offline:
     `awh status`, `awh skill list`, `awh registry list` (or equivalent
     read-only variants), asserting exit codes and key output markers.
   - Starts `awh mcp serve` (stdio) as a subprocess and performs JSON-RPC:
     `initialize` → `notifications/initialized` → `tools/list`, then at least
     two representative `tools/call` invocations on local/non-network tools;
     assert protocol version handshake and non-error responses.
   - Runs offline: no outbound network, no LLM keys, temporary `HOME`/workspace
     isolated; bound ports (if any) restricted to loopback.
2. Wire the gate into autonomous delivery: the merge guard/check verification
   used by Agent 1 must treat this workflow's success on the PR head SHA as
   required, alongside existing CI and the Agent 3 APPROVE.
3. Add/extend tests for any Python orchestration changes
   (`tests/awh_merge_guard_test.py` or new file) and keep them in the
   checkpoint-state path filter.
4. Do not modify Rust CLI behavior unless a command is genuinely broken for
   smoke use; any such fix must be minimal and justified in the PR.
5. Keep existing workflows (`rust.yml`, `release-rust.yml`, `main.yml`,
   `checkpoint-state-tests.yml`, `awh-*.yml`) intact; never weaken CI gates,
   security controls, or error handling.

# Acceptance Criteria

- A CI job builds the real `awh` binary and exercises core CLI commands,
  failing the pipeline on non-zero exit or contract mismatch.
- The same job initializes an MCP stdio session (`initialize` +
  `notifications/initialized`) and performs representative `tools/list` and
  at least two `tools/call` invocations successfully.
- The suite is deterministic and network-free (runs without `AWH_*` LLM keys,
  GitHub tokens, or external endpoints), on ubuntu at minimum.
- The autonomous merge path (`scripts/awh_merge_guard.py` / merge gate in
  `awh-*.yml`) fails closed if the smoke gate did not pass on the exact PR
  head SHA.
- New/updated orchestration logic has Python unit tests where logic is added;
  all existing tests still pass.

# Verification Commands

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`
- `python -m pytest tests/ -q` (or `python scripts`-style test runner used by
  `checkpoint-state-tests.yml`)
- Local smoke dry-run, e.g.:
  - `cargo build && ./target/debug/awh status`
  - `printf '{"jsonrpc":"2.0","id":1,"method":"initialize",...}\n...' | ./target/debug/awh mcp serve` and assert valid JSON-RPC responses
- `python -c "import yaml,sys; [yaml.safe_load(open(f)) for f in sys.argv[1:]]" .github/workflows/*.yml` (workflow syntax sanity)

# Non-Goals

- No changes to Rust source behavior, agents' LLM key handling, or the
  reviewer (`agent3-v2`) deterministic engine beyond what the gate requires.
- No new network-dependent tests, no integration with NVIDIA/GitHub APIs.
- No removal or weakening of any existing CI job, security control, or
  error handling.
- No live/destructive smoke (no `serve` on public interfaces, no tunnel, no
  remote TUI).
- No backlog/status bookkeeping beyond the loop's normal checkpoint commits.
