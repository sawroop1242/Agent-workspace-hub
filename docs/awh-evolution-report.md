# AWH Evolution Report

Status: **complete** · Prepared per `docs/MASTER_PROMPT.md` §43 (First
Engineering Task) · Evidence gathered at `rust` @ `29a7776` (post-Phase-11).

This report inspects the complete `rust` branch, classifies every
subsystem of the target architecture as **existing / partial / missing /
obsolete**, and derives the module plan, milestones, dependency graph,
testing strategy, and risk register for the evolution ahead. No
implementation begins until this report is accepted.

## 1. Current architecture

AWH is a single Rust binary (`awh`) that gives external AI coding agents
(OpenCode, Codex, any MCP client) a secure, persistent workspace through
the Model Context Protocol. It is **not** itself an agent runtime — there
is no LLM loop, no model provider, no agent identity in the request path.
The unit of dispatch is the **project**; the unit of concurrency is the
**OS process** (one `awh mcp serve` per agent, coordinated only through
cross-process file locks on shared `.agent/` state).

```
External agent (own model, own loop)
    │  stdio (JSON-RPC lines)      HTTPS/SSE (bearer + TLS)
    ▼                                   ▼
StdioMcpServer ──────────────┬── HTTP server (axum)
                             ▼
                      McpDispatcher (per project)
                             │  audit "tool_invoke"
                             ▼
   ┌────────────────────────────────────────────────┐
   │ Built-in tools (52)   External providers      │
   │ skills.* workspace.* memory.* tasks.*         │
   │ connectors.* connector.* context.*            │
   │ git.* terminal.* (+ github.*, 12, gated)      │
   │        → Composio / custom MCP servers        │
   │          (trust → permissions → execution gate │
   │           → sandbox → circuit breaker)         │
   └────────────────────────────────────────────────┘
        │                                   │
        ▼                                   ▼
  <project>/.agent/ …              ~/.agent-workspace-hub/
  (memory, tasks, context,          (skills, registries, trust,
   skills, mcp refs)                 global MCPs, accounts)
```

Plane separation (enforced by `tests/architecture.rs`): the **Control API**
(`/api/v1`, axum, `src/api/control.rs` — for TUI/CLI/humans) and the
**MCP plane** (JSON-RPC, `src/mcp/` — for agents) never call each other;
both wrap the same service + store layer.

### 1.1 What exists today — verified inventory

| Plane | Component | State | Evidence |
| --- | --- | --- | --- |
| MCP | stdio transport | complete | `src/mcp/server.rs`; 11 integration tests |
| MCP | HTTP/SSE transport | complete | `src/mcp/http.rs` (auth, sessions, TLS, limits); 12 tests |
| MCP | Tool catalog | complete | 53 core + 12 `github.*` (65 when `GITHUB_TOKEN` set) — `dispatcher.rs:401-476`, `:1255` |
| MCP | MCP server registry (global + project) | complete | `global_mcp.rs`, `custom_mcp.rs`; `StoreLock`-guarded |
| MCP | Trust / approval of external servers | complete | `trust.rs`, `trust_store.rs`, `execution_gate.rs`; 14 tests |
| MCP | Permissions (env/fs/net/secrets) for external servers | complete | `permissions.rs`; fail-closed |
| MCP | Sandboxing of external server processes | complete (Linux) / implemented-not-runtime-verified (macOS/Windows) | `sandbox.rs` (bwrap / sandbox-exec / Job Objects) |
| MCP | Circuit breaker + outbound limits | complete | `circuit_breaker.rs`, `config.rs` |
| MCP | Outbound MCP client (stdio + streamable HTTP) | complete | `custom_mcp.rs` (schema-validates before forwarding) |
| MCP | Resources/prompts | partial | `resources/*` read-only views; `prompts/list` hardcoded empty |
| Connectors | Composio multi-account | complete | `composio*.rs` (registry, auth, provider) |
| Connectors | GitHub native | complete | `github.rs` (Phase 11) |
| Context | Context engine (plan/select/budget/offload/snapshot) | complete | `src/context/` (12 modules; state in `.agent/context-engine/`) |
| Context | Token-aware assembly | complete | `scoring.rs:14-37` (relevance/priority/recency/alignment/cost), `selector.rs` |
| Context | Offload/restore, protect (pinning) | complete | `offload.rs`, `snapshot.rs`, `engine.rs:286-317` |
| Core | Workspace/project/task/memory/context stores | complete | `src/core/`, `src/mcp/{memory,tasks,workspace}.rs` |
| Core | Files service (traversal/symlink-safe, 8 MiB cap) | complete | `src/services/files.rs:37-83` |
| Core | Git service (argv-only, 30s timeout, high-risk ops) | complete | `src/services/git.rs` |
| Core | Terminal service (argv-only, 256 KiB capture) | complete | `src/services/terminal.rs` |
| Skills | Discovery/parse/install/lockfile/trust/registries | complete | `src/skills/` (15 files) |
| Control API | 28 routes under `/api/v1` | complete | `src/api/control.rs:142-191` |
| CLI | status/tui/serve/mcp/skill/registry/tunnel | complete | `src/main.rs` (human output only — no JSON mode) |
| TUI | 13 screens over `WorkspaceBackend` (local + remote) | complete | `src/tui/screens/` |
| Tunnel | ngrok transport for Control API | complete | `src/tunnel/mod.rs` |
| Ops | Audit ring (1000, redaction choke point) | complete (volatile) | `src/services/audit.rs:104-125` |
| Ops | Rate limiting (sliding window, bounded memory) | complete | `src/api/rate_limit.rs` |
| Ops | CI (fmt/clippy/test×3OS/audit) | complete | `.github/workflows/rust.yml`; 421 tests green at HEAD |
| Docs | 15 documents + interop evidence | complete | `docs/`, `examples/mcp-interop/` |

