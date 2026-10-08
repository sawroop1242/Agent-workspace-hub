# AWE-018 / AWE-019 — Agent-Grade Editing Contract, Implementation Status & Roadmap

> **Artifact provenance.** This file is the completed Prompt 17 deliverable and now
> supersedes the prompt text. The full requirements (47 sections, execution sequence,
> acceptance checklist) are preserved verbatim in git history at commit `4769090`
> (`docs: expand implementation prompt 17 contract status roadmap`) and can be
> recovered with `git show 4769090:docs/implementation-prompts/17-contract-status-and-roadmap.md`.
> Nothing outside this file was modified for this prompt (see section 19).

---

## 1. Purpose and scope

This document is the single authoritative statement, reconstructed **from current
source, tests, and CI on the `rust` branch**, of:

- the **canonical agent-grade editing contract** (AWE-018): vocabulary, semantics,
  lifecycle, identity/authorization boundary, expected-state/conflict behavior,
  filesystem/path/TOCTOU boundary, snapshots/provenance, rollback/recovery, audit,
  and externally observable behavior across MCP / CLI / Control API / TUI;
- the **current implementation status** (AWE-019): per-stage, per-subsystem
  classification backed by evidence locations — never an aggregate score;
- **known security and architecture gaps** that are current on `rust`, each with
  evidence, impact, and a next step;
- **historical-status reconciliation** — which prior claims are now closed, which
  were wrong, and which roadmap intent remains unbuilt;
- a **dependency-ordered roadmap** for the remaining work.

**Method.** Every current-state claim below was verified against the `rust` branch at
merge base `0d3a029` (2026-09-28) by reading production source and test files directly.
Historical claims are attributed to dated documents. Roadmap intent is labeled as such
and never presented as runtime reality.

## 2. Product boundary

AWH is an **agent-agnostic, local-first workspace runtime for coding agents**
(`docs/FEATURES.md` product boundary, `docs/roadmap/PROJECT_ROADMAP.md` section 1). It
owns workspace services, the MCP tool plane, the Control API, TUI/CLI clients, policy
and audit infrastructure. It is **not** an agent reasoning engine, model router, swarm
scheduler, or general workflow engine. Editing is a core workspace service AWH provides
**to** agents; agent intelligence is out of scope.

## 3. Evidence hierarchy

Claims below follow the required precedence:

1. current executable/source behavior on `rust` (`0d3a029`);
2. current automated tests that exercise that behavior;
3. current CI/workflow configuration and recorded verification;
4. current interface documentation matching executable behavior;
5. current roadmap/feature documents as target intent;
6. historical status reports;
7. issue/PR descriptions and external-agent claims;
8. comments or aspirational examples without executable evidence.

A roadmap item is not completion evidence. A command in help text is not completion
evidence. A data type is not evidence its full runtime path exists. A merged PR is not,
by itself, evidence its claimed behavior remains on current `rust` — behavior was
re-verified in this branch's tree, not assumed from PR descriptions.

## 4. Current branch/verification context

- **Implementation authority:** `origin/rust` at `0d3a029` ("Update rust.yml"). This
  branch (`awe-018-019-contract-status`) is merged to that commit and makes **no
  production source changes**.
- **Editing-lineage merges on current rust (behavior re-verified in tree, not assumed
  from PR text):** #111 (AWE-009/AWE-016 MCP editing + client validation), #115
  (AWE-014 `awh fs` CLI), #116 (GIT-001 worktrees), #117 (FS-001 coordination), #122
  (ARCH-001 service/store convergence); docs-prompt merges #113–#125.
- **Open PR:** #126 "AWE-015/AWE-017/TW-008: agent-grade editing acceptance suite"
  (branch `awe-015-acceptance`, base `81befe7`) — adds 12 real-interface acceptance
  tests (3 edit-gate + 9 CLI/MCP binary-suite tests) and one production fix: the `Fs`
  CLI arm in `src/main.rs` never called `audit::init_global`, so `awh fs` audit events
  died in the process-local buffer; the fix initializes the durable audit store for
  `fs` runs.
- **Recorded verification (this session, cargo 1.98.1):**
  - `rust` @ `0d3a029`: `cargo test --workspace` → **1227 passed, 0 failed** across 23
    result sections (lib + bins + 20 integration binaries + doc-tests);
    `cargo fmt --check` clean.
  - PR #126 branch head: `cargo test --workspace` → **1228 passed, 0 failed**
    (1216 base + 12 new acceptance tests); fmt and
    `clippy --all-targets -- -D warnings` clean.
  - `clippy -D warnings` at `0d3a029` itself was not re-run this session (docs-only
    change; CI runs it on the PR) — recorded as unverified, not assumed.
- **CI configuration (inspected in `.github/workflows/`):**
  - `rust.yml` — fmt; build/test matrix ubuntu/macos/windows; clippy; dependency
    vulnerability audit; plus an OpenHands PR review job.
  - `ci.yml` — same shape plus a concurrency group (cancel-in-progress) and a
    schedule trigger.
  - `release-rust.yml` — `workflow_dispatch` only: verify (fmt/clippy/test and
    tag-vs-Cargo.toml version consistency) then a 6-target build matrix (linux
    x64/aarch64, android aarch64, macos x64/aarch64).
- **CI live status:** not queried from the API this session; recorded as configuration
  evidence only. Distribution/release is classified as **configuration-only evidence**
  — no release artifact was produced or inspected.

