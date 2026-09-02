# Agent Workspace Hub (AWH) — Repo Memory

Rust implementation of the AI-native workspace runtime per
`/home/openhands/workspace/project/AWH_Master_Architecture_Implementation_Specification.txt`
(495 lines; the spec is the authority — section numbers are referenced below).

## Build & Validation Gates (run before every commit)

```
source ~/.cargo/env
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test --workspace      # 304 tests as of Phase 5
```

Git identity is NOT configured globally; commit with:
`git -c user.name="openhands" -c user.email="openhands@all-hands.dev" commit ...`
plus `Co-authored-by: openhands <openhands@all-hands.dev>` trailer.

## Architecture (spec-mandated)

- **Control API ≠ MCP** (spec §26): Control API (`src/api/control.rs`,
  `/api/v1`, axum) is for TUI/CLI/admin clients. MCP (`src/mcp/`, JSON-RPC
  stdio + SSE `/sse`+`/mcp`) is the agent-tool plane. Both wrap the same
  `src/services/` layer — keep it that way; do not let one call the other.
- **Repository-First Rule**: project state lives under workspace
  `projects/<name>/.agent/`; skills/MCP registries under user data dir.
- **Security**: services reject path traversal (`../`) and validate project
  names; auth via `mcp::auth` (`load_api_key`, `verify_token` constant-time,
  `bearer_token`); audit events via `mcp::audit` mirror into the shared
  bounded ring `services::audit::global()` (1000 entries, newest-first) and
  tracing stderr; served by `/api/v1/audit` and `/api/v1/logs`.
- **MCP stdio protocol**: stdout is JSON-RPC ONLY, tracing goes to stderr.

## Service Layer (`src/services/`) — all the Control API/TUI/MCP build on these

- `files::FilesService` — sync methods (`list/read/write/delete/rename/
  create_dir/search/meta`); reject traversal at this layer.
- `git::GitService` — async `status/log/commit/stage/unstage/diff/diff_staged`,
  `is_repo()` (async rev-parse) + `is_repo_blocking()` (`.git` exists; added
  Phase 5 for handlers). High-risk ops gated by `HighRiskGitOp`.
- `projects::ProjectsService` — sync `list/create/get/delete`; workspace is
  `Workspace::new(root)`, project store is `core::project::ProjectStore`
  (static methods, NOT the service).
- `terminal::TerminalService::run` — argv-only, 30s timeout, 256KB capture cap.
- `ListEntry`/`SearchHit`/`GitOutput`/`ExecOutcome`/`Project` are Serialize.

## Control API (Phase 5, commit 1676119)

- `ControlState { root, api_key, started, version }`; `build_router(state)`.
- Routes under `/api/v1`: `healthz` (PUBLIC, merged without auth layer),
  `status`, `projects` (+`/{name}`), `files`, `files/content`, `files/search`,
  `files/entry`, `git/status|log|diff|stage|unstage|commit`, `terminal/run`,
  `skills`, `mcp` (both read-only; secrets/commands/env omitted).
- Errors: `ApiError` → `{"error":{"code","message"}}`; internal errors are
  500 with generic message, full detail only in `tracing::error`.
- Non-repo git calls → 409 `not_a_git_repo` (via `open_repo()` helper).
- axum 0.8 route syntax: `"/projects/{name}"` (braces, NOT `:name`).
- `Router` isn't Copy — `app.clone().oneshot(...)` per request in tests.
- Middleware layer ordering matters: auth layer attaches to inner router,
  timeout/body-limit wrap the merged public+api router, then nest under
  `/api/v1`.
- CLI: `awh serve --host --port --api-key-env` (needs `AWH_API_KEY` set).

## Gotchas Learned (do not re-trip)

- TUI editor: `discard_changes` needs `goto(Editor)` first; UTF-8 backspace
  tests need cursor positioned before multibyte char; `line_col` after load
  returns (3,6) in the seeded fixture.
- git.rs TUI: scope borrows narrowly to avoid E0499; test fixtures need
  persistent git identity (`git config` in repo, not `-c` per-invocation).
- Files test fixture: seed 7 chars, must clear all before rename test.
- Cargo deps: axum 0.8, tower 0.5 (util), tower-http 0.6 already present —
  no new deps were needed through Phase 5 (spec: avoid unjustified crates).

## Phase Status

0-6 done (branch rust). Next: Phase 7 dashboard/status screens (the
audit ring `services::audit::global()` is available for recent
activity), Phase 8 context/memory/skills screens (`src/context/`
engines exist), Phase 9 tunnel + rate limiting (spec §25 chain:
Auth -> Authz -> Rate Limit -> Audit -> Services), Phase 10
hardening/docs.