### 1.2 Data model today

`Task {id, title, status}` (4 statuses), `MemoryEntry {timestamp, content}`,
`Project {name, path}`, `Skill {name, description, version?, path}`,
`Connector` (metadata only), `McpPermissions` (per external server).
No agent, session-caller, capability, policy, approval-request, event,
checkpoint, snapshot, artifact, or model-routing types exist.

## 2. Feature inventory vs. target (MASTER_PROMPT §1–§42)

Classification legend: **E**xisting · **P**artial · **M**issing · **O**bsolete.

| # | Target subsystem (spec §) | Class | Today's reality |
| --- | --- | --- | --- |
| 1 | Per-agent MCP runtime, agent registry/lifecycle (§1) | **M** | No `Agent` type, no lifecycle states, no `awh agent` CLI. Dispatcher is per-project; “agents” are OS processes distinguished only by `StoreLock` coordination (`store_lock.rs:1-16`) |
| 2 | Capability-based security, allow/deny + grants (§2) | **P** | Capability *schema* exists for external MCP servers (`permissions.rs`); nothing applies capabilities to agents, built-in tools, or API routes. No grant/expiry/path-restriction model |
| 3 | Central tool broker with risk classes (§3) | **M** | No broker. The dispatcher routes directly to handlers. Risk exists only as prose labels (`HighRiskGitOp` in `git.rs:20-28` has no enforcement plumbing). Built-in tools bypass trust/permissions entirely |
| 4 | Durable agent message bus (§4) | **M** | Nothing. Only per-SSE-session broadcast channels (`sse.rs:40-47`) |
| 5 | Multi-agent orchestration (§5) | **M** | Nothing (no spawn, delegation, handoff, teams) |
| 6 | Task DAG + scheduler (§6) | **M** | Tasks are flat `{id,title,status}` records; no dependencies, assignees, deadlines, priorities, or scheduler |
| 7 | Event-driven core, durable events (§7) | **M** | Audit ring only: in-memory, 1000 entries, process-lifetime, no pub/sub, no event types |
| 8 | Checkpoints + recovery (§8) | **M** | Nothing. (Context-engine snapshots cover engine state only) |
| 9 | Layered memory scopes (§9) | **P** | `MemoryScope` labels exist on entries (`memory.rs:36-43`) but are unenforced labels — no access policy, no private/project/workspace/global enforcement |
| 10 | Context engine (§10) | **E** | `src/context/` fully implements budget-aware assembly; the strongest existing match to the target |
| 11 | Skill System 2.0 — capabilities/permissions/tests per skill (§11) | **P** | Skills are `{name,description,version?,path}` manifests + trust levels; no required-capabilities declaration, no permission model, no tests/examples |
| 12 | MCP ecosystem manager (§12) | **P** | Registry/trust/sandbox/lifecycle/circuit-breaker exist per **project**; no per-agent manifests, no health-check scheduler, no machine-readable per-agent capability manifest |
| 13 | Approval engine, HITL queue (§13–14) | **P** | Trust/approval exists for *enabling external MCP servers* (one-shot allow/block/unknown), not for arbitrary pending actions. No request queue, scopes (once/task/agent), timeouts, or escalation. No autonomous/supervised/read-only/safe-mode/emergency-stop modes |
| 14 | Resource governor (§15) | **P** | Per-request caps exist (timeouts, body, sandbox rlimits, circuit breaker, rate limit); no CPU/mem/disk/process-count/child-agent budgets per agent or runtime |
| 15 | Cancellation/retry/failure classes (§16) | **P** | Timeouts + circuit breaker + fail-closed everywhere; no cross-component cancellation propagation, no retry/backoff policy, no failure taxonomy |
| 16 | Model routing (§17) | **M** | No model layer at all (by current design AWH hosts tools, agents bring models) |
| 17 | Workspace snapshots (§18) | **M** | No `awh snapshot`; context-engine snapshots only |
| 18 | Git worktrees for agent isolation (§19) | **M** | Git service has no worktree support; branch/diff/commit only |
| 19 | Artifacts + provenance (§20) | **M** | Nothing. Audit ring is the only lineage record |
| 20 | Observability: metrics/trace/cost (§21) | **P** | Structured `tracing` to stderr, audit ring; no metrics, no request/trace IDs, no cost/token accounting, no persistence |
| 21 | Connector system (§22) | **P** | Composio (multi-account) + GitHub native; no drive/Notion/Slack/DB/browser connectors; no per-connector resource scoping/risk policy |
| 22 | Security architecture (§23) | **P** | Strong for the current scope (11 documented threats, all mitigated + tested); the §23 pipeline (identity→capability→scope→…) exists only at the *external-server* boundary |
| 23 | Secret management, `secret://` refs (§24) | **P** | `${secret:NAME}` env-resolution with allowlist + audit (`custom_mcp.rs:95-117`); no secret store, no `secret://` URIs, no per-agent secret scoping |
| 24 | Prompt-injection defense, trust hierarchy (§25) | **M** | Nothing. (`context.protect` pins items against offload — unrelated, often confused) |
| 25 | Policy engine, most-restrictive-wins (§26) | **M** | `context/policy.rs` governs context decisions only; no declarative action policy anywhere |
| 26 | CLI-first, 20 command groups, JSON output (§27) | **P** | 7 command groups exist; `--json`/`--output` flag does not exist anywhere |
| 27 | Unified error model (§28) | **P** | Two decent error envelopes (JSON-RPC codes; `{"error":{code,message}}`) but no category/retryable/request-id/agent-id/task-id fields |
| 28 | `awh doctor` (§29) | **M** | Nothing. `awh status` prints a hardcoded banner string (`main.rs:279`) |
| 29 | Typed layered configuration (§30) | **P** | Env + CLI flags with documented precedence (`mcp/config.rs`); no system/user/workspace/project/agent layers |
| 30 | Concurrency model (§31) | **P** | tokio async, bounded channels in SSE, `kill_on_drop`; no structured concurrency across agents, no per-agent runtime |
| 31 | Testing (§32) | **E** | 421 tests (unit + integration + security + interop harnesses + property tests for security paths); architecture-conformance tests |
| 32 | TUI (§33) | **P** | 13 screens exist **before** the core is multi-agent (spec sequencing violated harmlessly — TUI rides the same `WorkspaceBackend` the Control API exposes; will need agent/workflow/approval screens later) |
| 33 | `awh run` autonomous command (§34) | **M** | Nothing (requires model layer) |
| 34 | Built-in agent teams (§35) | **M** | Nothing |
| 35 | Controlled self-improvement (§36) | **M** | Nothing |
| 36 | Extensibility trait seams (§37) | **P** | Good seams already: `WorkspaceBackend`, `ConnectorProvider`, `TunnelProvider`, `McpClient`, `ContextPolicy`/`ContextPlanner`; missing: `AgentRuntime`, `ToolBroker`, `PolicyEngine`, `Scheduler`, `EventBus`, etc. |
| 37 | Compatibility (§38) | **E** | 64 MCP tools + Control API + TUI backend trait are the stability surface; Phase 11 kept all prior tools |
| 38 | Documentation (§39) | **P** | 15 docs complete for today's scope; no agent/orchestration/policy/checkpoint docs yet (nothing to document) |
| 39 | Legacy Python on `main` | **O** | Reference-only, unbuilt, untested, unreferenced by any workflow. Dead `pyproject.toml` at root. Candidate for archival after this report is accepted |

