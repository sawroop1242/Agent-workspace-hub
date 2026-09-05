# Agent Workspace Hub (AWH) — Repo Memory

Rust implementation of the AI-native workspace runtime per
`/home/openhands/workspace/project/AWH_Master_Architecture_Implementation_Specification.txt`
(495 lines; the spec is the authority — section numbers are referenced below).

## Build & Validation Gates (run before every commit)

```
source ~/.cargo/env
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test --workspace      # 359 tests as of Phase 10
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
- Cross-platform CI gotchas (rust.yml runs tests on ubuntu/macos/windows):
  (1) never assert raw-path == canonicalize() — macOS resolves
  `/var`→`/private/var` and Windows yields `\\?\`-prefixed verbatim
  paths; compare canonical-to-canonical or raw-to-raw instead.
  (2) the `dirs` crate resolves home on Windows via the known-folders
  API (`SHGetKnownFolderPath`), NOT `HOME`/`USERPROFILE` env — tests
  needing a home directory must inject a root (`ControlState.
  global_skills_root`) rather than `set_var("HOME", …)`, which
  silently no-ops on Windows and races parallel tests everywhere.
  (3) tracing caches per-callsite Interest process-wide from the
  FIRST macro execution, evaluated against the registering thread's
  dispatcher (scoped `set_default` guards are invisible to sibling
  threads — they resolve to NoSubscriber → Interest::never). Tests
  asserting on tracing events must warm up all callsites, then
  `tracing::callsite::rebuild_interest_cache()` while holding their
  guard, then clear and re-emit (see mcp/audit.rs test for the
  pattern). Otherwise parallel sibling tests poison shared callsites.
- Cargo deps: axum 0.8, tower 0.5 (util), tower-http 0.6 already present —
  no new deps were needed through Phase 5 (spec: avoid unjustified crates).

## Phase Status

0-10 done (branch rust). Next: none — implementation plan complete;
future phases would be new spec work.
- **Registry semantics (QA-validated)**: two distinct planes. SKILL
  registries: URL is a BASE dir; client appends `/registry.json`
  ({name,version,skills:[{name,description,version,path,sha256}]}) and
  fetches packages at base+path; default = `DEFAULT_SKILL_REGISTRY`
  (.../rust/registry/skills; repo publishes it with the seeded
  `commit-hygiene` skill). MCP registries: URL is the FULL index.json
  ({schema_version,mcps:[…]}), fetched directly; default =
  `DEFAULT_MCP_REGISTRY` (.../mcps/index.json; seeded with the harmless
  `echo-helper` stdio entry). `skill install` verifies the manifest's
  sha256 against the downloaded bytes (`validate_sha256`), so a stale
  digest fails closed with "skill integrity check failed". CLI
  ghost-ID ops (skill read/uninstall, registry add-dup/remove-missing,
  mcp remove/enable/disable/uninstall) exit 1 via `bail!` — do not
  regress to println-and-exit-0.
- **GitService::log**: `git log` exits 128 for BOTH empty history and
  non-repo; log() now rev-parses `--git-dir` first and bails
  "not a git repository" instead of returning empty output.
- **Phase 10 — hardening**: three security fixes. (1) Rate-limiter
  bounded memory: `check()` prunes expired-key entries and caps
  distinct keys (default 10,000; `with_max_keys`), refusing unseen
  keys fail-closed once saturated — prune runs BEFORE the admission
  gate so expired windows free their slots. (2) ngrok authtoken now
  travels to the child via `NGROK_AUTHTOKEN` env, never argv
  (`/proc/<pid>/cmdline` is world-readable); CLI also reads
  `AWH_NGROK_AUTHTOKEN` so the token need not appear in `awh`'s argv
  either; `build_args` excludes the token, `child_env()` carries it.
  (3) Audit redaction at the choke point: `AuditLog::record` runs
  `redact_token_like` over subject/detail (≥16-char base62 runs →
  `[redacted]`; IPs/paths/short identifiers pass through), so no call
  site can leak token-shaped material into the ring. Live-verified:
  10,200 sprayed keys in one window → 10,000 admitted/200 429/RSS flat;
  fake-ngrok asserts token in env not argv; 359/359 tests, fmt+clippy
  clean.
- **Phase 9 — tunnel + rate limiting**: `src/tunnel/mod.rs` —
  `TunnelProvider` trait (`start/stop/status`) + `NgrokProvider` spawning
  the local `ngrok` binary via argv (never a shell); public URL resolved
  by polling the ngrok agent API `127.0.0.1:4040/api/tunnels`
  (`parse_agent_tunnels` prefers https, falls back to any public URL);
  failed start leaves no half-running child. CLI: `awh tunnel start
  --port/--provider/--ngrok-path/--ngrok-authtoken/--ngrok-region`
  (foreground, Ctrl-C stops, child killed on drop) and `awh tunnel status`
  (agent-API probe works across processes — no daemon/pidfile). A tunnel
  is transport, not auth: Control API keeps bearer auth behind it; the
  CLI warns when forwarding to non-loopback hosts.
- **Phase 9 — rate limiting**: `src/api/rate_limit.rs` — in-process
  sliding window (default 120 req/60s per client key), no new deps.
  Layer order in `build_router`: rate-limit layer added BEFORE the auth
  layer, so `authenticate` runs first (spec §25 chain) and 401s never
  consume quota. Key = `X-Forwarded-For` first value (ngrok/proxies set
  it; direct connections share the `direct` bucket). 429 responses carry
  `Retry-After` + structured `rate_limited` error and record an
  `api_rate_limit` deny in the audit ring (signature:
  `audit_deny(action, reason, subject)` — key is the SUBJECT, not
  reason). `/healthz` is on the public router — never throttled.
- **Phase 8**: Context/Memory/Skills screens (`src/tui/screens/{context,
  memory,skills}.rs`) ride `WorkspaceBackend` trait methods (scope =
  focused project or workspace root); API plane gained `/api/v1/context`
  (GET/PUT, 512 KiB cap), `/api/v1/memory` (GET/POST), `/api/v1/skills/
  project` (GET/POST/DELETE) with `store_scope` validating project names
  before path joins; mutations audited (`api_context_write`,
  `api_memory_append`, `api_skill_add`, `api_skill_remove`).
- **Env note**: the Rust toolchain can be wiped from this container
  between sessions; if `cargo` is missing reinstall with rustup
  (`--default-toolchain stable --profile minimal` then `rustup
  component add rustfmt clippy`).
