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

Sandbox note: fresh containers have NO Rust toolchain preinstalled -
install with rustup (`curl https://sh.rustup.rs | sh -s -- -y`), then
`source ~/.cargo/env`; `rustup component add clippy rustfmt` for gates.
`cargo test --all-targets` takes >4 min here; run it in the background
(`(cargo test --all-targets > log 2>&1 &)`) - foreground terminal is
capped at ~1080s; full run is ~988 tests as of 2026-09.

Prompt 01 (TW-001 init) verified COMPLETE: `src/services/init.rs` is the
single bootstrap owner (no duplicates; grep `workspace.json` first).
Contract: idempotent (byte-stable manifest), fail-closed on corrupt /
unsupported-version / foreign-root / invalid-workspace-id manifests and
corrupt `policy.json`, canonical-root binding, StoreLock-serialized +
atomic-rename manifest creation. Tests: 11 in `services::init` unit tests,
11 in `tests/init_cli.rs` (real compiled binary; 8-thread parallel race
asserts exactly one Created + one canonical identity).
`load_workspace_manifest` on a *nonexistent* root errors with
"failed to resolve" (not "not initialized") - both fail closed; test the
friendly message only against an existing-uninitialized dir.

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

0-11 done (branch rust; Phase 11 via PR #10, CI green on all 3 platforms).

- **Phase 3 policy - workspace-local DENY-only rules (on top of PR #19's
  built-in gate)**: PR #19 routes every Medium/High built-in tool through
  `authorize_builtin(...)` - a coarse, workspace-independent category gate
  via `awh.builtin` (per-user-machine trust). This phase adds a SECOND,
  narrower check for exactly the three tools whose call carries a meaningful
  resource: `workspace.write_file` (`path`), `workspace.delete_file`
  (`path`), and `terminal.run` (`program`). New `PolicyStore`
  (`src/core/policy.rs`) persists ALL rules as a single JSON array at
  `<workspace>/.agent/policy.json` - the ONE deliberate deviation from the
  `AgentStore`/`CapabilityGrantStore` one-file-per-record pattern (single
  hot-path read via `matching()`). `matching(tool, resource)` returns the
  first rule for that exact tool whose pattern matches: forward-slash
  relative-path PREFIX for the two `workspace.*` tools, EXACT case-sensitive
  match for `terminal.run` - no globs either way (v1 limitation, documented).
  New `src/models/policy_rule.rs` (`PolicyRule { id, tool, pattern, reason,
  created_at }`), `PolicyDenialError` in `src/mcp/error.rs`, JSON-RPC code
  `POLICY_DENIED_CODE = -32004` (after `BUILTIN_TOOL_DENIED_CODE = -32003`).
  Dispatcher gains a `policy: PolicyStore` field loaded from the workspace
  root (per-workspace, NOT home dir) + `with_policy_store` builder + private
  `authorize_policy(tool, resource)` that emits `policy_denied`/
  `policy_allowed` audit actions (distinct from Phase 2's `builtin_tool_denied`)
  and returns `-32004` on match. EXACTLY three call sites (write_file/
  delete_file/terminal.run) immediately after their `authorize_builtin` call,
  BEFORE the service runs; the ~34 other `authorize_builtin` arms and
  `execution_gate.rs` are untouched. CLI `awh policy deny|list|remove` mirrors
  `handle_agent_cli` (std::env::current_dir(), auto id = `policy-` + tool+
  pattern SHA-256 prefix). Deny-only: zero rules = byte-identical to Phase 2
  (`no_policy_rules_leaves_all_three_tools_working`). Fail closed on an
  unreadable store (internal error, not a false allow). No Allow rules/
  approval/globs/hierarchy - out of scope. Tests: `tests/mcp_policy_gate.rs`
  (11, spawns real `awh` + real dispatcher).

- **Phase 3 policy — workspace-local DENY-only rules (on top of PR #19's
  built-in gate)**: PR #19 routes every Medium/High built-in tool through
  `authorize_builtin("aws.../workspace.*")` — a coarse, workspace-independent
  category gate via `awh.builtin` (per-user-machine trust). This phase adds a
  SECOND, narrower check for exactly the three tools whose call carries a
  meaningful resource: `workspace.write_file` (`path`), `workspace.delete_file`
  (`path`), and `terminal.run` (`program`). New `PolicyStore`
  (`src/core/policy.rs`) persists ALL rules as a single JSON array at
  `<workspace>/.agent/policy.json` — the ONE deliberate deviation from the
  `AgentStore`/`CapabilityGrantStore` one-file-per-record pattern (single hot-
  path read via `matching()`). `matching(tool, resource)` returns the first
  rule for that exact tool whose pattern matches: forward-slash relative-path
  PREFIX for the two `workspace.*` tools, EXACT case-sensitive match for
  `terminal.run` — no globs either way (v1 limitation, documented). New
  `src/models/policy_rule.rs` (`PolicyRule { id, tool, pattern, reason,
  created_at }`), `PolicyDenialError` in `src/mcp/error.rs`, JSON-RPC code
  `POLICY_DENIED_CODE = -32004` (after `BUILTIN_TOOL_DENIED_CODE = -32003`).
  Dispatcher gains a `policy: PolicyStore` field loaded from the workspace
  root (per-workspace, NOT home dir) + `with_policy_store` builder + private
  `authorize_policy(tool, resource)` that emits `policy_denied`/`policy_allowed`
  audit actions (distinct from Phase 2's `builtin_tool_denied`) and returns
  `-32004` on match. EXACTLY three call sites (write_file/delete_file/terminal.
  run) immediately after their `authorize_builtin` call, BEFORE the service
  runs; the ~34 other `authorize_builtin` arms and `execution_gate.rs` are
  untouched. CLI `awh policy deny|list|remove` mirrors `handle_agent_cli`
  (std::env::current_dir(), auto id = `policy-` + tool+pattern SHA-256 prefix).
  Deny-only: zero rules = byte-identical to Phase 2 (`no_policy_rules_leaves_
  all_three_tools_working`). Fail closed on unreadable store (internal error,
  not a false allow). No Allow rules/approval/globs/hierarchy — out of scope.
  Tests: `tests/mcp_policy_gate.rs` (11, spawns real `awh` + real dispatcher).
**PR #15 (mcp-protocol-hardening) — MCP protocol hardening round**: landed
per-hook `catch_unwind` isolation in `McpHooks::fire` (panic recorded via
tracing with `method_hint`, hook skipped, registry stays usable — pinned
end-to-end: a panicking hook cannot change any dispatch outcome);
`RpcRequest` gained `id_present` via custom `Deserialize` (absent id =
notification silence; PRESENT id even `null` = request; `id:null` → -32600
per MCP, echoed id null); `SessionState` reduced to atomic-backed
New/Ready/Closed/Failed; `mark_initialized()` returns whether THIS call did
the New→Ready CAS, and the initialize completion path consumes it so
concurrent duplicate initialize deterministically loses with -32600
(one-winner pinned by `tokio::join!` test); `resources/read` validates
`awh://<kind>[/single-segment-id]` at the boundary (256-byte cap, no
`/` `\` `..` `%` NUL control chars; context takes no segments);
`tool_metadata` unknown fallback is explicit `"uncategorized"` (never
silently "workspace"); `mcp.status` reports `"running"` (process state,
NOT subsystem health); metrics u128→u64 saturating. Test counts:
mcp_protocol 32 (was 15), workspace 481 total. SDK interop evidence (not
committed; reproducible with `@modelcontextprotocol/sdk` TS client): stdio
StdioClientTransport 8/8, SSE `SSEClientTransport` (note the export name is
SSEClientTransport, class in client/sse.js) 10/10 over `GET /sse` →
endpoint event → `POST /mcp` with `mcp-session-id`; stdout stays 0 bytes
over a full session. `/mcp` POST without session header → 400 "missing
session id" (AWH implements the legacy SSE transport, NOT sessionless
Streamable HTTP — use SseClientTransport in interop tests).
Schemas deliberately allow extra well-typed fields (no
`additionalProperties:false` anywhere) — documented in docs/mcp.md.
`McpDispatcher` now derives Clone (all-Arc fields; doc always claimed
cheap-to-clone). Tool catalog 53 static.

- **Phase 11 — GitHub provider + gaps**: `src/mcp/github.rs` (12
  `github.*` tools, direct REST, gated on `GITHUB_TOKEN`; `GITHUB_API_URL`
  overrides base for GHES; target resolution explicit args → origin
  remote via `GitService::remote_url` → `GITHUB_DEFAULT_OWNER/REPO`,
  sources never mixed — half-specified pairs resolve as a pair or fail
  closed). Dispatcher: `github: Option<Arc<GithubProvider>>`,
  `github_tool_schemas()` is a THIRD json! array (recursion limit —
  never fold into the existing two), audit action is `github_invoke`
  (subject=tool, detail=owner/repo; never bodies). Also added
  `workspace.write_file` (5 MiB cap, atomic, `safe_new_path` — canonicalize
  deepest existing ancestor then join; symlink-out rejected),
  `workspace.delete_file` (Ok(false) missing), `tasks.get`/`connectors.get`
  (`get(&str) -> Result<Option<_>>`, missing → null not error). Catalog:
  53 core / 65 with github.*; docs/mcp.md + configuration.md + README
  counts all updated together. Testing gotchas: dispatcher tests inject a
  stubbed provider via private field (no env mutation, no races); the
  stub's tokio runtime must be leaked (`std::mem::forget`) or it dies with
  the helper; MCP SDK's StdioClientTransport SANITIZES env by default —
  pass `env: {...process.env}` when a test needs GITHUB_TOKEN to reach the
  child. Secret auto-injection only exports a key when the command text
  names it, and a wiped container loses `~/.cargo` mid-session (reinstall
  via rustup; `source ~/.cargo/env` per shell).
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
- **Phase 12 — MCP hardening (Ruflo-informed, AWH-native)**: implements only
  concepts that materially improved AWH's MCP surface. (1) `SessionLifecycle`
  explicit state machine (Uninitialized→Initializing→Ready; `-32002` for
  pre-init requests, `-32600` duplicate initialize; notifications silent at
  every state incl. invalid ones). (2) `SUPPORTED_PROTOCOL_VERSIONS` const
  (dispatcher.rs:89) + version negotiation: echo client version if
  supported, else fall back to latest; unknown → fallback NOT error (spec
  allows either; test pins the choice). (3) Schema-first argument
  validation BEFORE handlers run (`src/mcp/schema.rs`, depth-capped 32):
  wrong type/unknown field/missing required/non-string enum/
  additionalProperties=false → standardized `-32602` naming the failing
  field — this FIXED a real bug where `workspace.read_file {path: 42}`
  panicked-ish into -32603 instead of -32602. (4) Tool metadata: every
  `tools/list` entry carries `category` + `version` (json! third arrays per
  family — watch dispatcher.rs recursion limits, never fold them). (5)
  `mcp.status` tool (category "system") + `ToolMetrics` bounded fixed-size
  map (no per-tool-name allocation from untrusted input; avg duration via
  checked_div). (6) `McpEvent` observer-only hooks in `src/mcp/
  observability.rs` — hooks CANNOT veto/mutate/reorder; panicking hook is
  contained (catch_unwind) without breaking the registry. REJECTED from
  Ruflo: agent-runtime/policy/tool-broker architecture (premature per spec
  hard constraint), tool result streaming/cancellation, dynamic
  registration. Test gotchas: (a) tools/call results are wrapped in MCP
  `content` text envelope (dispatcher.rs:1831) — parse `result.content[0]
  .text` then serde_json::from_str; (b) mcp.status can't count itself —
  metrics record AFTER serialization; call another tool first then assert
  `metrics.tool_calls >= 1`; (c) notifications MUST omit `id` — sending
  `id` with notifications/initialized yields -32601 (JSON-RPC req); (d)
  `StdioClientTransport` sanitizes env — pass `{...process.env, HOME}` in
  interop harness; (e) binary stdio tests: `ChildStdin` lacks
  Default/take-once — hold it as `Option<ChildStdin>`, set `server.stdin =
  None` to close for EOF shutdown test; (f) initialize needs clientInfo
  (client SDK always sends it — hand-rolled JSON must too); (g) container
  wiped `~/.cargo` MID-SESSION after 460 green tests — rustup reinstall
  (`--profile minimal`, add rustfmt+clippy) then `source ~/.cargo/env`
  works; SSE interop needs self-signed certs at /tmp/awh-tls (README
  documents openssl command) — after reinstall, `cargo build --release`
  before `node examples/mcp-interop/*.mjs` (harness spawns the release
  binary). Suite now: 461 tests (385 lib + 15 protocol + 3 executable +
  3+3+13 integration + 39 doc-adjacent), interop: stdio+SSE harnesses pass
  with 53 tools advertised (no GITHUB_TOKEN).
- **Env note**: the Rust toolchain can be wiped from this container
  between sessions; if `cargo` is missing reinstall with rustup
  (`--default-toolchain stable --profile minimal` then `rustup
  component add rustfmt clippy`).
- **Custom MCP HTTP headers (Phase 12, PR #16, f072414)**:
  `CustomMcpServerConfig.headers` map + `awh mcp add --header NAME=VALUE
  --secret NAME`. Names = RFC 7230 tokens, values control-char-free
  (CRLF injection rejected at config time), all values via
  `expand_secret_ref` (`${secret:NAME}` resolves only with BOTH secrets
  AND environment permission — `McpPermissions::validate` requires
  every secret to also be an allowed env name, so `--secret` grants the
  pair). Fail closed at registration, never send literal refs upstream.
  Gotchas: (1) test env-var manipulation needs globally-unique names
  (AWH_TEST_HEADER_SECRET) to survive parallel siblings;
  (2) `unwrap_err()` needs Debug on the Ok type — use `match` for
  non-Debug clients; (3) a shell-exported COMPOSIO_API_KEY leaks into
  `cargo test` and makes dispatcher tests hit the live Composio backend
  (401) — unset before testing; (4) repo-root `.agent/` is gitignored
  (anchored `/.agent/` so `examples/mcp-interop/.agent` fixtures stay
  tracked) since `--header` can carry raw credentials in other setups;
  (5) Composio hosted MCP key only works on
  `connect.composio.dev/mcp` (x-consumer-api-key header), NOT on the
  backend API the native `ComposioProvider` uses — register the hosted
  endpoint as a custom server instead.
- **Toolchain-reinstall trap**: rustup minimal profile lacks
  `cargo-fmt`/`cargo-clippy` shims until `rustup component add rustfmt
  clippy`; verify with `cargo --version` before trusting rustup logs.

- **Custom MCP HTTP headers (Phase 12, PR #16, f072414)**: `CustomMcpServerConfig.headers` map + `awh mcp add --header NAME=VALUE --secret NAME`. Names = RFC 7230 tokens, values control-char-free (CRLF injection rejected at config time), all values via `expand_secret_ref` (`${secret:NAME}` resolves only with BOTH secrets AND environment permission — `McpPermissions::validate` requires every secret to also be an allowed env name, so `--secret` grants the pair). Fail closed at registration, never send literal refs upstream. Gotchas: (1) test env-var manipulation needs globally-unique names (AWH_TEST_HEADER_SECRET) to survive parallel siblings; (2) `unwrap_err()` needs Debug on the Ok type — use `match` for non-Debug clients; (3) a shell-exported COMPOSIO_API_KEY leaks into `cargo test` and makes dispatcher tests hit the live Composio backend (401) — unset before testing; (4) repo-root `.agent/` is gitignored (anchored `/.agent/` so `examples/mcp-interop/.agent` fixtures stay tracked) since `--header` can carry raw credentials in other setups; (5) Composio hosted MCP key only works on `connect.composio.dev/mcp` (x-consumer-api-key header), NOT on the backend API the native `ComposioProvider` uses — register the hosted endpoint as a custom server instead.
- **Toolchain-reinstall trap**: rustup minimal profile lacks `cargo-fmt`/`cargo-clippy` shims until `rustup component add rustfmt clippy`; verify with `cargo --version` before trusting rustup logs.
- **AGENTS.md is not pure UTF-8** (a 0xd1 byte near offset 8098 makes strict decoders fail); append via python/shell, not the file editor.

- **AWH automation safety layer (PR #51, branch awh-automation-safety-layer)**: all 5 awh-* workflows now mutate the checkpoint only through `scripts/awh_pipeline.py` (composes checkpoint_state CAS + safe_git push; no workflow re-implements sync). Checkpoint schema is v4 (recovery_attempt, recovery_claim, active_pr_sha staging for next-increment immutable review SHA). Operation ids are `FEATURE:STAGE:N`; begin-stage retries are idempotent (same id re-enters, never increments). Recovery = atomic claim/lease (CAS `<stale> -> RECOVERING` with owner/attempt/timestamp) -> decide from GitHub reality -> finalize; racing recovery jobs cannot both win. Events are notifications only: validate-event exits 3 on mismatch, never mutates state. record-failure redacts ghp_/nvapi-/sk- tokens. Contract tests ban force-push/reset --hard/un-CAS'd writes and duplicated sync logic. Gotchas: (1) `pull_request_target` workflows evaluate from the BASE branch, so the broken old reviewer on rust fails every PR's `review` check until this merges - pre-existing, not a PR regression; (2) workspace clone may be SHALLOW - `git merge-base` "no ancestor" is an artifact, `git fetch --unshallow` first; (3) live automation pushes to rust while you work - merge origin/rust in, never rebase/force; resolve .openhands/state.json conflicts to the v4 file; (4) pytest+pyyaml live in ~/.local, use `python -m pytest`; (5) pycache regenerates on every run - .gitignore has __pycache__/ now.

- **Recovery-claim fail-closed push (PR #51 follow-up)**: recovery claims no longer publish through safe_git.push (fetch/rebase/retry) - a losing claimant could rebase its claim onto the winner's and republish. `safe_git.push_claim(repo, remote, branch)` (single `run_git push HEAD:branch`; raises `LostClaimError` on non-fast-forward, `GitError` otherwise; NEVER fetches/rebases/retries) is the claim's only publish path. `awh_pipeline.claim_recovery(*, feature_id, expected_operation_id, claim_owner)` syncs (ff-only) to origin/rust, validates the claim against the REMOTE state it observed (identity + staleness + RECOVERING lease logic), CAS-transitions, commits locally via `stage_and_commit`, then push_claim; the LostClaimError handler is pure standdown. Result token printed as last stdout line (CLAIMED/LOST_CLAIM/NO_OP), parsed by the recovery workflow's case statement: LOST_CLAIM/NO_OP exit green WITHOUT dispatching or record-failure (a lost claim is not a failure); all downstream steps gated on `steps.claim.outputs.result == 'CLAIMED'`. Normal checkpoint writes (stage_and_push -> safe_git.push) keep fetch/rebase/retry. Tests: two-worker same-base race (in-process B + subprocess A, A's push injected inside B's observation window - loser's commit provably absent from origin, parent still base, no rebase dirs) and the never-rebases regression (instrumented safe_git loader: exactly one push_claim call, zero rebase, one sync). Gotchas: heredocs with blank lines misparse in this terminal - write helper .py files instead; pytest/rustup live in fresh containers, reinstall (pip install pytest pyyaml; rustup minimal + rustfmt/clippy components); module-load tests must monkeypatch AWH_REPO_ROOT BEFORE load_pipeline() since REPO_ROOT is resolved at import time.


- **TW-001: awh init + runtime identity (issue #73, 2026-09)**: `awh init [--path DIR]` implemented in `src/services/init.rs` (service) + `src/main.rs` (CLI). Durable workspace manifest `.agent/workspace.json` = `{version, workspace_id, workspace_root, created_at}` (MANIFEST_VERSION=1). `initialize_workspace` is idempotent (AlreadyInitialized never rewrites; re-init preserves manifest bytes, agents, grants, policy rules). Fails closed: file-as-root, corrupt JSON, unsupported version, manifest recorded for another root (copied dir) are explicit errors and never reset state; corrupt policy.json blocks init. Uses StoreLock (manifest lock + separate policy-store lock) + NamedTempFile fsync/persist atomic writes. Typed IDs in `src/core/identity.rs`: WorkspaceId/AgentId/SessionId/TaskId/AuditEventId (macro-generated, `ws-`/`agent-`/`sess-`/`task-`/`audit-` prefixes, transparent serde, `new_checked` validation); SessionIdentity binds session->agent+workspace; `resolve_session_relation` returns UnknownAgent/AgentInactive/WrongWorkspace/ActiveInWorkspace (identity only, never authority). EditId/SnapshotId NOT duplicated. `load_workspace_manifest` is the reload entry point. Tests: tests/init_cli.rs (8 CLI/service tests incl. restart + foreign-manifest rejection), unit tests in identity.rs/init.rs; 982 tests total, all gates green. Gotchas: (1) tempdir `current_dir` must exist before spawning the awh binary in tests; (2) manifest stores the CANONICAL root so a copied .agent/ dir is detected as foreign; (3) don't canonicalize --path in main.rs - the service owns resolution.

- **TW-002: agent runtime identity (AGENT-001, 2026-09)**: `AgentRuntimeService` (`src/services/agent_runtime.rs`) is the SINGLE shared boundary for agent profiles + AWH-native runtime sessions — every surface (CLI/MCP/TUI/API) must call it, never implement lifecycle logic locally. AgentProfile = existing `models::Agent` + `enabled: bool` (serde default true, so legacy JSON stays enabled); `AgentStore.register()` rejects duplicate ids (unlike `create()`, a compatibility upsert — keep both); `AgentStore.get()`/`SessionStore.get()` FAIL CLOSED (`Ok(None)`) on unsafe ids (traversal/separators/control chars, len caps `MAX_AGENT_ID_LEN`=64 / `MAX_ID_LEN`=128) — reads never escape `.agent/agents/` or `.agent/sessions/`; the id is the ONLY filesystem identifier, display names never touch paths. Sessions: `AgentSessionRecord` (`models/session.rs`) persisted one-JSON-per-session under `.agent/sessions/`, ids from `SessionId::new()` (`sess-` prefix), bound to the workspace manifest id at open; `session open` requires an initialized workspace (manifest binding) while `agent create` stays pre-init compatible. Lifecycle table in `core/sessions.rs` (`is_valid_transition`): Active<->Paused, anything->Stopped/Failed; Stopped/Failed are TERMINAL — never silently reactivated, open a new session. `resolve_session` re-validates EVERYTHING on every call (session exists, ownership vs claimed agent, usable status, workspace binding vs manifest, profile enabled+active via `IdentityRelation`, which now has `AgentDisabled` distinct from `AgentInactive`); identity resolution is never authority — capabilities/policy stay separate. `transition_session` resolves ownership FIRST, so a stopped session is rejected with "cannot be used" before the store table is consulted. CLI: `awh agent show/start [--all]/stop/restart/status/enable/disable` + `awh agent session open/list [--agent]/show/resolve/pause/resume/stop`; existing create/list/inspect/grant/revoke outputs preserved byte-identical. MCP protocol sessions (`SessionLifecycle`, mcp/dispatcher.rs) are transport-only and stay SEPARATE — no agent-specific MCP endpoint routing (Prompt 03 scope). Gotchas: (1) StoreLock is NOT reentrant — `transition()` holding a lock then calling `create()` (locks the same target) self-deadlocks to the 10s timeout; the store writes via lock-free internal `write_record()` under its own held lock — never nest StoreLocks on the same target; (2) create the sessions dir BEFORE `StoreLock::acquire` (lock-file creation ENOENTs on a missing parent); (3) serde error text for corrupt JSON varies — assert on stable substrings; (4) name/role are free text bounded 128/64 bytes. Tests: 12 service unit tests (incl. 8-thread parallel session creation: unique ids, no leakage, workspace-bound), store/identity unit tests, 9 e2e CLI tests in tests/agent_runtime_cli.rs; 1015 tests total, all gates green. Also repaired 9 pre-existing truncated em-dash bytes in this file (lone 0xd1) that made AGENTS.md invalid UTF-8.

- **Prompt 15 (ARCH-001) service/store convergence (2026-09)**: one canonical
  boundary per domain; interfaces are adapters. (1) Memory: `core::memory`
  `MemoryStore` IS the canonical store (rich `MemoryEntry` id/scope/tags/
  created_at/updated_at, `.agent/memory.json`, StoreLock, atomic save,
  destructive-once legacy `.agent/memory.jsonl` migration preserving line
  order); `mcp/memory.rs`+`mcp/tasks.rs` are 9-line re-export shims
  (`pub use core::memory::... as MemoryMcp`, `core::tasks::... as TasksMcp`)
  kept only so dispatcher import paths stay stable — never grow them back.
  Control API `/api/v1/memory` and TUI `WorkspaceBackend::list_memory` now
  serialize the SAME entry shape (API consumers see created_at, not the old
  `timestamp` field). (2) Tasks: `core::tasks::TaskStore` is canonical
  (`.agent/tasks.json`, Todo/InProgress/Blocked/Done); old one-file-per-task
  `models/task.rs` + `models/memory.rs` DELETED; `models/mod.rs` re-exports
  Agent/CapabilityGrant/PolicyRule/Project/session records only. (3) Files:
  `mcp/workspace.rs` is a thin adapter over `FilesService` (gained
  `delete_if_exists` -> bool + `write_atomic`); MCP traversal errors now say
  "path traversal is not allowed" (service text), not the old adapter text.
  (4) TUI `delete_project` routes through `ProjectsService` (TUI only audits).
  (5) `context/engine.rs`+`planner.rs` construct `core::memory::MemoryStore`
  directly — core must never import through `crate::mcp::`.
  Convergence pinned by `tests/store_convergence.rs` (7 tests: cross-plane
  write/read/update/restart for memory+tasks+files, alias-is-canonical,
  legacy jsonl migration). Gotchas: MCP wire enums are PascalCase
  ("Project"/"Global", "Todo"/"InProgress") — Phase 12 schema validation
  rejects lowercase BEFORE parse_scope sees it; `tools/call` needs an
  initialized SessionLifecycle + full-grant `awh.builtin` trust store in
  integration tests; MCP global-scoped memory stays in the PROJECT file
  (scope is a field, not a separate store); `memory.get` of a missing id
  returns JSON `null`, not an error. `TaskStore::create` takes
  (id,title,description,priority,tags) — all owned Strings.

- **FS-001 filesystem coordination (2026-09, branch fs-001-filesystem-coordination, commit fbcc42b)**: `src/core/fs_coordination.rs` — FsCoordinator; resource key = SHA-256(canonical root + normalized relpath) (spec-mandated; NEVER absolute-path keys). Two tiers: in-process registry (per-key FIFO queue, bounded) + cross-process zero-byte lock files in SYSTEM TEMP (`env::temp_dir()/awh-fs-coordination/`), stale-reclaim only past 30s. Sorted deterministic acquisition; 10s bounded timeout; MAX_LOCKS_PER_SET fail-closed. Wired: FilesService write/write_atomic/delete/rename/create_dir acquire->revalidate-under-lock->mutate (`*_locked` variants for caller-held spans); EditService commit_verified (own set) / patch_internal (one set across commit phase via commit_verified_locked) / attempt_rollback (`_locked` under caller set) / rollback_edits_inner (its OWN set — locks never survive across ops); WorkspaceMcp write_file/delete_file (same key derivation -> TUI/API/MCP converge). Audit = canonical `record_outcome` action `filesystem.coordination` + reason codes, no new store. Gotchas: (1) lock files must NOT live under `.agent/fs-coordination` — they pollute TUI listings and git status of uninitialized workspaces (4 TUI tests broke); system temp + root-hash key keeps workspaces byte-clean; (2) dangling final-component symlink escaped resolve_checked (Path::exists FOLLOWS links, so ancestor walk settled on root; fs::write then followed the link outside) — fixed with symlink_metadata chain-following, bounded 8, loops fail closed; MCP plane was safe (rename/unlink never follow the final link) — FilesService::write was NOT; (3) EditTransaction has 7 fields — construct via EditTransaction::new/single then assign `.expected`; (4) rerun locks: `tests/fs_coordination.rs` 20 integration tests incl. real same-file race (assert exactly one marker in final content), overlapping multi-file A=[a,b] vs B=[b,c] deadlock hammer, timeout >= 10s assertion. Suite: 1191 tests green (955 lib + 20 fs_coordination + rest).

## Prompt 16 (AWE-015/AWE-017/TW-008) - editing acceptance + Fs audit defect fix (PR #126)

- **Defect found by acceptance mapping**: the `Fs` CLI arm of `src/main.rs` never called
  `services::audit::init_global`, so `awh fs` edit/rollback/conflict events landed only in the
  process-local ring and were lost on exit (serve arms did init; fs did not). Fixed with the
  same degraded-mode discipline (log audit_init_failed, keep running). Any future CLI arm that
  emits auditable events must init_global first - check every arm when adding one.
- **Acceptance suites** (tests/acceptance_editing.rs 8 + tests/acceptance_edit_gates.rs 3) pin:
  cross-interface parity (EditService in-proc / real CLI binary / real MCP stdio binary all
  byte-identical for the same replace + rollback; stale-hash conflict exit 4 on the two planes
  that express ExpectedState - the MCP filesystem.replace tool schema deliberately does NOT
  expose expected-state args), exact-byte matrix incl. CRLF/mixed/emoji/no-eol/empty/
  valid-UTF-8-binary-like with byte-level rollback, MCP restart across 3 processes,
  unix symlink escape (file + parent shapes), corrupted recovery blob fails closed (exit 5),
  copied-.agent recovery rejected across workspaces, worktree edit isolation, durable
  correlated audit.
- **Contract facts discovered while testing** (now pinned, do not "fix"):
  (1) the edit plane is a TEXT plane - replace on a non-UTF-8 file is refused cleanly
  ("target file is not valid UTF-8 text", bytes preserved, no residue); NUL/control bytes are
  valid UTF-8 and in-contract;
  (2) durable audit.log lines are CHECKSUMMED ENVELOPES `{"checksum", "event"}` - unwrap
  `["event"]` before reading action/kind/correlation fields; the event carries workspace_id +
  edit_id correlation;
  (3) the SEC-001 built-in gate pins version `"local"` (BUILTIN_TOOL_TRUST_VERSION in
  mcp/execution_gate.rs) - trust approvals must use exactly "local" or they are
  trust-level/version denials (tests must mirror mcp_builtin_tool_gate.rs's helper);
  (4) NO `awh.builtin` record = Medium tools allowed (documented opt-in default); the denial
  case is a record that EXCLUDES the Filesystem category - assert with a positive control;
  (5) git worktree checkouts that contain a parent's .agent/workspace.json are (correctly)
  REFUSED by `awh init` foreign-root binding - worktree test fixtures must git-commit the base
  BEFORE running `awh init` at the parent, or every checkout carries the foreign manifest.
- Test-harness gotchas: in-process dispatcher tests use `dispatch_with_lifecycle` +
  SessionLifecycle (no transport); agent-route tests build AppState{dispatcher, sessions,
  api_key, ...} and SessionRegistry::create_with_binding - mirror tests/mcp_agent_routes.rs
  exactly; `read_response` blocks on a line read, so a hung server hangs the test (accepted,
  same as tests/mcp_executable.rs); grep the payload field names before asserting (fs --
  json prints {"command","edit_id","path","status"}).

- **Prompt 17 (AWE-018/AWE-019, PR #127, branch awe-018-019-contract-status)**: the
  artifact IS the prompt file - docs/implementation-prompts/17-contract-status-and-
  roadmap.md was rewritten in place (original requirements preserved at commit 4769090).
  Current-rust facts pinned: dispatcher advertises 71 tool names (59 core incl. six
  filesystem.* + 12 github.*), NOT the 53/65 that docs/mcp.md + README still claim;
  Command enum = 12 families (Init,Status,Tui,Serve,Mcp,Skill,Registry,Agent,Worktree,
  Policy,Fs,Tunnel); MCP filesystem.* schemas have NO expected_* args (CLI-only
  asymmetry); Control API has no edit-plane route and terminal/run has no capability
  gate; TUI editor is not EditService-backed; worktree merge absent; resolve_effective_root
  has no consumer outside CLI; rust@0d3a029 = 1227 tests/23 binaries (PR #126 head =
  1228 incl. 12 acceptance tests; 1216 base + 11 store_convergence). RUST ADVANCES
  MID-PROMPT: PR #122 (ARCH-001) merged after session start - fetch origin/rust
  BEFORE claiming "not merged"; live tree beats stale local refs. Toolchain can be
  wiped mid-session AGAIN (2nd time); rustup --default-toolchain stable --profile
  minimal + component add rustfmt clippy restores; target/ cache (6.8G) survives and
  makes re-verification fast. Forensics method that worked: count tool schema entries
  with regex over dispatcher.rs, extract Command enum variants with a brace-depth
  parser, grep callers of resolve_effective_root to classify wiring gaps.
- **windows-latest CI traps (PR #129 round, 2026-09)**: three found in one go. (1) The runner image ships machine-wide `core.autocrlf=true`, so any test that `git init`s a fixture then `git worktree add`/`checkout`s gets LF->CRLF smudging and byte-exact content asserts read `\r\n` — pin the fixture repo with repo-local `git config core.autocrlf false` (repo-local beats system/global scope; only command-scope GIT_CONFIG_* env would beat it, and runners don't use that). Reproduce on Linux with a scratch HOME containing `.gitconfig` with `autocrlf=true` (pin CARGO_HOME/RUSTUP_HOME to the real ones or rustup forgets its default toolchain). (2) Tests asserting the shared global audit ring via `dashboard()` (surfaces only recent(5)) race sibling threads: dashboard() does FS work (project scan, git probe) between the audit write and the ring read, and rate-limit/api tests spray audit events — fixed with a bounded re-record/re-check loop; sibling tests must also match on (action, subject), not action alone, since concurrent tests write the same action. (3) The `release-readiness` job (ci.yml) runs `cargo package` and REJECTS tarballs containing .github/workflows, .env, or key material — with no `package.exclude`, cargo's default ships everything not gitignored, so the job failed. Cargo.toml now has `exclude = [".github"]`. The job `needs: build-test`, so it had literally never run on a PR before (the windows leg was red since #127) — expect more never-exercised jobs to surface the first time the full matrix goes green.

- **Prompt 18 (master completion) — catalog-count correction (2026-09-30)**: the
  advertised MCP catalog is 59 core tools / 71 with github.* (verified live via
  tools/list and both interop harnesses). Earlier notes saying "53 core / 65"
  are historical (pre-filesystem.* exposure in prompt 11). docs/mcp.md,
  README.md updated; docs/mcp.md catalog table now carries the filesystem.*
  row. Fresh interop evidence regenerated (both harnesses PASS, incl. the six
  filesystem.* editing-interop checks the harness gained in prompt 11).
  Also: control API audit completeness fix — git push/pull/stage/unstage and
  files/content PUT now record api_git_* / api_file_write allow+deny events
  (subject = truncated path or remote name, detail = branch or byte count,
  never content); tests pin allow, deny, and no-content-leak behavior.

- **Prompt 18 — §15 caller attribution (MCP authorization/audit path)**:
  threaded `caller: Option<&SessionIdentity>` through `authorize_tool` /
  `authorize_policy` (the capability gate already had it). `McpDispatcher` has
  NO `caller()` accessor — removed the obsolete `caller_correlation` method in
  favor of the free fn `caller_audit_correlation(caller)` (dispatcher.rs,
  `pub`); SSE's `session_create` reuses it: bound sessions emit
  `audit_allow_as`, plain `audit_allow` otherwise (no invented identity). Gate
  order per tools/call: capability (-32005) -> builtin trust (-32003) -> policy
  (-32004) -> service. Gotchas: (1) `audit_deny_as(action, reason, subject,
  corr)` is positional — record_correlated stores `subject` as entry.subject
  and `reason` as entry.detail, so `policy_denied`'s SUBJECT is the RULE ID
  while the resource lands in detail; tests must match action + detail.
  (2) `redact_token_like` masks >=16-char base62 runs INCLUDING hyphens, so
  generated identity ids (`ws-`/`agent-`/`sess-` + 16-hex nanos) were coming
  out `[redacted]` in workspace_id/agent_id/session_id; new
  `services::audit::trusted_identity_id` (shape guard like
  trusted_edit_id/trusted_snapshot_id) stores the generated shape verbatim,
  everything else still redacts. Token-shaped POLICY RULE IDS (e.g. auto
  `policy-<sha>`) remain masked in audit subjects — pre-existing documented
  residual; the resource stays visible in detail. (3) A bound-caller test
  pinning a POLICY denial must first seed a CapabilityGrant
  (`crate::models::CapabilityGrant`, permission via
  `crate::mcp::permissions::Permission`, store root = the dispatcher's
  project root) or the capability gate answers -32005 before policy runs.
  Suite after the fix: 1246 tests green workspace-wide, fmt+clippy clean.

- **Prompt 18 Skills domain (2026-09, commit c230e53)**: `skills.enable`/`skills.disable`
  shipped end-to-end. `ProjectSkillReferences` (src/skills/project.rs) is the ONE canonical
  project-skill reference store (duplicate `references.rs` deleted): `states()`/`enable()`/
  `disable()` persist a `disabled: Vec<String>` list at `.agent/skills.json` (serde default,
  so legacy files load all-enabled), StoreLock-serialized + atomic-rename writes. Exposure
  state changes ONLY through enable/disable — add/remove never flip it; both toggles fail
  closed on unreferenced names ("skill is not referenced by the current project: {name}").
  MCP: two new tools (Medium risk), `skills.list` annotates `enabled`, `skills.read` refuses
  disabled references. CLI: `awh skill show/enable/disable` + state-marked list + durable
  audit (`init_global` at CLI startup, `cli_skill_*` actions). Control API GET
  `/api/v1/skills/project` renders `enabled` from the same store. Tool counts: 61 core /
  73 with github.*. Gotchas: (1) **tool_registry.rs MUST stay name-sorted** —
  `registry_lookup` is a binary search; a row out of alphabetical order silently breaks
  lookups for OTHER tools near it (skills.enable/disable placed after skills.search made
  skills.list resolve "uncategorized"). (2) NEVER re-type json! schema lines from terminal
  output — restore via `git show HEAD:src/mcp/dispatcher.rs` + a line-matching script; a
  retype drift in 8 entries broke schema-validation tests. (3) Long heredocs ECHO garbled
  prefixes (stray b/|/e/t) in this terminal, but the written file is correct — verify with
  `python3 -m py_compile` / grep, never trust the echo. (4)
  `GlobalSkillRegistry::discover()` honors `AWH_GLOBAL_SKILLS_ROOT` (registry.rs) as the
  test/embedder seam so tests never mutate HOME. (5) `SkillMcp::with_registry` is the
  in-crate injection seam; out-of-crate integration tests can only reach the ghost-denial
  path (no private-field injection across crates).

- **Prompt 18 Tasks domain audit (2026-09)**: verified TSK-001 convergence is complete and
  pinned. `src/core/tasks.rs` is the single task authority (ARCH-001; `src/mcp/tasks.rs` is a
  9-line re-export, CLI is a thin adapter); no Control-API tasks routes and no TUI tasks
  screen exist by contract (both prompts are conditional: "if exposed" / "as evidence
  permits"). One real gap fixed: a fieldless `tasks.update {id}` was a silent no-op on the
  MCP plane while the CLI rejected it at argument parsing — the canonical store now owns
  the verdict (`no changes requested: provide at least one of status, priority, or
  assignee`), firing AFTER the missing-id lookup so the missing -> null contract is intact
  (unknown id still returns Ok(None)/text "null"). Pinned by a store unit test
  (update_without_changes_fails_closed_on_every_plane) and a wire test
  (tasks_update_without_changes_fails_closed_on_the_wire; remember tools/call results are
  content-envelope-wrapped - assert on result.content[0].text). Suite: 1285 passed / 0
  failed workspace-wide, fmt+clippy clean. Audit parity confirmed: every tools/call emits
  `tool_invoke` (name only, args never logged, caller-attributed when bound) in addition to
  the CLI's `cli_task_*` durable audit events.

- **Prompt 18 Terminal domain (TRM-001, 2026-09)**: the documented `awh terminal
  run|list|kill` CLI contract (docs/CLI.md command tree + security-ordering list)
  was missing entirely — the CLI was the only plane without a terminal surface.
  Added src/cli/terminal.rs (thin adapter, same pattern as task/memory/context):
  `run` delegates to the canonical TerminalService (argv-only, kill_on_drop,
  --timeout bounded 1..=600s over the 30s default, 256 KiB caps inherited from the
  service), prints child stdout/stderr unmixed, propagates the child exit code
  (124 on timeout, 1 on spawn failure via Result), and audits cli_terminal_run
  (program name + timeout as detail; args NEVER logged — pinned adversarially by
  a test that greps the durable audit log for a sentinel arg). `list`/`kill`
  implement TRM-001's documented ephemeral-lifecycle allowance (its §6: no
  background-process plane exists — bounded synchronous runs only): list prints
  `[]` + explanatory stderr note; kill fails deterministically with "unknown
  execution id: {id}" for ANY id (never a raw-PID signal). The decision is
  documented in docs/terminal/README.md, not left implicit. Verified the other
  three planes already share the service: MCP terminal.run (authorize_tool +
  policy gate on program name + tool_invoke audit), Control API /terminal/run
  (bearer + api_terminal_run), TUI backend terminal_run (tui_terminal_run).
  Known limitation recorded: no OS sandbox around terminal children (the MCP
  sandbox wraps custom MCP server spawns only) — controls are auth/policy/argv/
  timeout/cap/audit. Tests: tests/terminal_cli.rs (9, real compiled binary;
  platform commands cfg-gated unix/windows like mcp_builtin_tool_gate.rs, since
  the SERVICE unit tests already use printf/sleep/head un-gated). Gotchas: (1)
  clap `trailing_var_arg` + `required` means empty argv is a USAGE error naming
  `<ARGV>...` — don't assert on the word "program"; (2) `tokio::runtime::Runtime
  ::new()?.block_on` is the CLI's bridge to the async service (main() is sync);
  (3) `std::process::exit(code)` does NOT flush stdout (LineWriter flushes on newline only) — flush stdout/stderr explicitly before ANY process::exit in the run path, or non-newline-terminated child output dies with the process (pinned by nonzero_exit_does_not_drop_buffered_child_stdout: awk writes no trailing newline, exits 3).
  automatically, but `print!` to a LineWriter... actually print! flushes on
  newline only; use explicit flush before process::exit to avoid losing the
  final stdout line on non-newline-terminated child output.

- **COL-001 Collaboration domain (Prompt 18, 2026-09)**: `src/core/collaboration.rs`
  (CollaborationStore, `.agent/collaboration.json`, schema-versioned, StoreLock +
  atomic rename) is the SINGLE collaboration state owner; `src/services/collaboration.rs`
  (CollaborationService) is the shared boundary all planes must call; `src/cli/collaboration.rs`
  + main.rs arm are thin adapters. Verbs: `awh collaboration agents|status|assign|
  activate|handoff(--request)|accept|release|conflicts|events`. Ownership states:
  Assigned/Active/HandoffRequested/HandedOff/Released (Released+HandedOff end a cycle;
  reactivation requires echoing the observed terminal revision; per-record revision
  increments). Store API returns `Result<CollabResult<T>>` (outer = I/O/anyhow, inner =
  domain CollabError) so domain denials ride `Ok(Err(..))`; service maps inner errors to
  stable category text via `CollabError::message()` - CLI exit-1 text == service text.
  Audit: transitions -> kind "collab" actions assign/activate/handoff_request/
  handoff_accept/release via `record_correlated` (subject `task:<id>`, detail owner/
  revision/state, correlation workspace/agent/session); refusals -> `record_deny` with
  actions collab_assign/collab_handoff/collab_accept/collab_activate/collab_release.
  `events` reads the durable log via `AuditLog::open(root).recent()` filtered by kind -
  requires the CLI arm's `init_global(&root)` (same pattern as task/memory arms).
  `conflicts` is evidence-only (never auto-resolves): scans held records for
  owner_agent_unknown/disabled/stopped, owner_session_unknown/not_usable, task_missing/
  task_terminal, worktree_missing; requires nothing to be deleted on the caller side.
  Validation at the mutation boundary: owner must exist + enabled (Active NOT required -
  stopped owners surface as conflict evidence instead of blocking transfer); supplied
  session must resolve via AgentRuntimeService::resolve_session (re-validates everything);
  Task/Worktree must exist in-workspace (TaskStore.get / WorktreeStore.list with
  removed_at.is_none()). Assignment never starts agents/activates sessions/mutates
  worktrees. GOTCHA (extends TW-002): StoreLock lock-file creation ENOENTs when
  `.agent/` parent is missing - CollaborationStore::ensure_parent() runs create_dir_all
  BEFORE StoreLock::acquire in assign/mutate paths (unit tests were failing on exactly
  this). Test shapes: core unit 11 (double-unwrap `.unwrap().unwrap()` for the two-layer
  Result), service unit 8 (single-unwrap; durable audit assertions must go through
  `audit::global().recent()` - init_global is OnceLock/process-wide so parallel unit
  tests cannot re-point the durable root; the durable round-trip is pinned by e2e),
  CLI unit 1, e2e `tests/collaboration_cli.rs` 6 (real binary; `agent session open` is
  POSITIONAL - `awh agent session open <AGENT_ID>`; `agent start <id>` required before
  session open; task create needs --description or it reads stdin). Suite: 1327 passed /
  0 failed (was 1301 pre-COL-001). Docs updated: CLI.md tree+counts, FEATURES.md.