**Summary: 9 E · 17 P · 12 M · 1 O.** The existing system is a
production-hardened *single-agent, per-project workspace runtime* — roughly
Milestones 4 (context) and half of 12 (MCP manager) of the target, with a
security core that is genuinely strong for its current scope.

## 3. Gap analysis — the five structural gaps

Everything above reduces to five structural gaps. Each one is a
*precondition* for the multi-agent vision; none can be skipped.

1. **No agent identity.** Nothing in the request path knows *who* is
   calling. The dispatcher is constructed per project; the Control API
   authenticates a single shared token; stdio has no authentication at all.
   Every §42 invariant ("no agent → direct uncontrolled tool access",
   "no child → parent privilege escalation") is unenforceable without
   caller identity. **This is gap zero.**
2. **No policy/capability plane for built-in tools.** The trust/permissions/
   execution-gate chain — the best code in the repo — applies *only to
   external MCP server processes*. All 52 built-in tools (including
   `terminal.run`, `git.commit`, `workspace.delete_file`, all 12
   `github.*` mutation tools) are gated by nothing beyond transport auth.
   The tool broker must extend the existing gate, not replace it.
3. **No coordination plane.** No events, no messaging, no scheduler, no
   approvals queue, no checkpoints. Multiple agents today = multiple
   processes racing on `StoreLock` — safe (no corruption) but blind
   (no visibility, no coordination, no cancellation).
