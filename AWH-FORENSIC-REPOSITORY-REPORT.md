# AWH — Forensic Repository Analysis Report

**Repository:** Agent-workspace-hub (`agent-workspace-hub`, crate v0.1.0)
**Branch analyzed:** `rust` (HEAD `0e38041`, merge of PR #21 "phase4a-tool-broker")
**Method:** Read-only source forensics. No files were modified, no commits created. `cargo` is unavailable in this environment, so all conclusions derive from static call-path tracing, not from executing the test suite. Every classification below states its confidence (HIGH = directly verified in source; MEDIUM = strongly inferred from multiple sources; LOW = architectural inference).

---

## 1. Executive Summary

AWH today is a **single-process, single-project MCP tool server with a competent transport/protocol layer, a genuine but narrow Phase-2/3 authorization gate, and a large amount of declared-but-unwired infrastructure** layered around it.

**What is real (verified by call paths):**

- A working MCP dispatcher (65 static built-in tools + dynamic `provider.tool` tools) over stdio, SSE, and streamable HTTP transports, with JSON-RPC envelope validation, per-session initialization lifecycle, protocol version negotiation, and per-session SSE registries.
- Two fail-closed authorization gates: a persistent trust store gating *external/custom MCP servers*, and a built-in-tool gate reusing the exact same `authorize` machinery for Medium/High-risk built-in tools, plus a workspace-local DENY-only policy store for 3 resource-scoped tools (`workspace.write_file`, `workspace.delete_file`, `terminal.run`).
- Two independent, high-quality filesystem containment implementations (`services::files::FilesService` and `mcp::workspace::WorkspaceMcp`) with traversal + symlink-escape checks, size caps, atomic writes, and bounded listings — but they are **duplicated, not shared** (see §24).
- argv-only (never shell-string) Git and terminal services with hard timeouts, output caps, and destructive-op taxonomy.
- A real Context Engine (~1,900 lines) with budgeting, scoring, selection, compression, durable offloads, and snapshot/restore of *context items* (not files).
- Cross-process advisory locking (`StoreLock`) for JSON-backed stores, with stale-lock reclaim.
- A bounded in-process audit ring with token-redaction at the single choke point.

**What is not real (verified absent):**

- **No EditService execution.** `services::edit.rs` defines `EditTransaction`/`EditOperation`/`ExpectedState`/`FileState`/`EditStatus`/`EditError` — the complete AWE-001 vocabulary — but **no code in the repository constructs an `EditTransaction` or calls `validate_shape()` outside the module's own tests**. There is no `filesystem.patch`, no apply, no rollback, no MCP/CLI/TUI exposure. The AWE master prompt itself mandates: *"Do not claim those features are complete merely because the core edit service exists"* (docs/issue-resolving-prompts/AWE-SEQUENTIAL-IMPLEMENTATION-MASTER-PROMPT.md, §"Remaining work", listing AWE-005 through AWE-015 as not done). Classification: **SCAFFOLD_ONLY.**
- **No file snapshots/rollback.** Roadmap Phase 5 (`awh snapshot create/list/inspect/restore/delete`, `awh workspace undo/history/restore`) has zero implementation: no `snapshot` CLI command exists (`enum Command` in `src/main.rs` has no Snapshot variant), no snapshot store for files, no provenance records. The only "snapshot" subsystem is `context/snapshot.rs`, which snapshots **ContextEngine item state** (`.agent/context-engine/snapshots/`), not workspace files.
- **No agent identity enforcement.** `AgentStore` + `CapabilityGrantStore` exist and are fully CLI-managed (`awh agent create/grant/revoke`), but the code itself states: *"Agents and their capability grants are plain persisted records in this phase: nothing here is consulted by tool execution — enforcement is a later phase"* (`src/main.rs:834-836`, `handle_agent_cli`). Classification: **DECLARED_BUT_NOT_CONNECTED.**
- **No Session→Agent→Workspace→Worktree model.** `SessionLifecycle` is a per-connection protocol-state machine (initialize-gate), not an agent session. No worktree support anywhere (`grep -rn worktree src` → 0 hits). No branch-per-agent isolation.
- **No memory/context integration with agents.** Memory entries and context items carry no `agent_id`, no `session_id` — provenance fields named in the roadmap's Provenance section (`file, agent, session, capability, tool, timestamp, before hash, after hash, snapshot`) do not exist on any mutation record.
- **MCP-first is only half-true.** The MCP plane is the most-developed surface, but it implements its **own** file/memory/task/connector/context stores in `src/mcp/*` while the Control API and TUI use a **different** set of stores (`core::memory::MemoryStore` on `.agent/memory.jsonl` vs MCP's `MemoryMcp` on `.agent/memory.json`; `services::files::FilesService` vs `mcp::workspace::WorkspaceMcp`; two different `Task` models with different status enums). The "services are the single owner of business logic shared by all interfaces" claim in `src/services/mod.rs` is violated by the MCP plane itself.

**Highest-risk findings (P0/P1, evidence in §35-36):** built-in tool authorization is opt-in with a default-allow posture (`authorize_builtin_tool` returns `Ok(())` when no `awh.builtin` trust record exists — deliberate, documented, and a genuine security posture choice that means **most deployments have no capability enforcement at all**); the in-memory audit ring is process-local and volatile (restart = total loss of audit history, bounded to 1,000 entries); the Control API exposes `terminal.run` and full git read/write with a single bearer token and no per-operation authorization, no workspace-escape protection on the terminal (`cwd` is the server's CWD, `program` can be any executable with inherited environment); and `cargo`-free verification means CI is the only thing standing between the repo and unverified claims.

**Overall maturity:** an early-prototype runtime whose protocol/transport and store layers are unusually disciplined, whose security model is honest about its own incompleteness, but whose **core product promises — agent-grade editing, snapshots/undo, provenance, and multi-agent isolation — are all scaffolding or absent.** Verdict sentence at the end (§44).

---

## 2. Repository Inventory

| Item | Value | Evidence |
|---|---|---|
| Language/edition | Rust 2021 | `Cargo.toml` |
| Crate name / binary | `agent-workspace-hub` / `awh` | `Cargo.toml` `[[bin]]` |
| Source size | 33,120 lines across `src/**` (`find src -name '*.rs' \| xargs wc -l`) | measured |
| Modules | `mcp/` (dispatcher, transports, gates, stores, providers), `services/` (files, git, terminal, projects, audit, rate_limit, edit), `core/` (agents, capability_grants, context, files, memory, policy, project, tasks, workspace), `context/` (engine + 11 submodules), `skills/` (store, registry, installer, parser), `tui/` (backend, remote, 13 screens), `api/control.rs`, `tunnel/`, `models/`, `mcp/http.rs`, `mcp/sse.rs`, `mcp/server.rs` (stdio) | directory listing |
| Tests | 12 integration test files in `tests/` (149 test fns) + 485 in-module `#[cfg(test)]` fns ≈ **634 test functions** | counted |
| Dependencies | 27 direct crates, 349 locked packages; `axum 0.8`, `tokio 1`, `reqwest 0.12` (rustls), `ratatui 0.30`, `clap 4`, `serde/serde_json`, `sha2`, `subtle`, `tempfile`, `thiserror`, `tracing`, `tower-http`, windows-sys (Windows-only) | `Cargo.toml`, `Cargo.lock` |
| Feature flags | **None.** No `[features]` section; optional providers (GitHub, Composio) are runtime/env-gated (`GITHUB_TOKEN`, `COMPOSIO_API_KEY`), not compile-time | `Cargo.toml` |
| CI | `.github/workflows/main.yml` — fmt --check, clippy `-D warnings` all-targets all-features, `cargo test --all-targets` on ubuntu (single OS) | workflow file |
| Release | `.github/workflows/release-rust.yml` — 6-target matrix: linux x64/aarch64, **aarch64-linux-android** (Android NDK), macOS x64/aarch64, Windows x64; needs `verify` job | workflow file |
| Examples | `examples/bench.rs` (schema-validation micro-benchmarks), `examples/mcp-interop` | listing |
| Benches | none (no `benches/` directory) | listing |
| Registry assets | `registry/mcps/` (community MCP catalog JSON), `registry/skills/` | listing |
| Docs | 20+ markdown docs: `PROJECT_ROADMAP.md` (10 phases), `PROJECT_STATUS.md`, `ROADMAP_GAP_MATRIX.md`, `completeness-audit.md`, `mcp.md`, `security.md`, `release.md`, `development.md`, plus 9 AWE issue-resolving prompts | `docs/` |
| Installers | `scripts/install.sh` | listing |
| Repo size | 2.6 MiB (excl. .git/target) | measured |

**Toolchain:** no `rust-toolchain.toml`; CI pins `dtolnay/rust-toolchain@stable`. No MSRV declared.

---

## 3. Actual Architecture

Derived from real construction paths (`McpDispatcher::new_async`, `ControlState::new`, `LocalBackend::new`):

```
External Agent (MCP client)          Human Operator
        │                                   │
  stdio / SSE / streamable-HTTP         CLI (clap)  /  TUI (ratatui)
        │                                   │        LocalBackend or RemoteBackend
        ▼                                   │              │
  mcp::dispatcher::McpDispatcher          ▼              ▼
  (SessionLifecycle, per-request          ┌─────────────┴──────────┐
   schema validation, authorize_tool)     │   services layer       │
        │                                 │  files/git/terminal/   │
        │  ├── WorkspaceMcp  (own FS code)│  projects/audit/       │
        │  ├── MemoryMcp     (.agent/memory.json)   rate_limit/edit│
        │  ├── TasksMcp      (.agent/tasks.json)   │(model only)   │
        │  ├── ConnectorsMcp  (.agent/connectors.json) └──────┬─────┘
        │  ├── SkillMcp      (.agent/skills + global)        │
        │  ├── ContextEngine  (.agent/context-engine/*)       │
        │  ├── GithubProvider (env-gated)        ▼            ▼
        │  ├── providers registry (dynamic        core stores:  core stores:
        │  │   MCP servers + Composio,           ProjectStore  MemoryStore
        │  │   CircuitBreakerMcpClient)          TaskStore     ContextStore
        │  └── GitService/TerminalService  ←—— also used by MCP git.*/terminal.run
        ▼
  Workspace: one directory = one project root (CWD at process start)
```

**Actual connections (verified):**

- MCP `git.*` tools and `terminal.run` **do** go through `services::git::GitService` / `services::terminal::TerminalService` (dispatcher lines ~2162-2251, `use crate::services::{git::GitService, terminal::TerminalService}` at `dispatcher.rs:24-25`). FACT.
- TUI `LocalBackend` goes through `services::*` and `core::*` stores (`tui/backend.rs:174-330`). FACT.
- Control API goes through `services::*` + `core::*` stores (`api/control.rs:28, 69-73, 762-828`). FACT.
- **MCP file/memory/task/connector tools do NOT go through `services::`** — they use MCP-local stores (`WorkspaceMcp`, `MemoryMcp`, `TasksMcp`, `ConnectorsMcp` constructed at `dispatcher.rs:582-586`). FACT. This is the single largest "MCP-first but not service-first" contradiction in the repo.

**Intended but absent connections (verified missing):**

- `services::edit` ← nothing calls it. No interface constructs an `EditTransaction`.
- `core::agents::AgentStore` / `core::capability_grants` ← CLI only; dispatcher never reads either.
- `core::tasks::TaskStore` / `core::files::FileStore` ← referenced by no non-core module (only `core/policy.rs` doc-comment mentions them). **DEAD_CODE** candidates: `FileStore` (zero call sites outside its own module and tests), `core::tasks::TaskStore` (zero call sites outside core).
- `context::planner::DeterministicPlanner` ← exported by `context::mod` but no call site in `mcp`, `main`, `api`, or `tui` constructs one. **DEAD_CODE** (planned Phase, unreachable today).
- `GitService` high-risk ops (`hard_reset`, `force_push`, `clean`, `delete_branch`, `discard_file`) ← **no caller in the entire repository** (only `HighRiskGitOp` taxonomy + `validate_repo_path` guard exist). `git.push`/`git.pull` exist on the service and the TUI, but **not** in the MCP tool catalog. FACT: `grep -rn` for `hard_reset()\|force_push()\|\.clean()\|delete_branch(\|discard_file(` outside `services/git.rs` → zero hits.

---

## 4. Runtime Architecture

**Bootstrap (`src/main.rs`):**
- `main()` initializes `tracing_subscriber::fmt().with_writer(stderr)` (stdout is reserved for JSON-RPC; verified `main.rs:342-347`), then `Cli::parse()`.
- `awh mcp serve --transport stdio|sse|http` → `serve_stdio()` or `serve_sse()`; `serve_sse` reads `AWH_HOST`/`AWH_PORT`/`AWH_TLS_*`/`AWH_*` limit overrides, builds `McpDispatcher::new_async` **on the serving runtime** (comment documents the nested-runtime hazard; regression tests `tests/mcp_dispatcher_lifecycle.rs` pin fail-closed behavior), then `mcp::http::serve`.
- `awh serve` → Control API on axum with bearer token, 30s timeout layer, 1 MiB body limit, sliding-window rate limiter.
- `awh tui` → `LocalBackend` (owns a **current-thread tokio runtime** for blocking on async git calls from a synchronous UI loop) or `RemoteBackend` (reqwest).

**Concurrency model:**
- Dispatcher: `Arc<McpDispatcher>` shared across HTTP connections; per-session `SessionLifecycle` with `AtomicU8` state + `Mutex`-wrapped metadata fields; initialize uses a CAS (`mark_initialized`) so exactly one concurrent `initialize` wins (`dispatcher.rs:949-960`). Solid.
- Provider registry: `RwLock` (`self.providers.read().await` / `.write().await` at call sites; `connector.composio_register/remove` take the write lock to mutate live providers).
- Stores: cross-process file locks (`StoreLock`, O_CREAT|O_EXCL, 10s acquire timeout, 30s stale reclaim). Good design for multi-process agents, unusual and thoughtful.
- ContextEngine: `RwLock` over in-memory items + atomic writes for offloads/snapshots.
- TUI: synchronous loop; all async service calls via `runtime.block_on` on a dedicated current-thread runtime — safe but serializes the UI during any git/network call (a 30s `git pull` freezes the TUI).

**Shutdown:** no signal handling for stdio MCP or Control API (no `tokio::signal` usage found in `main.rs`; `tokio`'s `signal` feature is enabled but the only consumer is… none found — the feature flag is enabled without a call site, minor dead dependency config). HTTP servers run until process kill. `kill_on_drop(true)` on terminal/git child processes is a genuine mitigation for orphaned children. Partially implemented.

**Entity analysis (the core question — does each runtime entity exist and is it enforced?):**

| Entity | Exists? | Where | Enforced? |
|---|---|---|---|
| Agent | As a persisted record only (`models/agent.rs`, `core/agents.rs`) | `awh agent` CLI | **No** — "nothing here is consulted by tool execution" (main.rs:835) |
| Session | Protocol-session only (`SessionLifecycle`, `dispatcher.rs:146`) | initialize-gate, SSE session registry | Protocol enforcement only; **no identity propagated to tools/audit** — audit events carry no session/agent id |
| Workspace | Implicitly = process CWD | `McpDispatcher::new(current_dir)`, `ControlState::new(current_dir)` | Path containment is enforced; "workspace" as an entity with identity is absent |
| Project | Directory containing `.agent` (`core/project.rs`, `ProjectStore::list` = directory scan) | CLI/TUI/API | Partially — create/delete validated (`validate_project_name`, canonicalize + containment before `rm` at `services/projects.rs:50-60`) |
| Worktree | **MISSING** | — | No |
| Branch | Only via `GitService` argv helpers | git tools | No isolation semantics |
| Capability | `Permission` enum + `McpPermissions` + `CapabilityGrant` model | trust gates, custom-MCP env filtering | Custom-MCP gate: enforced. Builtin gate: opt-in only (default-allow). Agent grants: not enforced |
| Policy | `PolicyStore` DENY-only, 3 tools | `authorize_policy` | Enforced but DENY-only and 3 tools only |
| Task | Two models (MCP `tasks.rs`, `core/tasks.rs`) — divergent | MCP tools / core store | MCP one functional; core one orphaned |
| Context | ContextEngine + items | context.* tools | Functional, in-process |
| Snapshot | Context-item snapshots only | `context/snapshot.rs` | Not a file-recovery mechanism |
| Audit | In-memory ring, 1000 entries | `services/audit.rs` | Volatile — see §12 |
| Memory | Two stores, two formats | MCP `.agent/memory.json`; core `.agent/memory.jsonl` | Both functional, mutually unaware |
| Skill | SkillStore (project) + GlobalSkillRegistry (home) | skills.* tools, CLI | Functional; docs-as-capability (SKILL.md), no enforcement surface |
| Process | TerminalService | terminal.run | Bounded; not capability-constrained by default |

---

## 5. MCP Architecture

**Protocol handling (`dispatcher.rs:992+` `dispatch_inner`, `dispatch_strict_with_lifecycle` at 844):**
- JSON-RPC 2.0 envelope validation with precise `-32600` errors (`validate_envelope`); notifications never answered (spec-correct, including for invalid notifications — documented and deliberate).
- Per-session initialization state machine: `New` → (initialize) → `Ready`; pre-init requests other than `ping` get `server_not_initialized`; duplicate initialize gets `invalid_request`; closed/failed sessions reject. Enforced in stdio (`server.rs:33`), SSE (per-session `Arc<SessionLifecycle>`, `sse.rs:79-103`) and HTTP (`http.rs:456-460` uses the SSE session registry so streamable-HTTP sessions share the same gate). HIGH confidence this is coherent.
- Protocol version negotiation: `SUPPORTED_PROTOCOL_VERSIONS` (includes `2025-06-18`), falls back to the default version; negotiated version + clientInfo recorded on the lifecycle.
- `ping` → `{}`; `resources/list`/`resources/read` implemented (project context, memory, skills as `awh://` URIs); `prompts/list` → empty; `prompts/get` → deterministic unknown-prompt error.

**Schema validation (before dispatch):** `call_tool` runs `validate_tool_arguments(&schema, name, &arguments)` for every static tool **before** the match (`dispatcher.rs:~2235`); failures are `-32602` with an audit deny (`tool_validation/schema_mismatch`). Dynamic tools are schema-validated via `validate_dynamic_arguments` on **both** direct `provider.tool` calls and the generic `connector.invoke` path — closing the "invoke a dynamic tool through the generic escape-hatch" hole (explicit comment: *"A dynamic tool cannot bypass schema validation by addressing itself through connector.invoke"*). Property-based tests of the schema validator exist (`schema.rs:2658+` proptest). This is genuinely well-engineered.

**Gaps:**
- **No cancellation** (JSON-RPC `$/cancelRequest` or MCP cancellation) anywhere.
- **No per-request timeout on tool execution** in the dispatcher: `git.*` (30s) and `terminal.run` (30s) have service-level timeouts, but `connector.invoke` into a hung provider relies solely on the circuit breaker and the HTTP transport's 30s layer; a stdio client has no bound on a slow dynamic provider call. MEDIUM.
- **`isError` is never set in built-in tool responses** — every successful `tools/call` returns `{"content":[{"type":"text",...}]}` unconditionally (dispatcher.rs:2250); MCP-compliant clients cannot distinguish tool-level failure when a handler returns an error-shaped JSON value as success. (Errors raised as `Err` do become JSON-RPC errors, which is one valid style, but the `isError` signal is unimplemented.) FACT, LOW severity.
- Rate limiting on the MCP HTTP plane exists (sliding window keyed by `X-Forwarded-For` first value or a shared "direct" bucket). The keying choice means all direct clients share one bucket — a trivial DoS-amplification quirk; also `X-Forwarded-For` is spoofable when not behind a trusted proxy. MEDIUM.
- TLS: optional; `TlsConfig::validate` permits `(None, None)` = plaintext. `serve_sse` defaults bind `0.0.0.0:8443` **with no TLS required** — the bearer token would travel in cleartext. `tunnel/mod.rs` warns about loopback but the MCP HTTP path does not refuse non-loopback-without-TLS. This is the remote-plane's most dangerous default. HIGH confidence (read `tls.rs:28-39`, `main.rs serve_sse`).

---

## 6. Tool Inventory (65 static tools, verified against `tools_list_static` catalog + registry)

Grouped by namespace; "authz" = what `authorize_tool` actually does.

**Workspace (5):** `workspace.context/read_file/list_files` (Low, no gate), `workspace.write_file`/`delete_file` (Medium → builtin gate + policy on `path`). Implementation: **`mcp::workspace::WorkspaceMcp`**, not `services::files` — duplicated FS logic (§24). Atomic write via NamedTempFile+persist+sync. Input schemas require `path` (+`content` for write).

**Memory (5):** `memory.store/update/delete` (Medium), `memory.get/search` (Low). Store: `.agent/memory.json`, `StoreLock`-guarded, max 10k entries / 1 MiB content / 64 tags. `update_existing` holds the lock across check+write so a concurrent delete can't race a resurrect (documented + correct).

**Tasks (5):** `tasks.create/update/delete` (Medium), `tasks.get/list` (Low). `.agent/tasks.json`, limits (10k tasks, id ≤256, title ≤1k, description ≤64k). Assignee is a free string — no agent-identity linkage (the grants/agents stores are never consulted).

**Connectors (8) + connector.* (8):** `connectors.add/enable/disable/remove` (Medium), `connectors.list/get` (Low); `connector.providers/tools` (no gate — read-only provider introspection), `connector.invoke` (High, [Network]), `connector.composio_link/register/remove` (Medium). **Note:** `connector.composio_accounts` has **no `authorize_tool` call** (read-only by intent, consistent with Low-risk exemption, but it is the only network-performing tool without one — inconsistency worth noting). `ConnectorsMcp` deliberately stores no secrets (`connectors.rs:49-50` documents "tokens must live in an OS credential store").

**Context engine (10):** `context.status/get/search` (Low), `insert/remove/offload/optimize/protect/unprotect/assemble/restore` (Medium). Backed by `ContextEngine` (RwLock in-process items + durable `.agent/context-engine/offloads/`). `context.restore` restores an offloaded *context item*, **not** files. No snapshot tools for files.

**Skills (5):** `skills.list/read/search` (Low), `skills.add/remove` (Medium). Project store `.agent/skills/<name>/SKILL.md` + global registry in home dir. Skills are markdown documents (front-matter parsed, `parse_skill`); they are **documentation containers, not executable capability packages** — no versioning enforcement, no dependency resolution, no per-agent activation, no trust model beyond "operator installed it". Installer does verify SHA-256 against the registry manifest (`skills/installer.rs:51-52`) — real integrity checking. 

**Git (7):** `git.status/branch/log/diff` (Low), `git.stage/unstage/commit` (Medium, [Filesystem]). Handlers build `GitService::open(self.workspace.root())` per call. **`git.push`/`git.pull` are NOT exposed** (verified: zero occurrences in dispatcher; they exist only on the service and the TUI). Destructive ops exist on the service with zero callers.

**GitHub (12, env-gated):** all call `authorize_tool` + provider. Read tools Low; `pr_create/pr_merge/pr_review/issue_create/issue_comment/workflow_dispatch/release_create` High [Network]. Fail-closed without `GITHUB_TOKEN` (constructor leaves `github = None`; calls return "set GITHUB_TOKEN" error; tools unadvertised). Target resolution (`resolve_github_target`): explicit args → origin remote URL (userinfo stripped, `github.rs:43`) → `GITHUB_DEFAULT_*`; **never mixes sources** (test `github_half_specified_target_never_mixes_sources` pins it). The full GitHub token is held in the provider; the token never reaches tool arguments or audit (audits log `owner/repo` only via `audit_github_invocation`).

**Terminal (1):** `terminal.run` (High, [Process]) — argv-only, cwd=workspace root, 30s timeout, 256 KiB capture, `kill_on_drop`, program name whitespace-rejected (anti-shell-string), policy-checkable on `program` name only.

**System (1):** `mcp.status` — honest: "status reports the SERVER PROCESS state only … it is NOT a subsystem-health verdict".

**Orphans/duplicates:** none truly orphaned (all 65 advertised and dispatched), but `git.branch`'s registry entry says "Create or switch a git branch" (tool_registry.rs:262-269) while the **tool description says "Current Git branch" and the handler calls `git.branch()` = `rev-parse --abbrev-ref HEAD`** — the registry metadata describes a feature that does not exist. Registry/advertised-description drift. FACT, P3.

**Validation matrix:** every tool with arguments has an `inputSchema`; `strval`/`u64val`/`strings` fail with actionable errors on wrong types; `git.log` clamps limit 1..=200; memory/context/tasks enforce count/size caps at the store layer.

---

## 7. Filesystem Security Forensics

Two independent implementations. Both verified line-by-line:

**`services::files::FilesService::resolve_checked` (files.rs:39-67):**
- Rejects absolute, `..`, root, prefix components. FACT.
- Symlink escape: canonicalizes root; walks up from the joined target to the deepest **existing** ancestor and requires the canonicalized ancestor to be inside the root. FACT.
- **Weakness (verified):** `resolve_checked` returns `joined` (the *non-canonicalized* join), not the resolved path. For a write, the final component could itself be a symlink that was created **between** `resolve_checked` and the subsequent `fs::write` — a classic TOCTOU window. In the single-process threat model this is a non-issue (the attacker is the caller), but under the repo's own multi-agent scenario (two agent processes against the same project), agent A can win a race: A calls write on `a.txt`, agent B replaces `a.txt` with a symlink to `/etc/cron.d/x` between the check and the `fs::write`, and A's content lands outside the root. `FilesService::write` uses a **plain `fs::write`** — not the temp+rename pattern. Compare with `WorkspaceMcp::write_file`, which uses `NamedTempFile::new_in(parent)` + `persist` — but **also** resolves via `safe_new_path` and has the same check-then-act gap on the final component (though its existing-target check does canonicalize and would catch a symlink already present at call time). **Residual TOCTOU risk on the final component in both implementations.** MEDIUM-HIGH confidence in the finding; exploitability requires a hostile concurrent writer inside the workspace.
- Read path: metadata len ≤ 8 MiB then `fs::read_to_string` (UTF-8 required — binary files error, not corrupt).
- `delete`: refuses root; `remove_dir_all` for dirs — a tool-path deletion of a whole directory subtree. **MCP exposes only `workspace.delete_file` (WorkspaceMcp::delete_file, files only, missing = false)** — the directory-deletion capability exists on the service and is reachable via the Control API `DELETE /files/entry` and TUI, with operator-grade auth only.
- `rename`: both endpoints resolved; refuses existing destination; `fs::rename` — same final-component TOCTOU caveat.
- `search`: bounded walk, skips `.git`/`.agent`, file-size-capped, needle lowercased (case-insensitive).

**`mcp::workspace::WorkspaceMcp::safe_path`/`safe_new_path` (workspace.rs:133-190):**
- `safe_path` (read/list): canonicalizes and requires containment — **rejects non-existent paths** with "workspace path does not exist". Symlink-safe for reads. FACT.
- `safe_new_path` (write/delete): existing target → canonical containment check; new target → deepest existing ancestor canonicalized + containment + missing tail rejoined. Correctly handles the "symlinked directory" case for writes into new files (the ancestor check). The final-component race noted above applies.
- `write_file`: content ≤ 5 MiB, atomic temp+rename+`sync_all`. Better than `FilesService::write`.
- `read_file`: ≤ 2 MiB, UTF-8.
- `list_files`: files only, no recursion, sizes included.

**Conceptual attack matrix:**

| Attack | FilesService | WorkspaceMcp | Verdict |
|---|---|---|---|
| `../` / `../../` | blocked (component scan) | blocked | ✅ |
| absolute path | blocked | blocked | ✅ |
| symlink → outside | blocked (ancestor canonicalization) | blocked (both paths) | ✅ |
| nested symlink (symlink dir → symlink dir → outside) | blocked (canonicalize resolves fully) | blocked | ✅ |
| non-existent parent | blocked via ancestor walk | blocked via ancestor walk | ✅ |
| rename outside root | blocked (both endpoints checked) | N/A (no rename tool) | ✅ |
| delete outside root | blocked + root-refusal | blocked | ✅ |
| write outside root **via race on final component** | **possible window** (`fs::write` after check) | **possible window** (persist after check) | ⚠️ TOCTOU |
| `context.rs` reading `AGENTS.md`/`AGENT.md`/`README.md` | `root.join(name)` — names are fixed literals, no user input | — | ✅ (not attacker-controllable) |

The security boundary is **real code, not convention** — unusual and commendable — but the write paths share the universal open-at-check/close-at-use gap that a file-open-with-O_NOFOLLOW-style or openat2/RESOLVE_BENEATH approach would close. On Android/Termux (no user namespaces, no `openat2` on older kernels) mitigation would need to be in-process.

---

## 8. Agent-Grade Editing

**Implemented (model only):** `src/services/edit.rs` (367 lines):
- `EditId` (process-unique, monotonic seq + nanos), `EditOperation::{Replace{occurrence}, Insert{line}, DeleteRange, Patch, ApplyDiff}`, `ExpectedState{hash, context, size, line_count}`, `FileState::from_content` (SHA-256), `FileState::matches`, `EditStatus` (11-state lifecycle vocabulary incl. `Snapshotted`, `Verified`, `RolledBack`), `EditTransaction::{new, single, validate_shape}`, `EditError` (structured, thiserror), `validate_path` (rejects absolute + traversal), `sha256_hex`.

**Not implemented (verified):** any executor. There is no `EditService` struct, no apply/verify/rollback functions, no occurrence-search, no line-insert/delete algorithm, no unified-diff parser (`ApplyDiff` exists as a data variant whose doc says "Parsing/application is implemented by the unified-diff milestone"), no multi-file transaction coordinator, no snapshot invocation, no provenance capture, no audit integration, no MCP/CLI/TUI exposure, no capability check anywhere in this module.

**Call-path trace attempt (Workflow B from §35):** `Agent → MCP → capability → policy → EditService → FilesService → verify → audit` **terminates at "MCP"**: no tool name matches `filesystem.*` or `edit.*`; `workspace.write_file` is a whole-file overwrite (no old-content match, no expected-hash precondition, no verification, no rollback). The closest real primitive is `workspace.write_file`, which is a non-verifying overwrite: an agent that mis-reads a file silently destroys it with a single call, with only the in-memory audit ring recording "workspace_write <path>" (no before/after hash — provenance fields do not exist).

**Does AWH support `read → locate → validate → patch → verify → snapshot → audit → rollback` as one operation?** **No.** Only "read" and "unverified overwrite" exist. The AWE-002/003/004 prompts specify the missing pieces precisely (safe contextual replacement with occurrence semantics, line-range ops, multi-op `filesystem.patch` with no-partial-preparation invariant) and the master prompt explicitly lists them as **remaining** (AWE-005…AWE-015). The repository is honest here; any external claim of "agent-grade editing" would not be.

Classification: **SCAFFOLD_ONLY.** Confidence: HIGH (exhaustive grep for `EditTransaction|EditOperation|services::edit` across `src/mcp`, `src/api`, `src/tui`, `src/main.rs`, `tests` returns zero non-definition hits).

---

## 9. Git Architecture

`GitService` (services/git.rs, argv-only, 30s default timeout, non-zero exit → `Err` with stderr, `run_raw` for tolerant internal calls):

| Operation | Method | Argv | Notes |
|---|---|---|---|
| status | `status` | `["status","--porcelain"]` | parsed via `porcelain_entries` |
| stage | `stage(path)` | `["add","--",path]` | `--` separator: pathspec injection blocked |
| unstage | `unstage` | `["reset","HEAD","--",path]` | |
| commit | `commit(msg)` | `["commit","-m",msg]` | empty-msg refused; `-m` value can't be reinterpreted as option (documented reasoning) |
| log | `log(limit)` | clamped upstream to 1..=200 | distinguishes empty-repo vs not-a-repo via `rev-parse --git-dir`/`--verify HEAD` — careful edge handling |
| branch | `branch` | `["rev-parse","--abbrev-ref","HEAD"]` | read-only (registry metadata wrongly says create/switch — §6) |
| branches | `branches` | `["branch","--list"]` | |
| diff / diff_staged | | `["diff","--",p]` / `--cached` | |
| remote_url | | `["remote","get-url",n]` | tolerant, returns Option |
| push / pull | | `["push","-u",r,b]` / `["pull","--no-rebase",r,b]` | **not MCP-exposed** |
| discard_file | | `["checkout","--",path]` + `validate_repo_path` | HIGH RISK, **zero callers** |
| hard_reset / clean / force_push / delete_branch | | `["reset","--hard","HEAD"]` / `["clean","-fd"]` / `["push","--force-with-lease",r,b]` / `["branch","-D",n]` (empty/HEAD refused) | HIGH RISK, **zero callers** |

**Command construction safety:** every operation is `Command::new("git").args(...)`. No shell. Message text rides as an argv value after `-m`. Pathspec-able args sit behind `--`. Argument injection: **not achievable through the exposed surface** (a path beginning with `-` is safe behind `--` in stage/unstage/diff; `discard_file` adds `validate_repo_path` rejecting absolute/`..`/empty). HIGH confidence.

**Destructive-operation protection:** the `HighRiskGitOp` enum exists **purely as taxonomy** — grep shows no consumer; "UIs must confirm before calling" is convention only. Since the destructive methods have no callers, no exposure exists today; but the protection model for when they get wired is absent.

**Agent→Session→Workspace→Worktree→Branch model: does it exist?** **No.** FACT: zero `worktree` references in `src/`. The dispatcher binds to exactly one directory (process CWD) for its lifetime; there is no per-agent project checkout, no branch allocation, no merge coordination, no conflict detection. Two agents connecting via stdio in the same directory operate on **the same working tree concurrently** with only `StoreLock` serializing the JSON side-stores (memory/tasks/connectors/policy), **not** the source files or git index. Concurrent `git stage/commit` from two agents can interleave arbitrarily (git's own index lock will reject simultaneous index writes with an error, which is incidentally the only protection). The roadmap's "First-class agent worktrees" (Phase 3, line 224) is entirely unimplemented.

---

## 10. Capability / Policy Security Model

**Three authorization systems exist:**

1. **Custom/external MCP server gate** (`execution_gate::authorize` + `is_authorized` at dispatcher.rs:2436): fail-closed; requires a persisted approval (`~/.agent-workspace-hub/trust.json`, overridable `AWH_TRUST_DIR`), version match, and subset-of-approved permissions (`can_enable` in trust.rs — Trusted/Reviewed pass, Blocked/Unknown fail; network/process booleans, filesystem/env/secrets lists). Env filtering on spawn: `is_valid_env_name` + `is_blocked_environment` (custom_mcp.rs:134-141). Secrets requested-but-not-approved are refused by name (`audit_secret_deny`). **Enforced.** HIGH.

2. **Built-in tool gate** (`execution_gate::authorize_builtin_tool`): registry lookup (unknown name → deny — "an ungatable name must not execute"); Low-risk tools bypass by design; **no trust record for `"awh.builtin"` → `Ok(())`** — the documented, deliberate default-allow ("every pre-existing workspace" keeps pre-gate behavior). With a record: required `Permission`s must be covered, level must permit, version must match; missing permission is named precisely in the error. Uses the *same* `authorize` machinery — no second parallel engine. **Opt-in enforcement.** This means the `awh mcp trust awh.builtin --filesystem` flag grants a *workspace-wide, all-files, list-scoped label* — the reserved `awh.builtin` scope marker is "never a real path" — so the Filesystem permission is **binary, not path-scoped** (comments say path-scoped policy is Phase 3+; in practice path scoping arrived only as the DENY-only policy store). 

3. **Workspace-local DENY-only policy** (`core::policy::PolicyStore` + `authorize_policy`): `.agent/policy.json`, `StoreLock`-guarded, first-match-wins in insertion order, exactly three tools (`workspace.write_file`/`workspace.delete_file` path-prefix matching; `terminal.run` exact program-name matching), rule ids validated (`is_safe_policy_id`), store corruption → fail-closed **as internal error** (not a policy denial — carefully distinguished), every decision audited (`policy_denied` vs `policy_allowed`). **Enforced for 3 tools only.** CLI: `awh policy deny/list/remove` (rejects unsupported tools before persisting).

**Agent grants (4th system, inert):** `CapabilityGrantStore` — per-agent grants with `expires_at` field — **never read by any enforcement path**. Its own creator documents this (main.rs:834-836). When Phase 5 per-agent scoping arrives, the plan is to wire this store into the gate; today it is bookkeeping. **DECLARED_BUT_NOT_CONNECTED.**

**Operation × plane authorization matrix (verified):**

| Operation | MCP | Control API | TUI | Internal |
|---|---|---|---|---|
| file write | builtin gate (opt-in) + policy (3 tools) | bearer token only | none (operator is trusted) | n/a |
| file delete (dir) | not exposed | bearer only | confirmation UI | n/a |
| git commit | builtin gate (opt-in) | bearer only | none | n/a |
| git push/pull | **not exposed** | bearer only (POST /git/push) | direct keybind | service exists |
| terminal.run | builtin gate (opt-in; policy by program name) | bearer only | confirmation | none |
| external MCP server spawn | trust store (fail-closed) | n/a | n/a | n/a |
| skill install (global) | CLI only | bearer only (`POST /skills/project`) | confirmation | registry SHA-256 |
| audit read | `mcp.status` (no authz needed; local) | bearer only | none | n/a |

**Findings:**
- The MCP plane has the only real authorization machinery; the Control API is **coarse single-token** (constant-time compared via `subtle` — verified `auth.rs:9`) with no scopes, no per-operation grants, and a `/terminal/run` that spawns **arbitrary programs** with **the server process's environment** (including `GITHUB_TOKEN`, `COMPOSIO_API_KEY`, `AWH_*` if exported). Whoever holds the API key fully owns the machine the server runs on. That is an accepted "admin plane" design, but it's indistinguishable-in-privilege from the MCP plane's most dangerous tool, and the TUI remote mode drives it. **P1.**
- **Internal-service bypass:** any code constructing `FilesService`/`GitService`/`TerminalService` directly skips all gates by construction (gates live in the dispatcher, not the services). The service doc comments acknowledge this ("callers must gate it behind their own authorization"). Today's callers (TUI, Control API) do it deliberately; tomorrow's callers must remember. No defense-in-depth at the service layer. **P2.**
- `audit_allow("tool_invoke", name, "tools/call")` fires **before** the tool runs (dispatcher, after schema validation) — so an invocation that later fails still logged as `allow` at the invoke level; failures are recorded separately via `audit_tool_failure`. Reconstructability is decent for allow/deny but no session/agent correlation (§12).

---

## 11. Snapshots / Rollback / Provenance

**File snapshots: MISSING entirely.** No `awh snapshot` CLI (verified: `enum Command` has no Snapshot variant), no file snapshot store, no before/after hash capture, no `awh explain`. Roadmap Phase 5 is fully unimplemented.

**What exists and calls itself snapshot:** `context::snapshot::SnapshotStore` — persists **ContextEngine item sets** under `.agent/context-engine/snapshots/` as JSON (`fs::write` + read/list/inspect/restore/delete, engine methods `snapshot/list_snapshots/inspect_snapshot/restore_snapshot/delete_snapshot`). This is a *context-window state* recovery mechanism (restore which items were active), **not** a workspace mutation recovery mechanism. The MCP `context.*` surface does not expose `context.snapshot`/`context.restore_snapshot` as tools either (tools are status/get/search/insert/remove/offload/optimize/protect/unprotect/assemble/restore — `restore` = restore one offloaded item).

**Provenance:** no record anywhere links (file, agent, session, tool, before-hash, after-hash, snapshot). `FileState`/`sha256_hex` exist in `services::edit.rs` precisely to enable this — and are unused. Audit entries record `{ts, kind, action, subject, detail}` with subject = path or tool name — that is the entire mutation history AWH can reconstruct, and it lives in a volatile 1,000-entry ring (§12).

**Edit → Snapshot → Mutation → Verification → Provenance → Audit → Rollback chain:** only the terminal nodes exist (mutation via `workspace.write_file`; audit via the ring), and they are **not linked** — the audit `workspace_write` event does not carry content hash, and there is no snapshot or rollback node at all.

**Crash recovery:** atomic temp+rename on `WorkspaceMcp::write_file`, all JSON stores, policy, and composio accounts (verified `persist`-based writes + `sync_all`). `StoreLock` reclaims stale locks after 30s. So: **no torn files, wedged-store recovery handled, but no semantic recovery** (undo, restore) of any kind.

---

## 12. Audit / Observability

**Layers (all verified):**
- `tracing` structured logs to stderr (fields `event=`, `action=`, `subject=`...). Env-filter feature enabled but no `EnvFilter` construction found in main (subscriber is plain `fmt()` to stderr) — the `env-filter` feature is effectively unused configuration. `RUST_LOG` does nothing today. FACT.
- `services::audit::AuditLog` — global `OnceLock` ring, 1,000 entries max, `Mutex<VecDeque>` (poison recovered via `into_inner` — deliberate), `record`/`recent`/`len`. `redact_token_like` runs at the single choke point (`record`), masking any ≥16-char base62 run — documented tradeoff (may redact UUIDs too). Good design.
- `mcp::audit::{audit_deny, audit_allow, audit_secret_deny, audit_circuit_open}` — thin wrappers that emit tracing + ring writes. All MCP security decisions funnel through these; Control API uses them too (`api/control.rs:26-27`).
- Tool metrics: per-tool-name counters (`ToolMetrics`, MAX_TRACKED_TOOLS), durations, `(tracked, calls, failures)` totals surfaced in `mcp.status`; `McpHooks` fires `ToolCallCompleted`, `InitializeCompleted`, `ResourceRead`, `PromptRequested` events for observability consumers.
- TUI logs screen reads the same global ring.

**What's missing for the stated goal** ("reconstruct: which agent changed which file, during which session, through which tool, under which policy, from which state to which state, and how it was later reverted"):
- **No agent id, no session id, no workspace id, no operation id, no snapshot id, no policy-decision reference, no before/after hash, no duration on security events, no rollback event** — because rollback doesn't exist. The `AuditEntry` schema has only `{ts_ms, kind, action, subject, detail}`. FACT. The answer to the reconstruction question is therefore: **AWH cannot reconstruct it.** Which *tool* was invoked and against *which path/subject* at *which time* — yes; who (identity), under what session, from/to what content state — no.
- **Volatility:** the ring is process-memory. Restart = erased history. There is no file-backed audit sink anywhere (`grep audit.*fs::write` → nothing). For a system whose stated purpose is unattended-agent accountability, this is a **P1**. The MCP HTTP plane and Control API run in separate processes with **separate rings** — events from one plane are invisible to the other's `/audit` endpoint.
- **Secret leakage controls:** arguments are never logged (documented at multiple call sites, and the redactor is the choke point); GitHub invocations log `owner/repo` only; connector invocations log provider+tool only. TUI dashboard logs `format!("{} {} {} ({})")` of ring entries — safe by construction. Verified clean.
- **File-content leakage:** `workspace_write` audits the path only. `files/content` Control API responses carry content over the wire (authenticated), not into audit. Clean.

---

## 13. Context Engine

`src/context/` — 12 modules, the most substantial subsystem by quality-per-line:
- **Items:** `ContextItem{id, source(File|Memory|Skill|ToolOutput), content, relevance, priority, scope(Session|Project|Global), state, protected, ...}` with id validation.
- **Budget:** token-counted (approx counter), overflow policy; `AWH_CONTEXT_*` env overrides with fail-safe defaults on malformed values.
- **Selection:** `select_within_budget` + deterministic scoring (`ScoreWeights`); `PolicyType::Deterministic` only (the enum's LLM variants don't exist yet — honest).
- **Compression:** `ContextCompressor` with options; `compress_item`.
- **Offload:** durable per-item records under `.agent/context-engine/offloads/` (atomic write, StoreLock-free but single-writer per project dir; restore round-trips).
- **Snapshots:** as noted, item-set snapshots (persisted, listable, inspectable, restorable, deletable).
- **Planner:** `DeterministicPlanner` (bounded walk ≤1,000 entries, ≤20 file hints, lexical scoring) — **DEAD_CODE today**: exported but never constructed outside its own tests. `PlanHint` includes `LikelyToolOutput`/`OffloadedContext` — the integration the planner was built for doesn't exist yet.
- **MCP exposure:** 10 tools; `context.assemble` composes per request; `get_context`/`optimize` are real. Engine disabled → tools fail with a clear error (`context()` helper); `enabled: true` default with env override `AWH_CONTEXT_ENABLED`.
- **Integration reality:** context items are inserted by *agents calling tools*. There is no automatic ingestion of file reads into context, no git awareness (planner walks the filesystem lexically, ignoring git state), no memory integration at the engine level (`memory_enabled` flag exists in config; the engine reads the MCP memory store? — not verified; the flag is plumbed but the cross-store linkage is thin). **Partially integrated**: usable standalone via MCP, not yet wired into agent workflows automatically.

**Concurrency:** engine state under `RwLock`; MCP handlers are `&self` shared — safe. Multiple agent processes each construct their own engine (per-connection dispatcher) — **cross-process consistency of the *active item set* is not attempted** (offloads are durable, active items are process-local). Documented? No. This means two agents' "context windows" diverge silently. MEDIUM.

---

## 14. Memory

**Two implementations, two formats, two planes — the cleanest duplicate-abstraction case in the repo:**

| | MCP `MemoryMcp` (`src/mcp/memory.rs`) | core `MemoryStore` (`src/core/memory.rs`) |
|---|---|---|
| File | `.agent/memory.json` | `.agent/memory.jsonl` |
| Model | `{id, scope(Session\|Project\|Global), content, tags[], created_at, updated_at}` | `{timestamp, content}` (models/memory.rs) |
| Ops | store/update_existing/get/delete/search(by substring, scope-filtered) | append/read_all |
| Lock | StoreLock across load-modify-save | **none** (plain append; append is atomic-ish for JSONL but no lock) |
| Consumers | MCP memory.* tools | Control API `/memory` (GET/POST), TUI memory screen |

Both are functional in their own plane. They **do not read each other's files**. An agent writing memory via MCP cannot see it in the TUI memory screen, and vice versa. There is no deletion/lifecycle in the core store, no identity beyond auto-timestamp in core, no permission model in either, no audit of memory mutations (memory.store is Medium-risk gated when opted in, but the ring logs only the tool name... actually `memory.store` handler does not audit subject — only the generic `tool_invoke` allow event; so content changes to memory are not attributable beyond "some tool call happened"). **DUPLICATED — MCP variant is authoritative for the agent plane; core variant is a vestige of the pre-MCP design** (spec sections 12-14 era). Recommendation embedded in §39: unify on one store.

Project vs developer vs agent vs session memory: only Session/Project/Global *scopes* exist (MCP variant). No developer memory, no agent-scoped memory (scope refers to visibility, not ownership).

---

## 15. Skills

- **Discovery:** `SkillStore` (project, `.agent/skills/<name>/SKILL.md`) + `GlobalSkillRegistry` (home-dir `~/.agent-workspace-hub/skills` or `AWH_SKILLS_ROOT`; Windows known-folders caveat documented in `control.rs:44-50`).
- **Parsing:** `parse_skill` reads front-matter (name, description, version) + markdown body. No schema validation beyond presence.
- **Install:** `RegistryClient` fetches from a skill registry URL (`DEFAULT_SKILL_REGISTRY`), **SHA-256 verified against the manifest** (installer.rs:51-52 `validate_sha256`), then extracted. Real supply-chain integrity control. Uninstall/remove present. `composio`-style toolkits aside, no signature verification (SHA-256 is integrity-not-authenticity — registry compromise still installs malware; P3 note).
- **MCP tools:** list/read/add/remove/search — add/remove are references (project references a globally installed skill), which keeps project stores light. Read returns skill content — **skills are documentation delivered to agents**, exactly per the OpenHands-style model.
- **What skills are NOT:** composable capability packages with versioning, dependency resolution, capability requirements, policy requirements, agent-specific activation, isolation, or a trust model. A SKILL.md's "Rules" section is prose for the LLM, not enforced configuration. `tool_registry.required_permissions` covers tools, not skills. No skill can grant anything; a malicious skill is only as dangerous as the prompt-injection it carries (content-level threat, not code-level — skills are never executed by AWH).

Classification: **IMPLEMENTED as a documentation/context subsystem; NOT IMPLEMENTED as a capability-package subsystem.**

---

## 16. Agent / Session Architecture

- `Agent {id, name, role, status(Created|Active|Paused|Stopped|Failed), created_at}` — persisted per-agent JSON under `.agent/agents/` with filename-safety validation (`is_safe_agent_id` rejects separators/`.`/`..`). CLI full CRUD + status. **Zero enforcement linkage** (main.rs:834-836). `id` is caller-chosen, not globally unique, not correlated to anything (MCP sessions do not carry an agent id; audit doesn't either).
- `CapabilityGrant {id = "{agent}-{permission}", agent_id, permission, scope: Option<String>, granted_at, expires_at: Option}` — `expires_at` **is never checked** (no code path reads it for enforcement; it's written and displayed only). `scope` is a free string never consumed by policy.
- `Session` (protocol): `SessionLifecycle` — solid for what it is (initialize gate, closed/failed states, metadata). Not an agent session: no identity propagation, no per-agent capabilities, no workspace binding beyond the process CWD.
- `Task` ownership: `tasks.assignee` is a free string; `awh agent create` and `tasks.update` never cross-validate (assigning a task to a nonexistent agent succeeds).
- **Disconnected concepts:** Agent ↔ Session ↔ Task ↔ Workspace are four islands with stringly-typed non-references between them. The model `Agent → Session → Task → Workspace → Worktree` does not exist as a connected graph; only the last node's path-containment is enforced.

---

## 17. Multi-Agent Readiness

**Implemented:** cross-process `StoreLock` (the strongest evidence the multi-agent same-project scenario is taken seriously); bounded stores; atomic JSON writes; memory `update_existing` race-safety; policy/trust stores shared via files.
**Not implemented:** agent registry consulted by anything; isolated worktrees/branches per agent; task ownership validation; handoff; conflict detection (two agents editing the same file = last-writer-wins with zero detection — no expected-state checks in the write path); synchronization/merge coordination; agent events; shared context with coherence; resource locking on *files* (locks cover only JSON side-stores).
**Implied but absent:** everything in roadmap Phase 3 "First-class agent worktrees" and Phase 4 per-agent scoping.

Verdict: multi-agent today means "multiple processes don't corrupt each other's JSON stores." It does not mean isolation, attribution, or coordination. **Score 1/5.**

---

## 18. Terminal / Process Security

`TerminalService::run` (services/terminal.rs):
- argv-only; program whitespace rejected (anti-embedded-command-line); `kill_on_drop(true)`; 30s wall clock (`tokio::time::timeout` over `wait_with_output`); 256 KiB capture cap with `truncated` flag; audit hook **exists but is never attached by any caller** (`with_audit_hook` has zero production call sites — the dispatcher audits `tool_invoke` at the MCP layer instead; the service-level hook is dead API surface). FACT.
- **cwd = workspace root** (dispatcher passes `self.workspace.root()`); the Control API passes `state.root` (server CWD). A `program` value may be an absolute path — no allowlist, no path constraint on the *program*, only on the CWD. `terminal.run` with `program: "/bin/sh", args: ["-c", "anything"]` fully escapes any notion of workspace containment — by design (it's a High-risk, Process-permission tool), and the opt-in builtin gate is the only thing that can stop it, which it won't unless the operator created a trust record. Policy can deny by exact program name (`/bin/sh` — but then `/bin/bash`, `/usr/bin/env`, ... each need their own rule; DENY-only with exact-match means the allowlist direction is impossible today). **This is the honest, documented posture, but it must be stated plainly: with default configuration, any connected MCP client has unrestricted shell on the host with the server's environment.** P1.
- Environment: fully inherited (including secrets in env). No env filtering on terminal.run (unlike custom MCP servers, which get `is_blocked_environment` filtering — an inconsistency worth flagging: **external servers get tighter env handling than the built-in terminal**). MEDIUM.
- No resource limits (CPU/memory/IO) beyond the time cap. Sandbox config exists (`mcp/sandbox.rs` — Linux-only, `SandboxConfig::new` rejects relative roots, `wrap_command` with `SandboxLimits`) but is applied to **custom MCP server spawning** only, not to `terminal.run`. Verified via tests (`tests/mcp_sandbox.rs` is `#[cfg(target_os = "linux")]`).

---

## 19. Control API / Remote

`api/control.rs` — axum, `/api/v1`:
- **Auth:** single bearer token, constant-time (`subtle::ConstantTimeEq`), from `AWH_API_KEY` (or named env). Health endpoint unauthenticated by design. 30s timeout layer, 1 MiB body limit, sliding-window rate limit **after** auth (spec §25 chain ordering documented in code).
- **Surface:** status, projects CRUD, files (list/read/write/search/meta/rename/create-dir/delete-entry), git (status/log/diff/branch/branches/push/pull/stage/unstage/commit), **terminal/run**, context read/write, memory list/append, skills list + project skill refs, mcp list (registry info), audit, logs.
- **Errors:** structured `{"error":{code,message}}`; internal errors logged server-side, generic to the client (`ApiError::internal` — paths/tool names deliberately hidden). Good practice.
- **Weaknesses:** no TLS (served over plain `axum::serve` in `serve_control_api`; the TLS machinery exists only on the MCP HTTP plane); no scopes/roles; `X-Forwarded-For`-keyed rate limiting (spoofable); CORS not configured here; `/terminal/run` as described in §18; git push/pull/commit without any confirmation flow (the "UIs must confirm" convention is entirely client-side). Combined with `awh tunnel` (ngrok argv-spawn, refuses non-loopback unless opted in — but the opt-in is a single bool), a public URL + one token = full host control, in cleartext if TLS isn't terminated upstream.
- **TUI remote mode** (`tui/remote.rs`, 728 lines): full `WorkspaceBackend` over reqwest against this API. Real, tested-ish (in-module tests exist), and the only "remote" story. **Classification: PARTIALLY_IMPLEMENTED, production-usable on a trusted network only; not production-ready as exposed.**

---

## 20. Persistence

| Store | Owner | Format | Location | Atomic? | Cross-proc lock | Migration | Corruption behavior |
|---|---|---|---|---|---|---|---|
| memory (MCP) | MemoryMcp | JSON array | `<proj>/.agent/memory.json` | temp+rename | StoreLock | none | `serde` error → tool error (fail closed, store unreadable) |
| tasks | TasksMcp | JSON | `.agent/tasks.json` | temp+rename | StoreLock | none | fail closed |
| connectors | ConnectorsMcp | JSON | `.agent/connectors.json` | temp+rename | StoreLock | none | fail closed |
| policy | PolicyStore | JSON array | `.agent/policy.json` | temp+rename | StoreLock | none | **fail closed as internal error** (dispatcher path) |
| trust | PersistentTrustStore | JSON | `~/.agent-workspace-hub/trust.json` (`AWH_TRUST_DIR`) | save() | none documented | none | unreadable → `None` → custom MCP all denied (fail closed); **builtin gate also denies Medium/High when store unreadable only if a record exists** — wait: `authorize_builtin_tool` with `trust: None` (store unavailable) denies. Verified: `let Some(store) = trust else { audit_deny(...); return Err(StoreUnavailable) }` — so a corrupt trust.json **breaks all Medium/High built-in tools** until fixed, even without opt-in records. Interesting: load failure returns None → deny. That's fail-closed but surprising for a workspace that never opted in. P3 (footgun). |
| agents / grants | AgentStore / CapabilityGrantStore | per-record JSON | `.agent/agents/`, `.agent/capability-grants/` | plain write | none | none | per-file |
| memory (core) | MemoryStore | JSONL append | `.agent/memory.jsonl` | append | none | none | partial-line risk |
| context.md | ContextStore | text | `.agent/context.md` | plain write | none | none | — |
| context items | ContextEngine | in-proc + JSON offloads | `.agent/context-engine/offloads/` | atomic per item | none (single-writer) | none | — |
| context snapshots | SnapshotStore | JSON per snapshot | `.agent/context-engine/snapshots/` | atomic | none | none | per-file |
| skills | SkillStore / GlobalSkillRegistry | dirs + SKILL.md | `.agent/skills/`, `~/.agent-workspace-hub/skills` | plain write | none | none | parse error per skill |
| composio accounts | ComposioRegistry | JSON | `~/.agent-workspace-hub/composio_accounts.json` | atomic | StoreLock | none | fail closed |
| custom MCP registry | CustomMcpRegistry | JSON | `<proj>/.agent/mcps.json` (per earlier reading) + global `mcps.json` | atomic | StoreLock | none | fail closed |
| audit | AuditLog | **memory only** | — | — | — | — | lost on restart |

No SQLite, no databases, no versioned schema, no migration machinery anywhere. All JSON deserialization is fail-closed. Backup/recovery = "git your workspace". The `.agent/` convention is consistently applied and skipped by `files.search`. **Storage abstraction inconsistency:** per-record files (agents/grants) vs whole-array files (policy) vs JSONL (core memory) vs JSON (MCP memory) — three shapes for the same concept class, each with its own locking story (or none).

---

## 21. Concurrency & Race Analysis

Verified primitives: `Arc` everywhere on the dispatcher path; `RwLock` (providers registry, context engine items); `Mutex` (audit ring, SessionLifecycle metadata fields, TUI paths recorder in tests); `AtomicU8` (session state, EditId sequence); `StoreLock` (cross-process); CAS in `mark_initialized`.

**Risks found:**
1. **Final-component TOCTOU** on file writes (both FS implementations) — §7. Exploitable only by a concurrent in-workspace writer; in the two-agents-one-project scenario that's exactly the threat model AWH advertises. **P1.**
2. **Git index races:** two agents staging/committing concurrently → git's own lockfile errors surface as tool errors; no AWH-level serialization of git mutations. Data-loss risk: none (git refuses), confusion risk: high. **P2.**
3. **`FilesService` constructed per call** (dispatcher `GitService::open(root)` per call too): no shared state, so no data races — but also no cross-call coordination. The `StoreLock` JSON stores serialize correctly; files do not.
4. **ContextEngine per-connection instances** (dispatcher per SSE session/stdio connection): active-item sets diverge across agent processes; offloads/snapshots are shared files but item identity is process-local. No detection. **P2.**
5. **Deadlock surface:** `StoreLock` is a blocking file-lock with retry (25ms interval, 10s timeout) acquired inside async tool handlers — a wedged holder blocks a tokio worker thread for up to 10s (the docs say "blocks the current thread"). In the multi-threaded HTTP runtime this is survivable; in a current-thread runtime (TUI) it freezes the UI. Documented, acceptable, worth noting.
6. **Session-close race:** SSE `mark_closed` on guard drop; a POST arriving between close and registry eviction gets `unknown session` — handled. `SessionGuard` implements `Stream` with an unpinned-fields SAFETY comment (http.rs:391) — reviewed, looks sound.
7. **Double-initialize race:** CAS-guarded (§4). Good.
8. **Audit ring lock poisoning** handled via `unwrap_or_else(|p| p.into_inner())` — deliberate, documented.
9. **Shutdown races:** no graceful shutdown anywhere; `kill_on_drop` saves child processes; temp files on crash: NamedTempFile cleans on drop; on SIGKILL, stale `.lock` files reclaimed after 30s — the system was actually thought through here.

No `unsafe` blocks found in production paths (only the documented SAFETY comment for the Stream impl, which uses unsafe internally for pin projection — http.rs:391; single unsafe site).

---

## 22. Error Handling

- **MCP plane:** `DispatchError {code, message}` with JSON-RPC codes (`invalid_params` -32602, `invalid_request` -32600, `internal`, `POLICY_DENIED_CODE`, `BUILTIN_TOOL_DENIED_CODE`, `SERVER_NOT_INITIALIZED_CODE`); `PolicyDenialError::Denied{tool, rule_id, pattern, reason}` — actionable, names the exact missing permission/rule. Transport-independent and consistent. Errors from `anyhow` get stringified into `DispatchError` via `to_dispatch_error` — **type/cause information is flattened to a string at this boundary** (the MCP plane cannot distinguish "file too large" from "disk full" from "permission denied" — all become one internal-error text). Information loss: moderate. P3.
- **Control API:** structured codes (`not_found`, `bad_request`, `unauthorized`, `internal`), details hidden from clients, logged server-side. Good.
- **TUI/CLI:** anyhow chains with `{:#}` display — good CLI errors.
- **Unwraps in production code:** counting only pre-`#[cfg(test)]` lines: total is small (~15 across all of `src/`); the notable ones are test-harness construction (`dispatcher_with_github_stub`), schema-internal `unwrap`s on validated-constant paths, and `VecDeque`/`OnceLock` initialization in `audit.rs` (poison-recovered). No `.unwrap()` was found on an I/O or input-dependent path in the dispatcher's tool handlers (they all use `?` + `strval`). `main.rs:797` has `McpCommand::Serve { .. } => unreachable!()` — a genuinely unreachable match arm guarded by the earlier special-case; acceptable but a `debug_assert` + fallback would be safer. **No panic paths found reachable from MCP input.** HIGH confidence: schema validation runs before dispatch, and every handler binds arguments via `?`-returning helpers.
- **Swallowed errors:** `load_trust_store` returns `None` on unreadable store and *warns* — fail-closed consequences documented above. `runtime.block_on` in TUI dashboard ignores git errors (`if let Ok`) — deliberate (dashboard shouldn't crash). `dispatcher.github = ... .ok()` on provider init failure — logs a warning, tool calls fail closed later. All conscious, none silent-fatal.

---

## 23. Dependencies

All 27 direct deps justified by observed call sites; no unused-heavy dependencies found (checked: `mime` used in HTTP responses; `futures-util` in SSE streams; `tokio-stream` in SSE; `rustls-pemfile` in TLS acceptor; `windows-sys` in Windows job-object code — platform-specific process management, presumably for sandbox/child cleanup on Windows; `subtle` for constant-time token compare; `sha2` for skill integrity + edit hashes).
- **`reqwest` with both `rustls-tls` and `blocking` features** — the blocking feature is for CLI registry/skill fetches inside sync `main` paths; acceptable but pulls a second reqwest runtime flavor. Fine.
- **No duplicate crypto/HTTP/serialization libraries.** `chrono` for all timestamps (consistent RFC 3339).
- **Termux/Android:** no `openssl` anywhere (pure-rust TLS via rustls+ring) — this is the correct choice for Termux. `aarch64-linux-android` release target with NDK setup verified in workflow. No glibc-only assumptions spotted in the audited paths (no `libc` direct usage found; `windows-sys` is cfg-gated). `dirs` crate handles Android home quirks partially (`GlobalSkillRegistry::discover()` on Windows uses known-folders; Android data-dir semantics untested — NOT VERIFIED).
- **`thiserror 1`** (not 2) and `chrono 0.4` with clock+serde only — lean. No `cargo audit`/`cargo-deny` in CI (**no dependency vulnerability scanning at all** — P2).

---

## 24. Dead / Duplicate / Legacy Code

**Duplicates (each with the authoritative variant identified):**
1. **Filesystem containment ×2**: `services::files::FilesService` (Control API + TUI; plain `fs::write`) vs `mcp::workspace::WorkspaceMcp` (MCP; atomic write). Different caps (8 MiB vs 2/5 MiB), different TOCTOU posture, same concept. **Authoritative for security review: WorkspaceMcp (better); the service should adopt its write path.** The services-layer doc claim ("single owner of business logic") is contradicted here.
2. **Memory ×2**: `.agent/memory.json` (MCP, authoritative for agents) vs `.agent/memory.jsonl` (core, Control API + TUI). Formats invisible to each other.
3. **Task model ×2**: `mcp/tasks.rs::Task{id,title,description,status:Todo|InProgress|Blocked|Done,priority,assignee,tags}` vs `models/task.rs::Task{id,title,status:Pending|InProgress|Completed|Cancelled}` — **different status vocabularies for the same concept**, plus a third store `core::tasks::TaskStore` using the models variant with **zero non-core callers**. The models Task + core TaskStore + `models/policy_rule`-adjacent types form the legacy pre-MCP design stratum.
4. **Terminal audit hook**: service-level `AuditHook` API with zero callers (dispatcher audits at its own layer).
5. **ContextStore vs WorkspaceMcp::context()**: both read project context files (`.agent/context.md` vs AGENTS.md/AGENT.md/README.md) — two different "project context" notions (the Control API `/context` writes `context.md`; MCP `workspace.context` reads the doc files). Conceptual duplication with different semantics — worse than same-semantics duplication because users will expect them to be the same thing.

**Dead code:**
- `core::files::FileStore` — no callers outside its module.
- `core::tasks::TaskStore` — no callers outside core.
- `context::planner::{ContextPlanner, DeterministicPlanner, PlanHint}` — exported, never constructed.
- `services::terminal::AuditHook`/`with_audit_hook` — never attached.
- `GitService::{push, pull, discard_file, hard_reset, force_push, clean, delete_branch, remote_url is used}` — push/pull used by TUI + Control API only; the five destructive ops have **zero** callers.
- `HighRiskGitOp` enum — taxonomy only.
- `tokio` `signal` feature — enabled, unused.
- `tracing-subscriber` `env-filter` feature — enabled, no `EnvFilter` built.

**Legacy strata:** `core/*` (files/memory/tasks/context/workspace) is the pre-MCP "spec sections 12-14" era; `services/*` is the "spec 16/25/26" era; `mcp/*` stores are the current agent-plane era. Three generations coexist; the `architecture.rs` conformance tests enforce only that MCP doesn't import the Control API — nothing prevents MCP from bypassing services (which it does for files/memory/tasks/connectors).

TODO/FIXME/HACK markers: essentially none in production code (the codebase's discipline shows here) — grep found only benign matches. The repo communicates incompleteness through phase-doc comments instead, which is better but leaves the code looking more complete than it is.

---

## 25. Testing Forensics

**Volume:** 12 integration files (149 fns) + 485 in-module fns ≈ 634 test functions. CI runs `cargo test --all-targets` on a single ubuntu job.

**What's genuinely tested (verified by reading the test files):**
- **MCP protocol conformance:** `tests/mcp_protocol.rs` (38 tests) — envelope validation, notifications, initialize lifecycle, error codes, `prompts/*`, `resources/*`.
- **Real-filesystem + real-git integration:** dispatcher tests build real tempdir workspaces and run real `git init` + stage/commit/log round-trips (`git_tools_round_trip_in_real_repo`); `workspace_write_rejects_traversal_through_tools_call` asserts the escaped file does not exist. **This is real-world validation, not mocks.** Strong.
- **Authorization:** `tests/mcp_builtin_tool_gate.rs` (18) — backward-compat default-allow, least-privilege denial, permission-named errors, store-unavailable fail-closed, unregistered-tool denial; `tests/mcp_policy_gate.rs` (14) — deny-narrows, precise matching, CLI round-trip, coarse-gate-before-policy ordering; `tests/mcp_security.rs` (16) — custom-server trust matrix, env filtering, blocked env names; `tests/mcp_sandbox.rs` (7, Linux-gated) — sandbox config validation.
- **Transports:** `tests/mcp_http.rs` (14) — real axum server, auth, TLS?, session lifecycle over HTTP; `tests/mcp_server.rs` (16) — stdio loop; `tests/mcp_executable.rs` (3) — binary-level.
- **Dynamic tools:** `tests/mcp_dynamic_tools.rs` (11) — provider aggregation, schema gating, `connector.invoke` validation parity.
- **Architecture conformance:** `tests/architecture.rs` (3) — textual boundary enforcement (a creative and effective use of tests).
- **Property-based:** proptest for schema validation (schema.rs:2658+).
- **Agent CLI:** `tests/agent_cli.rs` (3) — record CRUD only (correctly scoped, since there's nothing to enforce).
- **Benchmark-ish:** `examples/bench.rs` measures schema-validation hot paths manually (no criterion; fine).

**What's NOT tested / false-confidence risks:**
- **No test exercises an `EditTransaction` against files** (nothing to test — the executor doesn't exist).
- **No rollback/snapshot/provenance tests** (nothing to test).
- **No test that two concurrent agent processes race a file write** (the TOCTOU finding is untestable today because no harness spawns two dispatchers against one dir with a hostile interleaver). The multi-process `StoreLock` tests exist for JSON stores; **file-mutation races are untested.**
- **No concurrency stress tests** (no loom, no multi-process harness).
- **CI is single-OS**: Windows-specific code (`windows-sys` job objects) and the Windows lockfile path (`CREATE_NEW` mapping) ship in releases **without ever being tested in CI** (the release matrix builds them but `main.yml` runs tests only on ubuntu). macOS and Android targets likewise compile-but-never-test. **P2.**
- **No MCP-client-interop CI against a reference client** (the `examples/mcp-interop` harness exists but is manual; no `cargo deny`/audit).
- Tests are honest: several explicitly pin fail-closed contracts ("must fail closed", "must not panic") rather than happy paths. No test was found that mocks away core behavior to fake a pass.

---

## 26. CI / Release

- `main.yml`: fmt --check; clippy `-D warnings` all-targets/all-features; `cargo test --all-targets` — ubuntu only. Solid quality gate, narrow OS matrix.
- `release-rust.yml`: `needs: verify`; 6-target matrix incl. `aarch64-linux-android` with NDK; artifact upload with per-target names; no checksums (no `sha256sum` step found), **no signing**, no SBOM, no provenance attestation. Installer script exists but release artifacts are bare binaries. **P3** (checksums at minimum).
- No scheduled dependency audit, no `cargo-audit`/`cargo-deny`, no MSRV job, no coverage, no cross-compile smoke test of the test suite (tests run on ubuntu only, so `windows-sys`/macOS code is untested).
- GitHub context in-repo: workflows only; no other CI config. The CI does **not** validate the supported product surface (Windows/Android binaries are never executed).

---

## 27. Android / Termux

- **Release artifacts:** `awh-android-aarch64` built via NDK in CI — the packaging intent is real. FACT.
- **Compile-ability vs testability:** Android/Termux is *theoretically compilable* (pure-Rust TLS, no OpenSSL, no glibc-only calls found in audited paths, `dirs` for home) but **zero tests run on Android and zero Termux-specific handling exists** (no termux-path detection, no `termux-info` integration, no assumptions about `$PREFIX`, no proot/sdcard filesystem quirks handling — sdcardfs famously breaks `O_EXCL`-style locking semantics and rename atomicity, which `StoreLock` and the atomic-write pattern depend on). **`StoreLock`'s create_new semantics and temp-file rename durability on Android shared storage are UNVERIFIED and plausibly broken.** LOW-MEDIUM confidence (inference from filesystem semantics), flagged as needing a real Termux test.
- Process model: `tokio::process` on Android requires API-level-appropriate `waitpid` behavior — standard Rust handles it; the sandbox module is `#[cfg(target_os = "linux")]` so **Android gets the Linux sandbox path** (or none if the cfg doesn't match Android — `target_os = "linux"` does NOT match `android`; `cfg(target_os="linux")` is false on `aarch64-linux-android`, so **custom-MCP sandbox wrapping is compiled out entirely on Android**). MEDIUM-HIGH confidence this is an unnoticed platform gap. P2.

---

## 28. Documentation vs Reality Matrix

| Feature | Docs say | Code does | Tests prove | Status |
|---|---|---|---|---|
| MCP server (stdio/SSE/HTTP) | implemented (roadmap Ph1) | implemented | yes | **IMPLEMENTED** |
| 65 built-in tools | mcp.md catalog | 65 verified | tools/list pinned in tests | **IMPLEMENTED** |
| Custom MCP trust gate | security.md | fail-closed store | 16 tests | **IMPLEMENTED** |
| Built-in tool gate | Phase 2 docs | opt-in, default-allow | 18 tests | **IMPLEMENTED (opt-in semantics)** |
| Policy DENY rules (3 tools) | Phase 3 docs | implemented | 14 tests | **IMPLEMENTED (narrow)** |
| Context engine | Ph6 docs + mcp.md | implemented, MCP-exposed | extensive in-module tests | **IMPLEMENTED** |
| Skills install w/ SHA-256 | README/docs | implemented | installer tests | **IMPLEMENTED** |
| Editing subsystem | **AWE prompts + roadmap Ph2 "Controlled editing"** | model types only; no executor, no exposure | unit tests of `validate_shape` only | **SCAFFOLD_ONLY** — docs avoid overclaiming in AWE master prompt, but roadmap Ph2 checklist style implies more |
| Snapshots/undo/provenance | Ph5 full command set | **absent** | n/a | **MISSING** |
| Worktrees / agent isolation | Ph3 "first-class agent worktrees" | **absent** | n/a | **MISSING** |
| Agent registry + capability grants | Phase 4 "Capabilities" | records exist; **not consulted by execution** | CRUD tests | **DECLARED_BUT_NOT_CONNECTED** (code comments admit it) |
| Provenance fields (file/agent/session/tool/hashes) | Ph5 list | **absent from all records** | n/a | **MISSING** |
| `git.branch` "create or switch" | tool_registry metadata | reads current branch only | tests pin read semantics | **METADATA DRIFT** |
| Push/pull via MCP | not claimed in tool docs | not exposed (service + TUI only) | — | consistent |
| Memory | Ph "memory" | two divergent stores | both tested separately | **DUPLICATED** |
| Remote/Termux | README mentions Android release | artifact built, untested; sandbox cfg excludes android | none | **UNVERIFIED** |
| Audit | security.md "audit trail" | in-memory ring, 1000 entries, process-local | redaction tests | **PARTIALLY_IMPLEMENTED (volatile)** |
| `awh explain <file>` | Ph5 | absent | n/a | **MISSING** |
| TUI remote backend | backend doc comment "future HTTPS backend" | **implemented** (728 lines) | in-module tests | docs understate; **IMPLEMENTED** |

The documentation set is unusually candid (the AWE master prompt's "Do not claim those features are complete" clause, `mcp.status`'s honesty, phase-gate comments). The main doc-vs-code contradictions run in the *other* direction than usual: several docs **understate** implemented features (TUI remote), while roadmap checklists present absent subsystems as mere checklist items — the roadmap's own gap matrix (`docs/ROADMAP_GAP_MATRIX.md`) exists and partially does this reconciliation.

---

## 29. Roadmap vs Reality

| Phase | Claimed | Observed | Actual estimate |
|---|---|---|---|
| 0 Foundation & release infra | done | binaries + CI + install script present | ~90% (no signing/checksums) |
| 1 MCP infrastructure | done | dispatcher, transports, trust gate, interop harness | **~85%** (no cancellation, no isError, rate-limit keying weak) |
| 2 Workspace runtime | partially | files tools real; **controlled editing = model only** | **~45%** (editing is the headline gap) |
| 3 Git & workspace isolation | partially | git service solid; **worktrees absent**; destructive ops uncalled | **~35%** |
| 4 Capability & policy engine | partially | builtin gate + DENY policy real; **agent grants inert**; no per-agent scoping | **~40%** |
| 5 Snapshots/undo/provenance | planned | **absent** | **0%** |
| 6 Context engine | done | substantial engine + tools | **~70%** (planner dead, cross-process coherence absent) |
| 7 Memory (per roadmap sections) | done | two divergent stores | **~60%** (quality high, unification owed) |
| 8 Skills | done | store/install/registry + integrity | **~75%** |
| 9-10 (later phases incl. TUI/remote) | done-ish | TUI 13 screens + remote backend + tunnel | **~65%** |

The repo's own `docs/PROJECT_STATUS.md` / `completeness-audit.md` / `ROADMAP_GAP_MATRIX.md` were reviewed; where they assign percentages, the code broadly supports the lower-band claims (the repo is not inflating), with the notable exception that any external summary describing "editing with verification and rollback" would be inflated — the internal docs do not make that claim.

---

## 30. Architecture Integrity Review

Thesis: *"agent-agnostic, MCP-first workspace runtime rather than an AI agent framework."*

**Drift check:** The codebase resists framework drift admirably. `DeterministicPlanner` explicitly documents "an LLM planner can slot in later" but the default "needs no model at all"; `PolicyType::Deterministic` is the only variant; no LLM orchestration, no model router, no prompt engine exists anywhere. AWE master prompt's §1 ("Do not add: LLM reasoning / autonomous planning / model providers...") is reflected in the code. **No architectural drift toward an agent framework. Verdict: thesis holds.**

Scores (0=absent … 5=production-grade):

| Principle | Score | Evidence |
|---|---|---|
| MCP-first | **3** | MCP is the richest, best-tested plane; but MCP bypasses `services/` for files/memory/tasks/connectors, so "first" is true while "canonical core" is not — the MCP plane *is* its own core for those domains |
| Local-first | **4** | Everything except connectors/Composio/GitHub/skill-registry/tunnel is local; no cloud dependency; pure-rust TLS |
| Agent-agnostic | **4** | No agent assumptions embedded; identity hooks deliberately absent rather than wrong |
| Capability-controlled | **2** | Real gates for external servers; **opt-in only** for built-ins (default = unrestricted); DENY-only policy for 3 tools; grants inert |
| Workspace-centric | **3** | One-dir-one-project model enforced by path containment; workspace-as-entity (identity, history) absent |
| Reversible | **1** | Context-item snapshots only; no file-level undo, no provenance, no edit rollback — the "reversible" promise is almost entirely future work |
| Observable | **2** | Ring + tracing + per-tool metrics; no persistence, no correlation ids, no cross-plane aggregation |
| Composable | **2** | Provider/trait seams exist (`McpClient`, `TunnelProvider`, `WorkspaceBackend`, `ContextPlanner`); skills are prose not capabilities; composition story is mostly interface-shaped |

---

## 31. Threat Model

| # | Threat | Actor | Existing mitigation | Actual enforcement | Residual risk | Severity |
|---|---|---|---|---|---|---|
| T1 | Path traversal / symlink escape | untrusted agent (MCP client) | dual containment checks (§7) | enforced, tested | final-component TOCTOU | **P1** |
| T2 | Arbitrary command exec | untrusted agent | `terminal.run` High-risk metadata; builtin gate **opt-in**; policy exact-program DENY | none by default | full host shell by default | **P0** |
| T3 | Credential leakage via env inheritance | untrusted agent via terminal.run | none for terminal (custom MCP servers DO get env filtering) | inconsistent | `GITHUB_TOKEN`/`COMPOSIO_API_KEY` readable via `env` command | **P1** |
| T4 | MCP remote exposure | remote client | bearer token (constant-time), rate limit, body/timeout limits | enforced | **optional TLS, default 0.0.0.0 bind** → token MITM-able | **P0** |
| T5 | Control API over tunnel | remote attacker | token, loopback-pref for tunnels, ngrok argv-safe | enforced | single-scope token = full host; no TLS on control plane | **P1** |
| T6 | Malicious custom MCP server | compromised dependency | trust store fail-closed, env filtering, sandbox wrap (Linux), circuit breaker, schema validation | enforced | sandbox cfg excludes Android; secrets listed in config could be over-broad if operator approves blindly | P2 |
| T7 | Malicious skill | compromised registry | SHA-256 manifest check | integrity only | registry compromise = code-as-prompt injection into agents (skills are consumed by LLMs, not executed — content-level threat) | P2 |
| T8 | Git destruction | untrusted agent | destructive ops **not exposed** anywhere today | by absence | the moment `hard_reset`/`clean` get wired, only convention protects | P2 (latent P0) |
| T9 | Cross-agent data leakage | sibling agent | per-project dirs; scopes on memory/context | partial | same-project agents share all memory/tasks/context by design; `.agent` readable by `workspace.list_files`? — **no: `list_files` lists any dir including `.agent` (it's not excluded, unlike `search` which skips it)** — agents can read each other's stores via `workspace.read_file(".agent/policy.json")` etc. Minor info disclosure (stores hold no secrets by design) but policy.json/trust adjacency is observable. P3 |
| T10 | TOCTOU on write | sibling agent | none | none | §7/§21 | P1 |
| T11 | Audit loss | any | ring redaction | none | restart wipes forensics; 1,000-entry bound drops history | P1 |
| T12 | Policy store corruption | malicious workspace file | fail-closed as internal error | enforced | DoS of gated tools by a corrupt file (requires write access to `.agent` which an agent has via `workspace.write_file`! → **an agent can corrupt `.agent/policy.json` or write rules... no — add requires... it can OVERWRITE the file with garbage, denying all three gated tools, or overwrite trust.json? no, that's in home. It can overwrite policy.json with valid JSON containing rules to deny the human's favorite tools — policy is DENY-only so an agent can only make things *more* restrictive, not less; corrupting it denies tools (DoS). Also `.agent/trust.json` is in home dir — not reachable. **An agent CAN rewrite its own project's `.agent/mcps.json` custom-server registry** (it's a workspace file) to register a custom MCP server… but the gate then requires trust approval from the home-dir store, so fail-closed holds. Good depth here.) | net: DoS-only | P3 |
| T13 | Supply chain (Cargo) | n/a | none in CI | none | no cargo-audit | P2 |
| T14 | Sandbox escape (Linux) | custom MCP server | wrap_command_with + SandboxLimits, job objects on Windows | partially (Linux-only, untested on Android) | sandbox module exists for servers; `terminal.run` unsandboxed | P2 |

**Most dangerous single issue:** T2+T4 combined — a default-configured remote MCP (`awh mcp serve --transport sse`, binds `0.0.0.0:8443`, no TLS required) with a leaked or guessed token gives a remote party unrestricted command execution. The token is the only barrier and it may travel in cleartext.

---

## 32. Reliability / Failure Analysis

| Failure | Expected | Actual | Recovery | Data-loss risk |
|---|---|---|---|---|
| Process crash mid-write | no torn store files | temp+rename everywhere on stores; `WorkspaceMcp` writes atomic; **`FilesService::write` is plain `fs::write`** (Control API/TUI path CAN tear) | restart; stale `.lock` reclaimed at 30s | low (stores) / **medium (Control-API-written files)** |
| MCP disconnect | session closed | SSE guard drops → registry cleanup; session marked closed; later POSTs → `unknown session` | client reconnects, re-initializes | none |
| Disk full | clear error | `fs::write` errors propagate as tool errors; temp-file creation fails cleanly | manual | none beyond the failed op |
| Concurrent modification of a file being edited | stale-state detection | **none** — `workspace.write_file` overwrites unconditionally; expected-state model exists only in unused edit types | none | **high** (silent last-writer-wins) |
| Git conflict (pull) | surfaced | `GitOutput` with non-zero exit + stderr returned (TUI comment documents this contract) | agent resolves manually | low |
| Snapshot failure | n/a | no file snapshots | — | — |
| Rollback failure | n/a | no rollback | — | — |
| Corrupted JSON store | fail closed | all loaders error → tool error; policy → internal error; trust → None→deny-all (custom MCP) / **deny-all Medium+High builtins** (surprising, §20) | delete/regenerate file | low |
| Service init failure (github/context engine) | degrade | provider left `None`, tools unadvertised + fail-closed calls; engine disabled → tools error with clear message | set env and restart | none |
| Agent termination | locks released | `StoreLock` stale-reclaim after 30s; `kill_on_drop` reaps children | automatic | low |
| TUI crash | state lost? | TUI is stateless client; all mutations already persisted | relaunch | none |
| Wedged lock holder | timeout | 10s bounded acquire → clear error | retry | none |
| Rate-limit bucket shared ("direct") | per-client | all direct HTTP MCP clients share one bucket | tune limits | DoS quirk, P3 |

---

## 33. Critical Invariants

| Invariant | Implementation | Enforcement point | Test | Violable today? |
|---|---|---|---|---|
| No workspace escape (reads) | `safe_path` canonical containment | WorkspaceMcp | yes (traversal test asserts no file created outside) | no (static) |
| No workspace escape (writes) | `safe_new_path` + ancestor canonicalization | WorkspaceMcp | yes | **yes — final-component race (T1/T10)** |
| No traversal via rename/delete | both-endpoint resolution | FilesService | service tests | no (static) |
| No unauthorized mutation (external MCP) | trust gate | execution_gate | 16 tests | no |
| No unauthorized mutation (built-in tools) | opt-in gate | execution_gate | 18 tests | **yes by design (default-allow)** |
| No silent stale-state overwrite | ExpectedState model | **none — unimplemented** | n/a | **yes — every `workspace.write_file`** |
| No partial multi-file mutation | no-partial-preparation invariant (AWE-004 spec) | **unimplemented** (no multi-op tool) | n/a | n/a (feature absent) |
| Rollback restores expected state | n/a | **absent** | n/a | **n/a — feature absent** |
| Agent isolation | per-project dirs only | path containment | partial | **yes — same-project agents share everything; `.agent` readable** |
| Audit identity preserved | ring entries | AuditLog::record | redaction tests | **yes — no agent/session ids; restart erases** |
| MCP and CLI invoke the same core logic | services layer doctrine | — | **architecture.rs tests enforce only the MCP↔Control-API boundary** | **yes — files/memory/tasks have divergent MCP vs core implementations** |
| No tool runs without schema validation | `validate_tool_arguments` before dispatch | call_tool | proptests + protocol tests | no |
| Custom MCP env/secrets filtering | `is_blocked_environment` | custom_mcp spawn | security tests | no |
| Sessions must initialize first | SessionLifecycle gate | all three transports | lifecycle tests | no |

The three invariants AWH *cannot* currently guarantee — stale-state overwrite protection, rollback, and unified core logic — are precisely the ones its product story leans on.

---

## 34. End-to-End Workflow Traces

**A — File Read (MCP):** `tools/call workspace.read_file{path}` → envelope validation → session Ready check → schema validation → (Low risk: no gate) → `WorkspaceMcp::read_file` → `safe_path` (component scan + canonicalize + containment) → size check → UTF-8 read → JSON content. **Missing links: none material** (no audit on read — deliberate; reads aren't security events). Status: **complete**.

**B — File Edit (the promised one):** `Agent → MCP → capability → policy → EditService → FilesService → verification → audit`. Actual: `workspace.write_file` → gate (opt-in) → policy prefix check → `safe_new_path` → temp+rename. **Missing: capability enforcement is default-off; there is no EditService, no expected-state check, no verification, no before/after hash, no rollback path.** The write is atomic on disk but semantically blind. Status: **5 of 9 links present**.

**C — Git Commit:** `git.commit{message}` → schema → gate (opt-in; no policy — `git.commit` is not in `RESOURCE_SCOPED_TOOLS`) → `GitService::open(root)` → `["commit","-m",msg]` → timeout 30s → exit≠0 → Err(stderr) → JSON-RPC error; audit: generic `tool_invoke` allow + failure event if failed. **Missing: no attribution (who committed), no hook for user identity, no worktree isolation; git.commit policy-scoping absent (only 3 tools are policy-checkable).** Status: **functional, attribution-free**.

**D — Snapshot/Rollback:** **does not exist for files.** The context-engine analog (items) is complete: `context.offload` → durable record → `context.restore` → rehydrate; `engine.snapshot` → file → `restore_snapshot`. But no tool exposes engine snapshots (only internal API). Status: **absent for workspace mutation; partial for context items**.

**E — Agent Session:** `Agent` (CLI record) — never consulted. `SessionLifecycle` — protocol only. Capabilities — inert. Task — free-string assignee. Result: an agent connects via MCP, gets the **full default toolset** (minus gated opt-ins), operates on the shared working tree, and is recorded in the audit ring only as tool names. **No link in the chain Agent→Session→Capabilities→Tools exists.** Status: **disconnected**.

---

## 35. P0 Findings

**P0-1. Default-allow capability posture for all Medium/High built-in tools.**
- **Evidence:** `src/mcp/execution_gate.rs:99-133` — `authorize_builtin_tool` returns `Ok(())` when no `awh.builtin` record exists; the doc comment says this is deliberate backward compatibility.
- **Impact:** any connected MCP client (the "untrusted AI agent" threat actor) can run `terminal.run` (arbitrary host commands), delete files, commit, and invoke High-risk `github.*` mutations with zero capability checks in the default deployment.
- **Root cause:** Phase-2 design chose opt-in to avoid breaking existing users before a migration path exists.
- **Affected modules:** `mcp/execution_gate.rs`, `mcp/dispatcher.rs`.
- **Exploit scenario:** agent connects over stdio; calls `terminal.run {"program":"/bin/sh","args":["-c","cat ~/.agent-workspace-hub/trust.json"]}`; succeeds by default.
- **Recommended fix:** flip the default for High-risk tools only (`terminal.run`, `github.*` mutations, `connector.invoke`) to deny-without-record while keeping Medium workspace mutations default-allow; ship a one-line `awh mcp trust awh.builtin --process --network --filesystem` onboarding step. Dependency: none — a policy decision plus a test update.

**P0-2. Remote MCP binds 0.0.0.0 with optional TLS and token-only auth.**
- **Evidence:** `serve_sse` in `src/main.rs` defaults host `0.0.0.0`, port 8443; `TlsConfig::validate` accepts `(None, None)` (`mcp/tls.rs:28-39`); `http::serve` requires only a non-empty api key.
- **Impact:** token transmitted in cleartext on unconfigured deployments → MITM yields full tool access (compounding P0-1's terminal).
- **Root cause:** TLS treated as deployment concern; no refuse-plaintext-on-public-bind guard (the tunnel module has a loopback guard; the HTTP MCP server does not).
- **Affected modules:** `mcp/http.rs`, `mcp/tls.rs`, `main.rs`.
- **Exploit:** run `awh mcp serve --transport sse` on a host; sniff `Authorization` on-path; replay token.
- **Fix:** refuse non-loopback bind without TLS (mirror `tunnel/mod.rs`'s guard); document; add a test. Dependency: none.

**P0-3. No stale-state protection on the primary mutation primitive.**
- **Evidence:** `WorkspaceMcp::write_file` (workspace.rs:104-127) has no expected-hash/context parameter; `ExpectedState` exists only in the unwired `services::edit.rs`.
- **Impact:** an agent that read a file, deliberated, then wrote loses to any concurrent mutation — silently. In the repo's own flagship scenario (multiple agents, one project), this is the routine failure, and with no snapshots (P1-1) it is unrecoverable except by git.
- **Root cause:** edit verification deferred to AWE-002+ which was never executed.
- **Fix:** implement AWE-002/003/004 as specified (the specs are complete and high quality in `docs/issue-resolving-prompts/`). Dependencies: none external; the model types already exist.

---

## 36. P1 Findings

**P1-1. No file snapshot/undo/provenance subsystem** (roadmap Phase 5 = 0% implemented; `FileState`/`sha256_hex` scaffolding unused). Impact: irreversible agent mistakes; audit ring cannot answer "what changed". Fix: implement snapshot store keyed on `FileState` + `workspace.write_file` auto-capture. Depends on: P0-3 (edit transaction identity gives the capture point).

**P1-2. Audit trail is volatile, process-local, and identity-free.** `services/audit.rs` (OnceLock ring, 1,000 entries, no fsync). Restart erases; MCP-HTTP and Control API processes keep separate rings; no agent/session/correlation ids anywhere. Fix: file-backed append-only audit sink with rotation + id propagation from session lifecycle; keep the ring as a hot cache. Depends on: session identity (P1-4).

**P1-3. Control API = single-token, full-host admin plane, including `POST /terminal/run` and git push, over plain HTTP.** `api/control.rs` routes + `serve_control_api` (no TLS machinery). Fix: scopes on tokens (`read`/`write`/`admin`), TLS parity with the MCP plane, and a confirmation-token pattern (or removal) of `/terminal/run` remotely. Depends on: none.

**P1-4. No agent/session identity in any enforcement or audit path.** `AgentStore`/grants inert (main.rs:834-836); `SessionLifecycle` metadata never reaches tools/audit. Impact: multi-agent attribution impossible, capability grants unenforceable, per-agent scoping (Phase 5 of the gate plan) blocked. Fix: thread a `Caller{id, session, grants}` through `dispatch_*` → `authorize_tool` → audit; wire `CapabilityGrantStore` lookups into `authorize_builtin_tool`. Depends on: P1-2 (audit schema) — or land identity first and audit second.

**P1-5. TOCTOU on final path component in both filesystem implementations** (§7). Fix: open-with-contained-resolution semantics (write via `OpenOptions` with the temp file inside the resolved parent + rename, verifying post-resolve; on Linux `openat2(RESOLVE_BENEATH)` where available; a portable approximation: re-run `canonicalize` immediately before rename/persist and abort on mismatch). Depends on: none.

**P1-6. Cross-agent file/git mutations are unserialized and undetected** (§17, §21). Two agents share one working tree with no lock, no conflict detection, no merge coordination — the flagship scenario's central hazard is unaddressed. Fix (minimal): extend `StoreLock` to an opt-in per-path mutation advisory lock + expose "file changed since your read" via expected-hash (P0-3). Fix (real): worktrees per agent (roadmap Phase 3). Depends on: P0-3.

**P1-7. CI never tests Windows/macOS/Android.** `main.yml` runs ubuntu-only; release matrix ships six binaries. Windows job-object code, `CREATE_NEW` lockfile behavior, Android sandbox cfg-exclusion (§27) are all unverified. Fix: add a test job per tier-1 target (or at least run the test suite on windows-latest and macos-latest; add a Termux smoke test to docs at minimum). Depends on: none.

---

## 37. P2 Findings

**P2-1. MCP plane bypasses the services layer for files/memory/tasks/connectors** — contradicts `services/mod.rs` doctrine; `architecture.rs` tests don't catch it because they only check MCP↔Control-API separation. Fix: migrate `WorkspaceMcp`/`MemoryMcp`/`TasksMcp`/`ConnectorsMcp` behind `services/` facades or accept MCP-owns-stores and delete the doctrine; add a conformance test asserting every interface constructs stores via services.

**P2-2. Duplicate memory/task/filesystem abstractions** (§14, §24) with divergent schemas and locking. Fix: unify (MCP variants authoritative), delete `core::files::FileStore` + `core::tasks::TaskStore` or wire them.

**P2-3. Terminal env inheritance inconsistent with custom-server env filtering** (`terminal.run` inherits everything incl. secrets; custom MCP spawns filter). Fix: apply `is_blocked_environment` filtering to `terminal.run` children, or offer an env-allowlist flag.

**P2-4. `terminal.run` service-level audit hook is dead API**; audit happens only at the dispatcher. Control-API terminal calls audit at the API layer, TUI terminal at... TUI doesn't audit terminal (TUI is trusted-operator). Acceptable, but the dead hook misleads. Fix: remove `with_audit_hook` or attach it in the dispatcher.

**P2-5. No dependency vulnerability scanning** (no cargo-audit/cargo-deny in CI). Fix: add scheduled `cargo audit`.

**P2-6. Rate limiter keyed on spoofable `X-Forwarded-For` and a shared "direct" bucket** (both planes). Fix: document the trusted-proxy requirement; key by session id where available.

**P2-7. No MCP cancellation (`$/cancelRequest`/notifications) and no per-call timeout on dynamic provider invocation in stdio mode.** Fix: add a dispatcher-level timeout for dynamic tool calls (circuit breaker already bounds repeated failures).

**P2-8. `isError` never emitted** in tool results; error-shaped JSON values ride as success content. Fix: set `isError: true` when handlers return error envelopes.

**P2-9. Trust-store corruption denies all Medium/High built-ins even for never-opted-in workspaces** (§20 footnote) — surprising availability behavior. Fix: distinguish "unreadable but never opted in" (default-allow) from "unreadable after opt-in" (deny).

**P2-10. ContextEngine active-item sets are process-local with no cross-process coherence** (§13) — offloads shared, windows divergent. Fix: document; or journal item mutations into `.agent/context-engine/` with the same StoreLock pattern.

**P2-11. Android/Termux: `StoreLock` O_EXCL semantics + rename atomicity on shared storage unverified; sandbox cfg excludes Android** (§27). Fix: Termux test harness; `#[cfg(any(target_os="linux", target_os="android"))]` for the sandbox module if intended.

---

## 38. P3 Findings

- **P3-1.** `git.branch` registry metadata says "create or switch" but the tool reads the current branch (§6). Fix metadata.
- **P3-2.** `DeterministicPlanner` is dead code; either wire `context.assemble` to consult it or cut it until its phase.
- **P3-3.** `tokio` `signal` + `tracing-subscriber` `env-filter` features enabled without consumers; `RUST_LOG` does nothing. Wire an EnvFilter or drop the feature.
- **P3-4.** `tasks.assignee` accepts nonexistent agents; validate against `AgentStore` (cheap once P1-4 lands).
- **P3-5.** `mcp.status` dynamic tool count calls `aggregate_tools()` (provider fan-out) on every status — fine, but unbounded provider count could make the "health" call slow; add a cap.
- **P3-6.** `unreachable!()` at main.rs:797 — replace with a graceful bail.
- **P3-7.** Release artifacts lack checksums/signature/SBOM.
- **P3-8.** `workspace.list_files` doesn't skip `.git`/`.agent` while `search` does — inconsistent; agents can enumerate store files (harmless today, useful for hygiene).
- **P3-9.** Error cause flattening at the MCP boundary (§22) — consider a small error-taxonomy so clients can distinguish capacity/auth/io errors.
- **P3-10.** Docs: `services/mod.rs` doctrine comment contradicts reality (see P2-1); fix the comment or the code.

---

## 39. Technical Debt Graph

```
No caller identity (P1-4) ──────────────┐
   └─> capability grants inert ────────┤
   └─> audit w/o agent/session ids (P1-2)│
   └─> no per-agent scoping (Phase-5 gate)│
                                        ▼
                       multi-agent isolation impossible (P1-6)
                                        ▲
EditTransaction model exists ───────────┤
   └─> no executor (P0-3) ──────────────┤
        └─> no verification ────────────┤
             └─> no snapshots (P1-1) ──┤
                  └─> no provenance ────┤
                       └─> no rollback ┘
                                        ▲
StoreLock (done) ────────────────────────┤
   └─> JSON stores serialized ──────────┤
        └─> files/git unserialized ────┘  (the only cross-agent safety that exists covers side-stores, not the artifact)

MCP stores ≠ services stores (P2-1/P2-2) ──> every future feature (edit, snapshot, provenance) must pick a home first, or duplicate again
```

**The real critical path:** *caller identity → unified store layer → edit executor with expected-state → snapshot capture → provenance records → audit persistence → per-agent capability scoping → worktree isolation.* Everything else (remote hardening, connectors, TUI polish) is parallel work that doesn't unblock, or get unblocked by, this chain.

---

## 40. Missing Foundations (What Must Exist Before Higher Layers)

1. **Caller/agent identity primitive** — nothing can be attributed, scoped, or isolated without it.
2. **A mutation transaction** — verification, snapshots, provenance, and rollback all hang off one transaction record; `EditId`/`EditStatus` were built for exactly this and then stopped.
3. **Persistent audit sink** — accountability claims are void while history is a volatile ring.
4. **File-snapshot store** — the "reversible" promise's physical substrate.
5. **Unified store layer** — one memory, one task model, one filesystem service; today every new subsystem must choose between two lineages.
6. **Session→workspace binding stronger than process CWD** — a prerequisite for worktrees and for any multi-project server.
7. **Remote-plane transport security defaults** — refuse plaintext on public binds.

---

## 41. Features That Should Be Postponed

Grounded in the evidence above, despite appearing in docs/roadmap/ambitions:

1. **Per-agent worktrees & branch allocation (Phase 3 remainder)** — blocked by identity (P1-4) and by serialization (P1-6); building worktrees over unattributable callers produces tree-shaped messes.
2. **Advanced multi-agent orchestration** (handoff, shared context, agent events) — same blockers; also blocked on P2-10 coherence.
3. **Cloud control plane / remote-first deployments** — blocked by P0-2, P1-3 (transport security, token scopes). The Control API is an admin plane, not a tenant plane.
4. **Connector ecosystem expansion (beyond Composio/GitHub)** — blocked by P0-1 (dynamic `connector.invoke` is High-risk, default-allowed) and the absent per-agent scoping; each new connector widens the default-open blast radius.
5. **Enterprise dashboards / richer TUI** — the TUI already has 13 screens against a runtime that can't answer "who changed what"; UI depth compounds the illusion. (The TUI itself is good — postpone *expansion*.)
6. **LLM planner integration (`PolicyType` non-deterministic variants)** — the planner is dead code and the deterministic engine is unproven in cross-process use; an LLM planner on an incoherent memory/context base adds nondeterminism to an unauditable system.
7. **Workflow engine / task-dependency graphs** — `tasks.*` has no ownership semantics; dependencies without owners are decoration.

**What should NOT be postponed** (i.e., is correctly sequenced now): the gates themselves, the AWE-001→004 implementation, audit persistence, and snapshot storage — these are foundations, not features.

---

## 42. Recommended Implementation Sequence

| # | Milestone | Why now | Prerequisites | Modules | Risk | Acceptance criteria |
|---|---|---|---|---|---|---|
| M1 | **Flip High-risk default to deny** (P0-1) + **refuse public-bind-without-TLS** (P0-2) | smallest change, largest risk delta; pure policy + guard + tests | none | `execution_gate.rs`, `http.rs`/`tls.rs`, `main.rs` | low | default `terminal.run` denied without trust record; non-loopback bind without TLS refuses with actionable error; both pinned by tests |
| M2 | **Execute AWE-002/003/004**: EditService executor, `filesystem.patch` MCP tool, expected-state verification, no-partial-preparation | specs already written; model already exists; unblocks everything downstream | M1 (so the new mutating tool is gated sanely) | `services/edit.rs` (+new `services/edit_exec.rs`), `mcp/dispatcher.rs`, `services/files.rs` (adopt atomic write) | medium | multi-op patch with stale hash fails whole-transaction, no file mutated; property tests for occurrence semantics; real-FS integration tests |
| M3 | **Snapshot + provenance on the edit path** (P1-1): capture `before/after FileState`, store under `.agent/snapshots/`, `awh snapshot list/restore` CLI + MCP tool | reversibility is the product's core promise; the edit transaction from M2 is the natural capture point | M2 | new `services/snapshots.rs`, `core/` store, CLI, dispatcher | medium | write→snapshot→restore round-trips byte-identical; crash mid-edit leaves recoverable prior state; tests incl. kill -9 mid-transaction |
| M4 | **Persistent audit + caller identity** (P1-2/P1-4): append-only file sink with rotation; thread session/agent ids; wire `CapabilityGrantStore` into the builtin gate | attribution is the blocker for scoping, isolation, and trustworthy audit | M1 (gate surface exists) | `services/audit.rs`, `mcp/dispatcher.rs`, `execution_gate.rs`, `core/agents.rs` | medium | every mutation event carries agent/session ids; grants actually deny; audit survives restart (integration test) |
| M5 | **Unify the store layer** (P2-1/P2-2): one memory store, one task model, MCP+API+TUI on it; add conformance test | every later feature forks otherwise | none (parallelizable with M2-M4) | `mcp/memory.rs`→`services`, `core/*` deletions | medium (migration) | single `.agent/memory.json`; architecture conformance test extended to store construction; no format drift |
| M6 | **Cross-agent mutation safety** (P1-5/P1-6): final-component-safe writes; optional per-path mutation lock; "changed since read" surfaced via M2's expected-state | the flagship scenario is currently unsafe | M2 | `services/files.rs`, `mcp/workspace.rs` | medium | adversarial interleaving harness (two processes) cannot produce an out-of-root write; concurrent edit detected, not silent |
| M7 | **Transport/ops hardening**: TLS-required-on-public for Control API too, token scopes, cancellation, isError, rate-limit keying, cargo-audit in CI, multi-OS test matrix | polish that compounds M1 | M1 | `api/control.rs`, `mcp/http.rs`, `.github/workflows` | low | CI green on 3 OSes; audit job present; scopes enforced by test |
| M8 | **Worktrees & per-agent isolation** (Phase 3): agent→worktree allocation, git worktree service ops, merge-coordination primitives | now unblocked by M4 identity + M6 safety | M4, M6 | `services/git.rs`, new `services/worktrees.rs`, dispatcher | high | two agents in isolated worktrees cannot touch each other's trees; merge conflict detection tested with real git |
| M9+ | Connector expansion, richer remote/TUI, planner activation, anything ecosystem | only after the runtime can attribute, verify, and reverse | M1-M8 | — | — | — |

Sequence rationale: correctness of the mutation core (M2/M3) and attribution (M4) are jointly the critical path; security defaults first because they're cheap (M1); isolation last because it composes over everything else (M8).

---

## 43. Repository Health Score

| Dimension | Score /10 | Justification |
|---|---|---|
| Architecture coherence | **6** | Three generational strata coexist; planes cleanly separated (enforced by tests) but the services doctrine is violated by MCP; entity model disconnected |
| MCP implementation | **7** | Strong protocol conformance, lifecycle, schema validation, transports; missing cancellation/isError, weak rate-limit keying |
| Workspace runtime | **4** | Containment real; but workspace is just CWD; no identity, no history, no recovery |
| Filesystem security | **7** | Dual, mostly rigorous implementations with symlink handling rare in this class; final-component TOCTOU and one non-atomic write path |
| Editing safety | **2** | Model-only; the actual write primitive is unverified overwrite |
| Git isolation | **3** | Safe argv, good edge handling; no isolation semantics, destructive ops unexposed/uncalled, push/pull not on MCP |
| Capability/policy enforcement | **4** | Real, well-tested, fail-closed machinery — but default-allow for builtins and 3-tool DENY-only policy |
| Snapshots/rollback | **1** | Absent for files; present for context items |
| Provenance/audit | **2** | Volatile ring, no identity, good redaction |
| Context | **6** | Substantial, honest engine; planner dead; cross-process coherence missing |
| Memory | **4** | Two divergent quality stores; the MCP one is good |
| Skills | **6** | Solid store/install/integrity; capability-package semantics absent |
| Agent/session model | **2** | Records exist, nothing consults them |
| Multi-agent readiness | **2** | StoreLock is genuinely good; everything else absent |
| TUI/CLI | **7** | 13 screens, remote backend real, backend abstraction exemplary; block_on serialization in UI |
| Remote architecture | **3** | Auth present but single-scope; TLS optional-on-public; admin plane conflated |
| Testing | **7** | 634 tests, real FS/git integration, fail-closed contracts, proptests; single-OS, no race harness, no cross-plane tests |
| CI/release | **5** | Strong verify gate; 6-target release; no audit/scanning/signing/multi-OS tests |
| Documentation accuracy | **7** | Unusually honest (gap matrices, fail-closed admissions); metadata drift (git.branch) and doctrine comment wrong |
| Android/Termux readiness | **2** | Artifact built; zero validation; sandbox cfg excludes Android; lock semantics on Android FS unverified |
| **Production readiness (as "safe agent workspace runtime")** | **3** | Transports and protocol are production-adjacent; the core safety narrative (verify/reverse/attribute) is unimplemented |

---

## 44. Final Verdict

**What AWH actually is today:** a disciplined single-project MCP tool server (65 tools + dynamic providers) with genuinely strong transport/protocol security engineering, fail-closed external-server trust gating, careful cross-process store locking, and a real context engine — surrounded by correctly-shaped but unexecuted scaffolding for its product-defining features.

**What it is not yet:** a runtime that can safely execute, verify, attribute, or reverse agent mutations. No edit verification, no snapshots, no provenance, no agent identity enforcement, no isolation.

**Strongest subsystem:** the MCP transport/protocol + authorization gate stack (`dispatcher.rs`, `execution_gate.rs`, `schema.rs`, transports) — tested, honest, and fail-closed by construction.

**Weakest subsystem:** the agent/session/capability layer — a persisted records island consulted by nothing.

**Most dangerous security issue:** default-allow High-risk built-in tools (arbitrary command execution) — compounded by the remote MCP plane's optional-TLS-on-public-bind default (P0-1 + P0-2).

**Most dangerous reliability issue:** unconditional whole-file overwrite with no stale-state detection in the exact multi-agent, same-worktree scenario the product advertises (P0-3 + P1-6).

**Largest architectural gap:** three storage generations with MCP bypassing the service layer, so there is no single canonical core for the "MCP-first" claim to point at.

**Most important missing primitive:** the executed edit transaction with expected-state verification — it is the hinge on which snapshots, provenance, rollback, and audit all swing (and its type vocabulary already exists, unused).

**Most important next milestone:** M1 (security-default flip) then M2 (AWE-002/003/004 execution) — cheap risk elimination followed by the critical-path feature.

**What should be postponed:** worktrees/isolation, multi-agent orchestration, cloud/remote expansion, connector ecosystem growth, and any LLM-planner integration — each is blocked on identity, verification, or transport-security foundations.

**Overall maturity:** early-stage runtime (≈ v0.1名副其实): infrastructure grades 6-7/10, product-defining capabilities grade 1-3/10.

---

### One-Sentence Verdict

**AWH is a well-engineered MCP tool server whose transport, protocol, and store layers already meet a high bar for a local-first agent surface, but whose defining promises — verified editing, reversible snapshots, attributable provenance, and per-agent isolation — remain unwired scaffolding, and whose default configuration currently grants any connected agent unrestricted command execution on the host.**

---

*Report generated from static forensic analysis of branch `rust` @ `0e38041`. Environment lacked `cargo`; no build/test execution was performed, and no repository state was modified. All FACT-labeled claims cite files/symbols verified by direct reading; INFERENCE and RECOMMENDATION labels are used where stated.*