## 5. Canonical editing contract (AWE-018)

### 5.1 Vocabulary (single owner: `src/services/edit.rs`)

| Concept | Canonical definition | Location |
| --- | --- | --- |
| `EditId` | `edit-<unix-nanos>-<seq>` process-unique id; durable-recovery lookup keys validate it as a safe `.agent` filename component (no separators/traversal/control chars, max 128 bytes) | `edit.rs` `EditId`, `is_safe_edit_id` |
| `EditOperation` | 5 mutation primitives: `Replace` (exact string, optional 1-based `occurrence`), `Insert` (before 1-based line; 0 = file start), `DeleteRange` (1-based inclusive lines), `Patch` (ordered normalized operations), `ApplyDiff` (unified diff) | `edit.rs` `EditOperation` |
| `ExpectedState` | optimistic-concurrency precondition with components `hash` (SHA-256), `size`, `lines`, `context` (must-match text) | `edit.rs` `ExpectedState` |
| `FileState` | observed on-disk fact (`hash`, `size`, `lines`) | `edit.rs` `FileState` |
| `StateMatch` | `Matches` / `Mismatch` / `Missing` | `edit.rs` `StateMatch` |
| `EditStatus` | lifecycle state machine; terminal statuses are irreversible; `can_transition_to` pins legal moves | `edit.rs` `EditStatus` |
| `EditError` | structured taxonomy incl. `AuthorizationDenied` and `Conflict` with `ExpectedStateConflictPayload`, verification and recovery payloads | `edit.rs` `EditError` |
| `PatchResult` / `PatchStepResult` / `NormalizedOperation` | per-step patch application outcome | `edit.rs` |
| `ProvenanceRecord` | one committed edit's actor, refs, result states | `src/services/snapshot.rs` |
| `EditIdentity` / `EditRefs` | typed agent/session binding of an edit | `edit.rs` |

**Competing-definition audit.** No second edit-vocabulary module exists. Two distinct,
legitimately separate concepts share names and must not be conflated:
`src/context/snapshot.rs` (context-engine token-budget snapshot — unrelated to file
snapshots) and MCP **protocol** sessions (`SessionLifecycle` in `mcp/dispatcher.rs` —
transport-only, separate from AWH agent sessions in `models/session.rs`). Both facts
were previously misread as duplication by earlier status documents; they are not.

### 5.2 Lifecycle (every stage has a status in section 10)

```text
transport authentication (bearer token / stdio spawn)
  -> MCP session initialize gate (-32002 / -32600 duplicate, protocol hardening)
  -> agent-scope routing: /{agent}/sse and /{agent}/mcp -> typed route agent
  -> SEC-001 built-in trust gate (mcp/execution_gate.rs; High default-deny)
  -> workspace-local deny policy (core/policy.rs; 3 resource-carrying tools)
  -> edit authorization boundary (services/authorization.rs *_as entry points)
  -> path validation (services/files.rs resolve_checked; edit.rs validate_path)
  -> FS-001 mutation coordination (core/fs_coordination.rs)
  -> ExpectedState check (stale-state conflicts reject before mutation)
  -> prepare (in memory), snapshot capture (SnapshotStore)
  -> apply (atomic write), post-edit verification (FileState re-read)
  -> provenance record + correlated durable audit (AuditLog::record_correlated)
  -> interface result (MCP content / CLI stdout + exit code)
```

**Separation of concerns (audited in tree):** domain logic in `services/edit.rs`;
transport in `mcp/`; authorization split across `services/authorization.rs` (edit
decisions), `mcp/execution_gate.rs` (built-in trust), `core/policy.rs` (deny rules);
filesystem safety in `services/files.rs`; coordination in `core/fs_coordination.rs`;
snapshots/provenance in `services/snapshot.rs`; audit in `services/audit.rs`. The CLI
is a thin adapter (`cli/fs_edit.rs`, 595 lines) over `EditService`; MCP tools translate
JSON-RPC arguments into the same service calls.

### 5.3 Contract semantics pinned by tests