4. **No model layer.** The spec's §34 (`awh run`), §16 (fallback),
   §35 (teams) all presuppose AWH driving models. Today AWH is model-less
   by design. Introducing the model router is a deliberate architecture
   change, not an incremental refactor.
5. **Volatile observability.** The audit ring dies with the process.
   Durable events + a real `awh status`/`awh doctor` are prerequisites for
   trusting anything the orchestration layer reports.

## 4. Security risks (current system)

The current threat model (`threat-model.md`) covers 11 threats with
implemented mitigations and passing tests. The following are the *new*
risks the evolution introduces, plus current-scope items the report must
flag honestly:

| # | Risk | Severity | Notes |
| --- | --- | --- | --- |
| S1 | Multi-agent same-project races are safe but uncoordinated | Medium | `StoreLock` prevents corruption, not conflict; two agents can still undo each other's work with no visibility |
| S2 | Single shared Control API token = full access (incl. `terminal.run`) | Medium | No scopes/roles; documented for hardening (§23 pipeline needs per-agent identity to fix this) |
| S3 | Audit ring volatile (1000 entries, process-lifetime) | Medium | Restart loses the trail; also blocks the event system |
| S4 | stdio transport has no authentication | Low | Local-process trust model — any local process can spawn `awh mcp serve`; acceptable today, changes with remote agent runtimes |
| S5 | macOS/Windows sandbox runtime not verified | Low | CI compiles and runs generic tests; dedicated confinement probes pending (documented in completeness-audit) |
| S6 | Direct-connection rate-limit key spoofable (`X-Forwarded-For`) | Low | Documented as best-effort bounding, not a security boundary |
| S7 | `awh status` hardcoded — health reporting absent | Low | Feeds operator blindness; fix with `doctor` in M1 |
| S8 | Evolution itself: capability/policy engine bugs could fail *open* | High (forward) | The #1 engineering risk of the whole program. Mitigation: keep fail-closed defaults, architecture tests, and mandatory security tests per capability-sensitive component (§41, §32) |

## 5. Performance risks

