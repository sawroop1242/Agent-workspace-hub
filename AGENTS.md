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

0-11 done (branch rust; Phase 11 via PR #10, CI green on all 3 platforms).
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
  AND environment permission � `McpPermissions::validate` requires
  every secret to also be an allowed env name, so `--secret` grants the
  pair). Fail closed at registration, never send literal refs upstream.
  Gotchas: (1) test env-var manipulation needs globally-unique names
  (AWH_TEST_HEADER_SECRET) to survive parallel siblings;
  (2) `unwrap_err()` needs Debug on the Ok type � use `match` for
  non-Debug clients; (3) a shell-exported COMPOSIO_API_KEY leaks into
  `cargo test` and makes dispatcher tests hit the live Composio backend
  (401) � unset before testing; (4) repo-root `.agent/` is gitignored
  (anchored `/.agent/` so `examples/mcp-interop/.agent` fixtures stay
  tracked) since `--header` can carry raw credentials in other setups;
  (5) Composio hosted MCP key only works on
  `connect.composio.dev/mcp` (x-consumer-api-key header), NOT on the
  backend API the native `ComposioProvider` uses � register the hosted
  endpoint as a custom server instead.
- **Toolchain-reinstall trap**: rustup minimal profile lacks
  `cargo-fmt`/`cargo-clippy` shims until `rustup component add rustfmt
  clippy`; verify with `cargo --version` before trusting rustup logs.

- **Custom MCP HTTP headers (Phase 12, PR #16, f072414)**: `CustomMcpServerConfig.headers` map + `awh mcp add --header NAME=VALUE --secret NAME`. Names = RFC 7230 tokens, values control-char-free (CRLF injection rejected at config time), all values via `expand_secret_ref` (`${secret:NAME}` resolves only with BOTH secrets AND environment permission — `McpPermissions::validate` requires every secret to also be an allowed env name, so `--secret` grants the pair). Fail closed at registration, never send literal refs upstream. Gotchas: (1) test env-var manipulation needs globally-unique names (AWH_TEST_HEADER_SECRET) to survive parallel siblings; (2) `unwrap_err()` needs Debug on the Ok type — use `match` for non-Debug clients; (3) a shell-exported COMPOSIO_API_KEY leaks into `cargo test` and makes dispatcher tests hit the live Composio backend (401) — unset before testing; (4) repo-root `.agent/` is gitignored (anchored `/.agent/` so `examples/mcp-interop/.agent` fixtures stay tracked) since `--header` can carry raw credentials in other setups; (5) Composio hosted MCP key only works on `connect.composio.dev/mcp` (x-consumer-api-key header), NOT on the backend API the native `ComposioProvider` uses — register the hosted endpoint as a custom server instead.
- **Toolchain-reinstall trap**: rustup minimal profile lacks `cargo-fmt`/`cargo-clippy` shims until `rustup component add rustfmt clippy`; verify with `cargo --version` before trusting rustup logs.
- **AGENTS.md is not pure UTF-8** (a 0xd1 byte near offset 8098 makes strict decoders fail); append via python/shell, not the file editor.