- **Zero side effects on denial** — an authorization/trust denial performs no
  filesystem work and leaves every target byte-identical:
  `tests/mcp_builtin_tool_gate.rs` (74), `tests/mcp_policy_gate.rs` (40),
  `tests/mcp_agent_routes.rs` (38), and (on open PR #126, not yet on rust)
  `tests/acceptance_edit_gates.rs` (3, including a positive control proving the
  no-writes assertion is not vacuous).
- **Conflict behavior** — an `ExpectedState` mismatch rejects with a structured
  conflict payload naming observed vs expected state before any mutation; CLI maps
  `FsError::Conflict -> 4`, `Recovery -> 5`, `Authorization -> 3`, `Usage -> 2`,
  `Service -> 1` (`cli/fs_edit.rs`); MCP returns the equivalent JSON-RPC error.
- **Verification** — post-edit verification re-reads actual on-disk state and
  distinguishes verification failure from apply failure and from stale-state conflict
  (AWE-008; `docs/FEATURES.md` marks it completed; exercised by `tests/cli_fs_edit.rs`
  and inline service tests).
- **Rollback** — by exact `edit_id`; refuses to overwrite external changes made after
  the edit (produced-state guard); rolling back an already-rolled-back edit is a
  deterministic recovery error, not a silent no-op; corrupted or missing snapshot
  blobs fail closed (`services/edit.rs` rollback family; `tests/cli_fs_edit.rs`
  recovery cases).
- **Text plane** — editing operates on UTF-8 text; non-text files are rejected rather
  than mangled. Declined and failed operations leave targets byte-identical.

## 6. Current-state contract matrix

| Editing stage | Current state | Evidence (source; tests) |
| --- | --- | --- |
| Edit transaction model (AWE-001) | Implemented & verified | `services/edit.rs` (9,480 lines incl. inline unit tests); `tests/cli_fs_edit.rs` (29) |
| Replace / Insert / DeleteRange engine (AWE-002..005) | Implemented & verified | engine in `edit.rs`; MCP via `tests/mcp_server.rs` (36) + `tests/mcp_executable.rs` (21); CLI via `tests/cli_fs_edit.rs` |
| Patch + ApplyDiff with normalization (AWE-004) | Implemented & verified | `edit.rs` `patch_as`; MCP `filesystem.patch` / `filesystem.apply_diff` schemas in `mcp/dispatcher.rs`; `tests/cli_fs_edit.rs` |
| Conflict detection / ExpectedState (AWE-006) | Implemented & verified | `edit.rs` `StateMatch`; CLI `--expected-*` flags; **MCP-side exposure absent — sections 7 and 12** |
| Atomicity, verification, recovery boundaries (AWE-007/008) | Implemented & verified | `edit.rs` verification payloads; `tests/cli_fs_edit.rs` |
| Edit authorization boundary (AWE-011/TW-004) | Implemented & verified | `services/authorization.rs` (1,025 lines); `tests/mcp_policy_gate.rs`, `tests/mcp_builtin_tool_gate.rs`, PR #126 gates suite |
| Snapshots + provenance (AWE-012/TW-005) | Implemented & verified | `services/snapshot.rs` (1,478 lines): `.agent/snapshots/<edit_id>/...`, `.agent/provenance/<edit_id>.json`; inline + CLI recovery tests |
| Edit-level rollback + recovery (AWE-010/TW-006) | Implemented & verified | `edit.rs` rollback family; `tests/cli_fs_edit.rs` recovery cases; MCP `filesystem.rollback` |
| Durable structured audit (AWE-013/TW-007) | Implemented & verified | `services/audit.rs` (1,445 lines): JSONL with per-line SHA-256 checksum, monotonic `sequence`, rotation at 10k, degraded-mode buffer, redaction choke point; PR #126 fixes the `awh fs` init defect |
| MCP editing surface (AWE-009/AWE-016) | Implemented & verified (6 tools) | `mcp/dispatcher.rs` `filesystem.*`; `tests/mcp_server.rs`; real-client validation from Prompt 11; PR #126 real-binary suites |
| CLI editing surface (AWE-014) | Implemented & verified (8 commands) | `cli/fs_edit.rs`: replace, insert, delete-range, patch, apply-diff, verify, history, rollback; `tests/cli_fs_edit.rs` (29) |
| Agent identity & sessions (TW-002) | Implemented & verified | `services/agent_runtime.rs` (784), `core/sessions.rs`, `core/identity.rs`; `tests/agent_runtime_cli.rs`, `tests/agent_cli.rs` |
| Agent-scoped MCP routing (TW-003) | Implemented & verified | `mcp/agent_route.rs`; routes `/{agent}/sse`, `/{agent}/mcp` in `mcp/http.rs`; `tests/mcp_agent_routes.rs` (38) |
| Public-MCP TLS guard (SEC-002) | Implemented & verified | `tests/sec_002_public_mcp_tls_guard.rs` (33) |
| Filesystem TOCTOU coordination (FS-001) | Implemented & verified (documented residual) | `core/fs_coordination.rs` (667 lines); `tests/fs_coordination.rs` (42) |
| Worktree isolation (GIT-001) | Implemented & verified (lifecycle only) | `services/worktree.rs` (789 lines): create/list/inspect/remove/reconcile; `tests/worktree_cli.rs` (26) |
| Service/store convergence (ARCH-001) | Implemented & verified | merged PR #122: `mcp/memory.rs` is a re-export shim; `mcp/workspace.rs` is a `FilesService` adapter; `core::memory`/`core::tasks` canonical; `models/memory.rs` + `models/task.rs` deleted; `tests/store_convergence.rs` (11) |
| Workspace init identity (TW-001) | Implemented & verified | `services/init.rs`, `core/identity.rs`; `tests/init_cli.rs` (24) |
| Control API editing routes | **Absent** | `api/control.rs` exposes whole-file `files/content` PUT via `FilesService` only — no EditService/transactional route (gap 12.1) |
| TUI editing | **Raw editor only** | `tui/screens/editor.rs` is a plain text-editor UI; no `EditService` reference anywhere in `src/tui/` |
| Worktree merge / reintegration | **Absent** | `services/worktree.rs` exposes create/list/inspect/remove only |
| Session-to-worktree effective-root wiring | **Partial** | `WorktreeStore::resolve_effective_root(session_id)` exists and is store-tested, but no MCP/API call site consumes it end-to-end (grep: no callers outside `services/worktree.rs` / `cli/worktree.rs`) |

## 7. Interface parity

Two interfaces expose the canonical editing plane today:

| Capability | MCP (`filesystem.*`) | CLI (`awh fs ...`) | Service |
| --- | --- | --- | --- |
| replace / insert / delete_range | yes (`filesystem.replace/insert/delete_range`) | yes | `EditService::*_as` |
| patch / apply_diff | yes | yes | `patch_as` / apply-diff path |
| rollback by edit_id | yes | yes | rollback family |
| history / verify | **no** (no MCP tools) | yes (`fs verify`, `fs history`) | store-level APIs exist |
| ExpectedState precondition args | **no** — schemas expose only `path/old/new/occurrence`, `path/line/content`, `path/start_line/end_line`, `operations[]`, `diff` (verified in `dispatcher.rs` schema JSON) | yes (`--expected-hash/size/lines/context`, `cli/fs_edit.rs`) | `ExpectedState` |
| Structured failure codes | JSON-RPC error codes | exit 0/1/2/3/4/5 (Service/Usage/Authorization/Conflict/Recovery) | — |
| Control API | **no edit-plane route** | n/a | — |
| TUI | **no edit-plane backing** | **no edit-plane backing** | — |

**Parity evidence.** Replace + rollback + conflict semantics are proven equivalent
across MCP and CLI by PR #126's acceptance suites, which drive a real `awh` MCP server
process over stdio and the real CLI binary. The `expected_*` argument asymmetry is a
documented interface difference, not a parity claim: an MCP caller currently cannot
express an optimistic-concurrency precondition; only a CLI caller can. Recorded as
gap 12.4.

## 8. Security and safety contract

### 8.1 Authorization stack (order matters, each layer audited in tree)

1. **Transport auth** — bearer token constant-time comparison for HTTP/SSE; stdio is
   process-spawn trust. Public `healthz` only.
2. **MCP session lifecycle** — pre-initialize requests get `-32002`; duplicate
   initialize loses deterministically with `-32600` (atomic CAS, pinned by test).
3. **Agent-scope routing (TW-003)** — `/{agent}/sse` + `/{agent}/mcp` resolve the route
   segment to a registered, enabled agent; URL namespace is a selector, **never**
   authority; requests still traverse the capability/policy engine.
4. **SEC-001 built-in trust gate** (`mcp/execution_gate.rs`, trust id `awh.builtin`)
   — **High-risk built-ins fail closed with no trust record** (terminal execution,
   connector dispatch, external-provider mutations). **Medium-risk workspace-local
   mutations are unrestricted when no record exists** (the documented, deliberate
   backward-compatible default: absence of a record means "no restriction
   configured", not implicit trust); once a record exists, it is enforced exactly
   like High — covered permissions required, blocking level or version → deny, and
   every infrastructure failure (corrupt `trust.json`, unregistered name) fails
   closed. The Medium default is a documented risk posture, not an oversight;
   changing it is a product decision.
5. **Workspace-local deny policy** (`core/policy.rs`) — deny-only rules for exactly the
   three resource-carrying built-ins (`workspace.write_file`, `workspace.delete_file`,
   `terminal.run`); forward-slash component-prefix match for the workspace tools,
   exact case-sensitive match for `terminal.run`; fail-closed on an unreadable store.
6. **Edit authorization boundary (AWE-011)** — `services/authorization.rs`
   `EditAuthorizer`: exact `agent_id`, required `Permission::Filesystem`, RFC-3339 UTC
   expiry with malformed-entries-fail-closed, component-prefix scope matching
   (`src/foo` matches `src/foo/bar.rs`, never `src/foobar`), **policy deny precedence
   over capabilities**, infrastructure failure = Deny. Enforced only through the
   `*_as` entry points; the plain methods are the documented trusted-operator surface
   (in-process services, CLI, tests) — no transport constructs an operator principal.

### 8.2 Filesystem/path safety

- `services/files.rs` `resolve_checked`: traversal + symlink-escape rejection
  (canonicalize deepest existing ancestor; bounded symlink-chain hops); workspace
  containment is absolute.
- `edit.rs` `validate_path`: same rules for edit arguments before any operation.
- 2 MiB read / 5 MiB write caps on the basic workspace tools; editing operates on
  UTF-8 text only.

### 8.3 TOCTOU / mutation coordination (FS-001) — precise claim

`core/fs_coordination.rs` closes the practical race windows **among AWH-conformant
actors**: an in-process per-resource mutex registry (threads in one `awh` process) and
a cross-process advisory lock (`O_CREAT|O_EXCL` / `CREATE_NEW` lock files under
`.agent/fs-coordination/<sha256-of-canonical-root+rel-path>.lock` for sibling `awh`
processes). Resource keys are derived only after canonical validation. Multi-resource
acquisition sorts keys to prevent deadlock cycles; guards release on drop including
panic; acquisition failure fails closed. Callers **revalidate path containment and
live state after acquisition** and commit via atomic rename.

**Documented residual, not fixable portably:** the lock is advisory — a foreign
(non-AWH) process is not stopped by it, and the final-component race against non-AWH
actors is narrowed (revalidation under lock + atomic rename) but cannot be closed. This
document deliberately does not claim TOCTOU-freedom; it claims serialized
AWH-actor mutation with a bounded, explicitly documented window against outsiders.
`StoreLock` remains the separate canonical owner for `.agent` JSON store files; the
two guard disjoint resource classes.

### 8.4 Evidence for the security posture

- `tests/mcp_builtin_tool_gate.rs` (74), `tests/mcp_policy_gate.rs` (40),
  `tests/mcp_agent_routes.rs` (38), `tests/mcp_security.rs` (34),
  `tests/mcp_sandbox.rs` (15), `tests/sec_002_public_mcp_tls_guard.rs` (33),
  `tests/fs_coordination.rs` (42), `tests/init_cli.rs` (24 — foreign-manifest and
  corrupt-state fail-closed), PR #126 acceptance gates (3).
- Audit redaction at the `AuditLog::record` choke point (token-shaped runs replaced);
  audit is observational and never an authorization input.

## 9. Snapshot / provenance / rollback / audit contract

- **Snapshots** — `SnapshotStore` writes pre-edit bytes under
  `.agent/snapshots/<edit_id>/<relative-path>` before any mutation; content hashes are
  verified on load; corrupted blobs fail closed rather than fabricating state.
  Workspace binding: an `.agent` directory copied from another root is detected via
  the manifest's canonical-root record and rejected (TW-001).
- **Provenance** — one JSON record per committed edit at
  `.agent/provenance/<edit_id>.json`: actor identity, refs, before/after states,
  outcome. This is the durable explanation layer; audit is the observational layer;
  they are deliberately distinct.
- **Rollback** — exact-`edit_id` lookup; refuses to roll back when the file changed
  after the edit (produced-state guard, structured `Recovery` error); already-rolled-back
  edits fail deterministically; multi-edit rollback aborts atomically without partial
  application.
- **Durable audit (AWE-013)** — `.agent/audit/audit.log` append-only JSONL with a
  SHA-256 checksum per line; strictly monotonic `sequence` recovered at startup (never
  reused after restart); rotation at 10,000 events keeps one previous generation;
  torn final line is detected and excluded, mid-file corruption is a hard error
  (evidence never silently replaced); pre-init events buffer bounded and replay;
  redaction at the single choke point. **Durability point is the OS write buffer** — a
  crash may lose the tail; this is a documented tradeoff, not a bug (audit must not
  dominate operation latency). PR #126 fixed the last known init gap (`awh fs`
  previously wrote to the ring only).
- **Correlation** — `AuditLog::record_correlated` ties allow/deny events to the
  agent/session pair, and edit provenance records carry `EditIdentity`/`EditRefs`, so
  "which agent changed what, when, under which decision" is answerable from durable
  state after restart.

## 10. Current implementation status (AWE-019)

Classification vocabulary (two dimensions, no scores):
- **Runtime state:** Absent / Partial / Implemented.
- **Evidence state:** Unverified / Service-verified (unit+integration) / Real-interface
  verified (spawns real binary / real MCP client) / Persistence/concurrency-verified
  (restart, multi-process, crash-window injection).

See the matrix in section 6 for the per-stage table. Summary by subsystem:

| Subsystem | Runtime | Evidence |
| --- | --- | --- |
| Edit engine + conflict + verification + rollback | Implemented | Real-interface (CLI + MCP binaries; PR #126) + failure-injection (corrupt snapshot, repeat rollback, stale state) |
| Authorization stack (trust gate, policy, capability edit boundary) | Implemented | Real-interface + zero-side-effect negative suites |
| FS coordination | Implemented | Concurrency-verified (cross-process lock tests in `tests/fs_coordination.rs`) |
| Snapshots/provenance | Implemented | Persistence-verified (restart/recovery paths) |
| Audit | Implemented | Persistence-verified (rotation, checksum, restart ordering) |
| Agent identity/sessions/routes | Implemented | Real-interface CLI + HTTP route tests |
| Worktree lifecycle | Implemented (lifecycle only; no merge) | Real-interface CLI (`tests/worktree_cli.rs` 26) |
| Store convergence (memory/tasks/files) | Implemented | Cross-plane convergence tests (`tests/store_convergence.rs` 11) |
| Control API edit plane | Absent | n/a |
| TUI edit plane | Absent (raw editor) | n/a |
| Session-to-worktree effective-root consumption | Partial (store API only) | Service-verified only |

**No aggregate score is offered by design.** Historical percentage estimates are
attributed in section 13 and do not override any row here.

## 11. Evidence inventory

**Unit/service (inline in src, run by `cargo test --workspace`):** `services::edit`,
`services::snapshot`, `services::audit`, `services::authorization`,
`services::worktree`, `services::agent_runtime`, `services::init`, `core::fs_coordination`,
`core::identity`, `core::sessions`, `core::policy`, `mcp::schema`, `mcp::audit`, TUI modules.

**Integration binaries (tests/):** `cli_fs_edit.rs` (29), `fs_coordination.rs` (42),
`mcp_server.rs` (36), `mcp_executable.rs` (21), `mcp_protocol.rs` (80),
`mcp_http.rs` (31), `mcp_agent_routes.rs` (38), `mcp_builtin_tool_gate.rs` (74),
`mcp_policy_gate.rs` (40), `mcp_security.rs` (34), `mcp_sandbox.rs` (15),
`mcp_dynamic_tools.rs` (31), `mcp_dispatcher_lifecycle.rs` (6),
`sec_002_public_mcp_tls_guard.rs` (33), `worktree_cli.rs` (26), `agent_cli.rs` (7),
`agent_runtime_cli.rs` (24), `init_cli.rs` (24), `architecture.rs` (7),
`store_convergence.rs` (11) — plus PR #126's `acceptance_edit_gates.rs` (3) and
`acceptance_editing.rs` (9) once merged.

**Failure-injection / durability:** corrupt snapshot blob → fail closed; corrupt
`.agent` manifests/policy → fail closed; torn audit line → excluded; mid-file audit
corruption → hard error; crashed reservation reconciliation (worktree `Creating`
records) → deterministic `Missing`; duplicate initialize race → exactly one winner.

**Python pipeline/contract tests (repo automation, not run this session):**
`tests/awh_pipeline_test.py`, `checkpoint_state_test.py`, `safe_git_test.py`,
`awh_merge_guard_test.py`, `automation_sha_contract_test.py`, `reviewer_v2_test.py`,
`stale_event_termination_test.py`, `awh_dispatcher_test.py` — recorded as existing
evidence for the CI-automation plane, unverified in this session.

**Recorded runs:** rust@`0d3a029` 1227 passed / 0 failed (23 result sections);
PR #126 head 1228 passed / 0 failed; fmt clean on both; clippy `-D warnings` clean on
the PR branch (CI covers rust).

## 12. Known gaps / issues (current, evidence-backed; each with next step)

1. **Control API has no edit-plane exposure.** `api/control.rs` offers whole-file
   `files/content` PUT (FilesService) but no EditService route; remote TUI/API clients
   cannot do transactional edits, preconditions, snapshots, or rollback. *Impact:*
   the "every plane calls the canonical service" rule holds for memory/tasks/files,
   but the edit plane is MCP+CLI only. *Next:* add `/api/v1/fs/...` handlers as thin
   adapters over `EditService` (mirroring `cli/fs_edit.rs`), with the same
   authorization boundary.
2. **Control API `terminal/run` runs with the server process's workspace root as cwd
   and no per-agent capability check** (`api/control.rs` `run_command`: empty-program
   check, then spawn; audited but not capability-gated; forensic report flagged this
   and it remains current). *Next:* gate on capability + policy like MCP
   `terminal.run`, and bind cwd to the claimed agent's effective root.
3. **TUI editing is not transactional.** `tui/screens/editor.rs` is a raw editor with
   no EditService/snapshot/authorization path. *Next:* back the editor screen with
   `EditService` operations or route saves through the canonical edit path.
4. **MCP `filesystem.*` tools do not expose ExpectedState preconditions** (schemas
   verified: no expected_* fields). MCP callers cannot express stale-state guards;
   only CLI can. *Next:* extend the six tool schemas with optional
   `expected_hash/size/lines/context` passthrough to the service.
5. **MCP has no `verify`/`history` tools** while the CLI does; agents operating purely
   over MCP cannot inspect edit history or re-verify. *Next:* add read-only
   `filesystem.history` + `filesystem.verify` (no new mutation surface).
6. **Worktree merge/reintegration absent** (`services/worktree.rs`: create/list/
   inspect/remove only). *Next:* design a merge/reap step that composes with
   capability/policy and keeps unmanaged worktrees unadopted.
7. **Session-to-worktree effective-root is store-level only.**
   `resolve_effective_root` has no MCP/API consumer; an agent session does not yet
   transparently edit inside its worktree. *Next:* wire MCP workspace/filesystem tool
   roots through the session's effective root (this is the largest remaining gap to
   the roadmap's multi-agent story).
8. **Documentation drift (reported, not fixed here — out of scope):**
   - `docs/mcp.md` + `README.md` still claim a **53 core / 65 total** catalog; the
     dispatcher advertises **59 core / 71 total** (71 unique tool names counted in
     `mcp/dispatcher.rs` schemas: 59 incl. the six `filesystem.*`, + 12 `github.*`).
   - `docs/CLI.md` lists `fs read/write/stat/search/hash` (planned P0 step 6) — none
     exist as CLI commands; basic fs operations are MCP (`workspace.*`) and Control
     API only.
   - `docs/CLI.md` and `docs/roadmap/PROJECT_ROADMAP.md` list `awh workspace/git/
     capability/snapshot/audit/session/context/memory/task/doctor/version/config`
     families; the actual `Command` enum has 12 families: Init, Status, Tui, Serve,
     Mcp, Skill, Registry, Agent, Worktree, Policy, Fs, Tunnel.
   - `docs/testing.md` test-count claims are stale (claims ~123; current suite is
     ~1227).
   - `docs/security.md` cites `src/secure_path` (module does not exist; the path
     logic lives in `src/services/files.rs`), and `src/core/fs_coordination.rs`
     cites `docs/filesystem-coordination.md` (file does not exist on rust);
     `mcp/execution_gate.rs`'s doc comment says path-scoped policy "arrives with
     the Phase 3 policy engine" and per-agent scoping "with Phase 5" — both
     already exist (`core/policy.rs`; AWE-011 boundary), so the comment understates
     current behavior.
9. **Audit tail-loss on crash** (OS-buffer durability point) — documented tradeoff;
   accept or add an fsync option for high-assurance deployments. *Next:* make the
   durability point configurable and document the cost.
10. **Advisory-only cross-process locks** — by design (portable truth); a non-AWH
    actor is not stopped. *Next:* none required technically; keep the wording precise
    in all docs (this artifact does).
11. **Medium-risk default posture** (SEC-001) — with no `awh.builtin` trust record,
    Medium-risk workspace-local mutation tools run unrestricted (documented
    backward-compatible default; a record restricts them). Revisit only as an
    explicit product decision with migration notes.
12. **Release/distribution:** workflow exists (`release-rust.yml`) but no recorded
    successful release run/artifact was inspected; treat "installable binaries" as
    unverified. *Next:* run the release dispatch once and record evidence.
13. **No `awh doctor/version/config` CLI** (roadmap Phase 0 items unimplemented).

**Gaps closed since the forensic reports (with closing evidence):** duplicate
edit/filesystem engines → one canonical `EditService` + ARCH-001 convergence
(`tests/store_convergence.rs`); missing snapshots/provenance → Prompt 08
(`services/snapshot.rs` + tests); missing rollback → Prompt 09 + `tests/cli_fs_edit.rs`
recovery cases; ring-only audit → Prompt 10 durable store (+ PR #126 `fs` init fix);
absent agent/session binding → TW-002/TW-003 (`tests/agent_runtime_cli.rs`,
`tests/mcp_agent_routes.rs`); absent worktrees → GIT-001 (`tests/worktree_cli.rs`);
raw TOCTOU windows → FS-001 (`tests/fs_coordination.rs`) with the documented advisory
residual. Claims that were **wrong then and are wrong to repeat now:** "no session
model exists" (2025-09-09 forensic; superseded), "snapshots ~5%" (STATUS.md v2,
superseded), "first-class worktrees do not exist" (STATUS.md v2, superseded).

## 13. Historical-status reconciliation

| Historical claim (source, date) | Current finding on rust @ `0d3a029` |
| --- | --- |
| "MCP-first is only half-true" — duplicate filesystem/memory engines (`AWH_FORENSIC_REPORT.md`, 2025-09-09, tree @ `0e38041`) | **Closed.** ARCH-001 (PR #122) made `core::memory`/`core::tasks` canonical, turned `mcp/memory.rs` into a re-export shim and `mcp/workspace.rs` into a `FilesService` adapter, and deleted the dead one-file-per-record stores; pinned by `tests/store_convergence.rs` (11). |
| "No snapshot/provenance/undo modules exist" (forensic report, 2025-09-09; STATUS.md v2 2026-09-20 estimated "Snapshots ~5%") | **Closed.** `services/snapshot.rs` (1,478 lines) implements durable snapshots + provenance; `services/edit.rs` implements rollback/recovery; verified by CLI recovery tests and inline suites. |
| "No distinct Session model exists… sessions ~20%" (forensic; STATUS.md v2) | **Closed.** `models/session.rs` + `core/sessions.rs` + `services/agent_runtime.rs`; lifecycle table with terminal states; `tests/agent_runtime_cli.rs` (24). |
| "First-class worktrees do not exist at all" (STATUS.md v2, corrected "~45%") | **Closed (lifecycle).** `services/worktree.rs` + `awh worktree create/list/inspect/remove` + crash reconciliation; `tests/worktree_cli.rs` (26). Merge/reintegration remains absent (gap 12.6). |
| "Audit is process-local ring only" (forensic) | **Closed.** durable JSONL audit (Prompt 10) + PR #126 fixed the last `awh fs` init gap. Residual: crash-tail durability point (gap 12.9). |
| "No per-agent enforcement / session-to-agent binding" (STATUS.md v2) | **Closed for identity+authorization plumbing:** typed ids (TW-001), agent profiles/sessions (TW-002), agent-scoped MCP routes with capability gate (TW-003), edit authorization boundary (AWE-011). **Open:** Control API `terminal.run` capability gap (12.2) and session-to-worktree effective-root consumption (12.7). |
| "Editing (AWE-001..004) designed but unexecuted" (STATUS.md v2) | **Closed.** Engine, conflict, verification, rollback all implemented and verified (section 6). |
| STATUS.md v2 percentages (e.g. MCP ~95%, Context ~75%) | **Attributed historical estimates only.** Superseded by per-row evidence in sections 6/10; not carried forward. |

All percentage figures above are attributed history, not current status.

## 14. Roadmap authority and contradictions

- **Forward build plan (authoritative for intent):** `docs/roadmap/PROJECT_ROADMAP.md`
  — its phase/CLI lists are targets, not claims. Its "final CLI contract" (§4:
  `awh workspace/git/capability/snapshot/audit/session/context/memory/task/doctor/
  version/config`) is largely **unbuilt**; the real `Command` enum today is:
  Init, Status, Tui, Serve, Mcp, Skill, Registry, Agent, Worktree, Policy, Fs, Tunnel.
- **Adoption strategy:** `docs/roadmap/GROWTH_STRATEGY.md` — marketing/positioning,
  non-authoritative for engineering status.
- **Target multi-agent model:** `docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md` —
  design intent for per-agent routes/policy; partially realized (TW-003 routes exist;
  per-agent policy granularity beyond deny-rules is not built).
- **Historical snapshots (not status):** `docs/roadmap/STATUS.md` (v2, 2026-09-20 —
  itself a *correction* of earlier percentage documents, and now corrected by this
  artifact), `AWH_FORENSIC_REPORT.md` + `AWH_FORENSIC_REPOSITORY-REPORT.md`
  (2025-09-09), `docs/archive/*`.
- **Feature contract:** `docs/FEATURES.md` — target contract with per-milestone
  completion notes; its AWE-008-completed note matches source; its "AWE-009 and later
  must not be inferred complete" caveat is superseded by evidence in section 6 for
  AWE-009..AWE-017 items listed there.
- **Known contradictions (current):** the documentation-drift items in gap 12.8
  (mcp.md/README tool counts 53/65 vs actual 59/71; CLI.md command list; testing.md
  counts; security.md `src/secure_path`). This artifact does not edit those files
  (out of scope); they are recorded as required by the evidence rules.
- **Prompt-collection authority:** `docs/implementation-prompts/README.md` declares
  this directory the single canonical prompt home; the old `docs/trust-wedge/` and
  `docs/issue-resolving-prompts/` collections were replaced and are **absent from the
  current tree** — their content survives only in git history and in the issues the
  prompts consolidated (AWE-*, TW-*, GIT-001, FS-001, ARCH-001, SEC-00*).

## 15. Dependency-ordered roadmap (next work, order matters)

| # | Work item | Depends on | Unlocks |
| --- | --- | --- | --- |
| 1 | Merge PR #126 (acceptance suite + `fs` audit-init fix) | — | Real-interface acceptance evidence on rust; later items get a safety net |
| 2 | MCP `expected_*` passthrough + `filesystem.history`/`verify` tools (12.4, 12.5) | 1 | MCP/CLI parity completion for the editing plane |
| 3 | Session-to-worktree effective-root wiring in MCP tool construction (12.7) | 1 (worktrees + sessions exist) | The roadmap's multi-agent isolation story: one agent session edits only inside its worktree |
| 4 | Control API edit-plane routes as thin EditService adapters (12.1) | 1 | API/TUI parity for editing |
| 5 | Control API `terminal.run` capability+policy gate and per-agent cwd (12.2) | 3 | Closes the last known un-gated mutation surface |
| 6 | Worktree merge/reintegration (12.6) | 3 | Full GIT-001 lifecycle |
| 7 | TUI editor backed by EditService (12.3) | 4 | Transactional edits from the TUI |
| 8 | Documentation drift cleanup (12.8: mcp.md/README counts, CLI.md, testing.md, security.md paths) | independent; do with 2 | Trust in interface docs |
| 9 | Release dispatch run + recorded artifacts (12.12) | independent | Verifiable distribution |
| 10 | `awh doctor/version/config` CLI + `fs read/write/stat/search/hash` decision (12.13, 12.8) | independent | Operator ergonomics; resolve the CLI.md/roadmap vs reality question explicitly |

Out of scope for AWH (restate): agent reasoning, model routing, swarm scheduling.

## 16. Phase/acceptance gates

The editing phases now satisfy the required gate model (implementation → unit →
integration → real-interface → failure/recovery → documentation):

- implementation + unit: inline suites in `services::*`;
- integration: `tests/cli_fs_edit.rs`, `tests/mcp_server.rs`, etc.;
- real-interface: PR #126 acceptance suites (real CLI binary; real MCP server over
  stdio) — the layer this collection was missing before Prompt 16;
- failure/recovery: corrupt-blob, stale-state, repeat-rollback, crash-reconciliation,
  torn-audit-line, duplicate-initialize cases;
- documentation: this artifact (note: `src/core/fs_coordination.rs` cites a
  `docs/filesystem-coordination.md` that does not exist on current rust — recorded in
  gap 12.8);
- CI gates on every PR: fmt, build+test (3 OS), clippy, dependency audit.

Remaining phases must reproduce the same gate ladder before being called done.

## 17. Unverified claims and environment limitations

- `clippy -D warnings` at `0d3a029` was not re-run this session (fmt was; tests were);
  CI covers clippy on the PR.
- Live CI status and branch-protection settings were not queried from the GitHub API
  this session; CI is cited as configuration evidence.
- Release/distribution is configuration-only evidence; no release artifact was
  inspected.
- The Python automation tests were not run this session.
- macOS/Windows behavior is exercised only by CI runners (matrix); the local
  verification was Linux. Sandbox internals on macOS/Windows are runtime-unverified
  locally.
- MCP-SDK interop harnesses (`examples/mcp-interop`) are not part of CI; last
  recorded interop evidence is from Prompt 11 sessions.
- `mcp.status` metrics are in-process only; there is no cross-process aggregate — any
  multi-instance observability claim would be unsupported.
- No performance/latency claims are made anywhere in this artifact; none were measured.

## 18. Final evidence summary

- Implementation authority: `rust` @ `0d3a029`; this branch adds documentation only.
- Verified suite: 1227 passed / 0 failed at `0d3a029` (this session); 1228 / 0 on PR
  #126 head; fmt clean both; clippy clean on PR #126 head.
- The canonical editing contract lives in `src/services/edit.rs` +
  `src/services/snapshot.rs` + `src/services/authorization.rs` +
  `src/core/fs_coordination.rs` + `src/services/audit.rs`, exposed by `mcp/dispatcher.rs`
  (`filesystem.*` tools) and `cli/fs_edit.rs` (`awh fs`), and is **not** exposed by the
  Control API or the TUI.
- Authorization order: transport → session gate → agent route → trust gate → deny
  policy → edit authorization → path validation → coordination → expected-state →
  mutation → verification → provenance/audit.
- Thirteen current gaps carry evidence and next steps (12.1–12.13); the forensic-era
  gaps are closed with named closing evidence (section 12 tail, section 13).
- Roadmap authority: PROJECT_ROADMAP for intent; this artifact for status; historical
  percentages retired.

## 19. Strict scope confirmation

For Prompt 17, exactly **one file** was modified:

- `docs/implementation-prompts/17-contract-status-and-roadmap.md` (this artifact; the
  prompt text it replaces is preserved at git commit `4769090`).

No production source, test, workflow, README, CLI/MCP doc, roadmap doc, archive doc,
or other implementation prompt was modified. No completion percentage, security score,
or maturity score was introduced. TARGET CONTRACT ≠ CURRENT IMPLEMENTATION ≠ VERIFIED
BEHAVIOR is maintained throughout: contract rows cite the contract, current-state rows
cite source, verified rows cite tests.