| # | Risk | Notes |
| --- | --- | --- |
| P1 | Per-agent MCP instance fan-out | N agents × dispatcher + providers → memory/runtime overhead; must share immutable stores (`Arc`), keep per-agent state small |
| P2 | Durable event bus write amplification | Every tool call → audit + event → disk; needs append-only batching (JSONL like memory), fsync policy, bounded queues |
| P3 | DAG scheduler under load | Priority + deadline scheduling with bounded parallelism; must not spin-poll (async, channels) |
| P4 | Context engine memory | Active items are in-process `RwLock<Vec>` — engine restart loses active items (only offloads/snapshots persist); per-agent engines multiply this |
| P5 | Sandbox spawn cost | bwrap per tool call would be heavy; confine to *external server processes* (as today) + high-risk ops only |
| P6 | Interop harness drift | Node harnesses pin SDK; keep as CI-optional evidence, not gates |

## 6. Target architecture (delta view)

The master prompt's full target (§ core diagram) is adopted as written.
The delta from today:

```
+-----------------+   +------------------+   +-----------------+
| Agent Registry  |   | Task/Workflow    |   | Policy/Approval |
| (lifecycle, ID) |   | Engine (DAG)    |   | Engine          |
+-----------------+   +------------------+   +-----------------+
         \                   |                    /
          ▸▸ Milestone 1 inserts these above the EXISTING core ◂◂
                    +-------------------+
                    | Tool Broker       |  extends execution_gate
                    | (risk classes,     |  to built-in tools
                    |  capabilities)     |
                    +-------------------+
                               |
        McpDispatcher (per agent, capability-filtered tools/list)
                               |
      (everything below exists today and is kept: stores, services,
       context engine, skills, connectors, sandbox, audit)
```

Key decisions proposed:

1. **Agent identity wraps the dispatcher, not replaces it.** An
   `AgentRuntime` owns a `McpDispatcher` configured with the agent's
   capability manifest — `tools/list` is filtered per agent, `tools/call`
   re-enters the broker. One dispatcher per agent satisfies "one isolated
   MCP server per agent" (§1) without rewriting the tool layer.
2. **The broker generalizes `execution_gate`.** Same authorize-entry-point
   pattern, extended from "is this external server trusted" to "does this
   agent hold this capability for this tool at this risk level in this
   scope" — preserving the fail-closed + audited architecture tests.
3. **Events subsume the audit ring.** `AuditLog` becomes one *subscriber*
   to a durable `EventBus`; the ring stays as a fast in-process view.
   Append-only JSONL per day + bounded queue; no new deps (spec: avoid
   unjustified crates).
4. **StoreLock stays the cross-process floor.** The message bus and
   scheduler live above it; in-process agents use channels, cross-process
   agents keep file locks.
5. **Skills grow capability declarations.** Front-matter gains optional
   `requires:` capabilities; undeclared = no capabilities (fail closed),
   and skills can never silently grant (§11).
6. **Trust hierarchy (§25) is implemented at context assembly**: mark
   item provenance (system/user/agent/skill/tool-output), refuse policy
   content from lower-trust sources — extends the existing `ContextSource`
   enum rather than adding a new subsystem.

## 7. Module plan

| Module (new) | Contents | Sits above |
| --- | --- | --- |
| `src/agent/mod.rs` | `AgentId`, `AgentState` (12 states per §1), `AgentRecord` | registry |
| `src/agent/registry.rs` | Durable registry at `.agent/agents/` (StoreLock, atomic writes) | core stores |
| `src/agent/runtime.rs` | `AgentRuntime` trait + `LocalAgentRuntime` (owns dispatcher, cancellation token, resource budget) | dispatcher |
| `src/agent/capabilities.rs` | `Capability` enum, `CapabilitySet`, grant/expiry/path-scope model | — |
| `src/broker/mod.rs` | `ToolBroker` (risk classes, capability check, argument/schema validation, rate, approval trigger), result sanitization hook | agent + mcp |
| `src/policy/mod.rs` | Declarative policy files (global/workspace/project/agent), most-restrictive-wins combinator | capabilities |
| `src/approval/mod.rs` | Approval requests (agent/tool/args/risk/reason/impact), scopes, queue, timeout, escalation | broker + policy |
| `src/events/mod.rs` | `Event` (typed, §7 categories), `EventBus` (durable JSONL + in-process broadcast) | audit ring |
| `src/tasks/dag.rs` | `DagTask` (dependencies, priority, deadline, retry, required caps, artifacts), scheduler | events + broker |
| `src/orchestrator/mod.rs` | Workflow engine (sequential/parallel/hierarchical), delegation/handoff over events | tasks + agents |
| `src/messaging/mod.rs` | send/broadcast/request-response/delegation, dead-letter | events |
| `src/checkpoint/mod.rs` | Agent/task/workflow checkpoints + resume rules | events |
| `src/snapshots/mod.rs` | Workspace snapshots (git state + stores + config) | checkpoint |
| `src/artifacts/mod.rs` | Artifact store + provenance (Agent→Task→Tool→File) | events |
| `src/models/mod.rs` | Extended: `Agent`, `Approval`, `Event`, `DagTask`, `Artifact`, `SecretRef` | — |
| `src/secret/mod.rs` | `secret://` reference resolution layer over env (store-agnostic) | capabilities |
| `src/main.rs` | `awh agent …`, `awh approvals …`, `awh policy …`, `awh event …`, `awh doctor`, `--json` global flag | — |

All new stores follow the existing patterns: `.agent/` location, atomic
temp+rename writes, `StoreLock` for cross-process mutation, fail-closed
parse. No new external dependencies are required for M1–M3 (JSONL + tokio
channels + existing crates cover it).

## 8. Milestones (adapted from §40, sequenced by dependency)

- **M1 — Identity & broker (the keystone).** Agent registry + lifecycle +
  runtime + capability model + tool broker (risk classes) + policy engine
  v1 + `awh agent` CLI + `awh doctor` + `--json` + durable events (audit
  ring becomes a subscriber). Invariant tests land with the code, not
  after. *Exit: two agents in one project see different tool lists;
  a capability-less agent gets zero tools; `terminal.run` requires
  `shell.execute`.*
- **M2 — Orchestration.** Messaging, task DAG + scheduler, orchestrator,
  cancellation propagation, resource governor (per-agent budgets), child
  spawn with least privilege. *Exit: parent agent cannot grant child more
  than itself; a workflow of 3 agents completes with per-agent audits.*
- **M3 — Reliability.** Approval engine (queue, scopes, HITL modes,
  emergency stop), checkpoints/recovery, retries/backoff, failure
  taxonomy, durable event replay. *Exit: killing the hub mid-workflow and
  restarting resumes without duplicating destructive ops.*
- **M4 — Model layer & teams.** Model router trait + provider adapters,
  role policies, `awh run`, built-in teams (§35). *Exit: `awh run`
  plans, spawns least-privilege agents, checkpoints, and reports.*
- **M5 — Developer experience.** Worktree-isolated parallel agents (§19),
  snapshots, artifacts + provenance, self-improvement proposals (§36,
  control-plane changes still require human approval).
- **M6 — TUI extension.** Agent dashboard, workflow DAG view, approvals
  inbox, event stream, per-agent inspection (extend the existing
  `WorkspaceBackend` — screens are cheap once M1–M3 exist).

M1 is intentionally large because identity + broker + policy are one
inseparable design; splitting them produces an agent registry nothing
enforces.

## 9. Dependency graph

```
capabilities ── policy ── broker ── agent runtime ── registry
                                   │
        ┌───────────── agent runtime enables ─────────────┐
        ▼                       ▼                          ▼
   messaging ────────── task DAG/scheduler ──────── orchestrator
        │                       │                          │
        └────── events ────────┴───── checkpoint ──── snapshots
                   │                 │
                artifacts      approval engine ── HITL modes
                                    │
                              model router ── awh run ── teams
```

Hard ordering: capabilities → policy → broker → runtime → (messaging,
scheduler, approvals) → model layer → TUI extension. The context engine,
skills, stores, connectors, and sandbox are *already* at their target
shape and only gain integration points (provenance, skill `requires:`).

## 10. Testing strategy (§32 mapped to the plan)

Keep the existing bars — every phase PR: `cargo fmt`, `cargo clippy -D
warnings`, `cargo test --all-targets` on the 3-OS matrix, `cargo audit` —
and add per-milestone:

| Layer | Required tests |
| --- | --- |
| Architecture conformance | Plane-isolation tests extended: `src/agent/**` must not bypass `src/broker`; broker must not be optional in the dispatch path |
| Capability/policy | Property tests: capability lattice (grant ≤ policy ⊆ deny), most-restrictive-wins composition, expiry, path scopes; fail-closed on corrupt policy files |
| Broker/invariants | The four §42 invariants as executable tests: capability-less agent → zero tools; per-agent `tools/list` filtering; child grants never exceed parent; external data cannot mint capabilities |
| Events/recovery | Crash-injection: kill between event append and state write → replay is idempotent; destructive-op double-delivery is impossible |
| Scheduler/DAG | Cycle detection, priority under contention, deadline, cancellation mid-flight, retry taxonomy |
| Interop | Existing SDK harnesses extended: N agents on one server see filtered tool lists; agent-scoped memory isolation |
| Security | Mandatory per component (S8): every new gate defaults deny; permission-escalation attempt matrices |

## 11. Risk register

| ID | Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- | --- |
| R1 | Capability/policy engine ships with a fail-open bug | Medium | Critical | Fail-closed defaults; invariant tests (§10); every denial audited; security review per milestone |
| R2 | Scope creep: rebuilding what exists (stores, context, sandbox) | High | High | This report freezes the inventory: E-classified subsystems are kept, not rewritten |
| R3 | stdio compatibility break for existing agent clients | Medium | High | Dispatcher API unchanged; per-agent filtering only *narrows* `tools/list` for agents without capabilities; default agent profile preserves today's 64-tool surface |
| R4 | Event store grows unbounded / slows the hot path | Medium | Medium | Append-only JSONL, rotation, async flush, bounded queues (P2) |
| R5 | Multi-process/in-process agent duality doubles state bugs | Medium | High | `StoreLock` remains the floor; in-process agents use channels; one serialization format for both |
| R6 | Model-layer costs/runaway behavior (§16) | Low (M4+) | High | Resource governor + budgets before M4; emergency stop lands in M3, not M4 |
| R7 | Windows/macOS drift (PR #9 lesson) | Medium | Medium | Keep 3-OS CI; never assume Unix semantics in new stores/locks |
| R8 | Tool-catalog growth re-trips the json! macro limit | Medium | Low | Keep per-group schema functions (Phase 11 pattern); add a regression test counting array invocations |
| R9 | Documentation goes stale as surfaces multiply (44→64 lesson) | High | Medium | Counts+tables update in the same commit as code (process rule, already followed) |

## 12. Migration strategy

1. **Zero breaking changes to the 64-tool surface and Control API** (§38).
   New behavior arrives behind new commands and capability profiles.
2. **Default agent profile = today's behavior.** When no agent registry
   exists, AWH behaves exactly as now (single implicit agent with full
   tools). This keeps every existing client (OpenCode, Codex, Inspector
   harnesses) green while the registry is introduced.
3. **Audit ring → event subscriber** is additive: the ring API stays; the
   event bus adds durability underneath.
4. **Skills' new `requires:` front-matter is optional**; skills without
   it are unchanged (no capabilities requested).
5. **`main` (legacy Python) is archived** as a branch/tag, not deleted;
   the dead `pyproject.toml` is removed from the working tree after this
   report is accepted.
6. Each milestone ships behind `awh agent` commands that fail closed when
   the registry/policy is absent, so partial rollouts never silently
   weaken security.

## 13. What this report recommends first

M1 as scoped in §8 — agent identity, capabilities, policy, broker, doctor,
JSON output, durable events — is the keystone and the only milestone where
all four §42 invariants become enforceable. Everything else in the master
prompt either exists today (context engine, skills, sandbox, connectors,
stores, CI) or is downstream of M1.

---

*Prepared as the §43 deliverable. Inspection evidence: 421 tests green at
`rust@29a7776`; 34 MCP-plane modules, 12 context-engine modules, 15 skills
modules, 13 TUI screens audited; all findings carry file:line references
above. The next engineering action is M1 implementation planning against
§7–§10.*
