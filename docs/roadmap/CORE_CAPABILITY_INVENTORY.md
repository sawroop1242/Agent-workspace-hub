# Core Capability Inventory and Trust-Wedge Gate

**Source baseline:** `ce286663bcd1aa1fb24cb3f5b6b4b7814445f7b8` (`rust`)

**Review date:** 2026-10-24

**Purpose:** make one supported core workflow explicit before adding breadth.

This is a source-backed inventory, not a release-readiness claim. “Test evidence”
means tests or recorded interoperability evidence exist in the repository. It
does **not** mean those tests were executed during this review. Every command
must therefore be read together with [Verification status](#verification-status).

## 1. Supported workflow and decision

The workflow to prove, in this order, is:

1. initialize an ordinary repository and run read-only diagnostics;
2. connect a real MCP client over a supported transport;
3. inspect files and canonical provenance;
4. deny and permit a controlled edit through the canonical policy/authorization
   boundary, with denial producing zero edit, snapshot, or provenance records;
5. inspect the resulting diff, evidence, and records;
6. roll back through the verified snapshot/recovery path.

**Disposition:** keep and improve the initialization, diagnostics, MCP stdio,
controlled `filesystem.*` edit, inspection, and rollback path. Keep its safety
controls visible. Hide optional breadth from the default quickstart rather than
deleting working integrations. Defer additional transports and remote-control
work until this path is verified on the current source.

No mass removal is proposed. A surface marked “hide” still needs a documented
opt-in path, accurate help, and security boundaries; “defer” means no new work
before the core gate passes.

## 2. Verification status

| Gate | Current evidence | State |
|---|---|---|
| Current source and PR review | PR [#153](https://github.com/sawroop1242/Agent-workspace-hub/pull/153) merged `awh doctor`; current source and tests were inspected; no open PRs were listed at review time | Reviewed |
| `cargo fmt --all -- --check` | No native `cargo`/`rustfmt` exists. Changed Rust files were formatted and parsed with `@scalar/rust-fmt` 0.3.0 (real rustfmt on WASI); this is not the required Cargo gate | **Blocked** |
| `cargo clippy --all-targets -- -D warnings` | No native toolchain | **Blocked** |
| `cargo build` / `cargo test --all` | No native toolchain or current prebuilt binary | **Blocked** |
| Current-head CI | Rust run [40102391522](https://github.com/sawroop1242/Agent-workspace-hub/actions/runs/40102391522): format passed; Linux/macOS/Windows check, clippy, and audit failed. Equivalent build/clippy/audit failure categories already existed at pre-doctor `4f62634`; detailed compiler diagnostics were unavailable. CodeQL run [40102391570](https://github.com/sawroop1242/Agent-workspace-hub/actions/runs/40102391570) passed for Rust, Actions, and Python | Existing CI failure; not attributed to doctor |
| Doctor behavior on current binary | Source review plus isolated Git pathspec/ignore probes only; no current binary | **Blocked** |
| Real MCP client on current binary | Official TypeScript SDK stdio/SSE harnesses and historical results exist in `examples/mcp-interop/` and `docs/mcp.md`; OpenCode and Codex themselves are explicitly “NOT VERIFIED” there | **Blocked for this review** |

The environment cannot reach `static.rust-lang.org`; the npm `rustc` package
contains Mach-O executables and fails on Linux. Release `v0.1.0` predates doctor
and no current Actions binary artifact exists. Consequently no diagnostic,
format, build, lint, or test result is represented as a pass without execution.

### Changes made by this review

- Removed the CLI's duplicate three-tool pre-validation from `awh policy deny`.
  `PolicyStore::add` now remains the sole canonical nine-tool validator and
  runs before creating a policy directory, acquiring a lock, or writing.
  Previously the CLI rejected six edit tools that `PolicyStore` and
  `EditService` already supported.
- Added `tests/policy_edit_cli.rs`: real-binary coverage for all six
  `filesystem.*` policy rules, denied-edit exit 3, unchanged target bytes, zero
  snapshot/content/provenance records, and an allow-after-remove positive
  control.
- Added `tests/doctor_cli.rs`: real-binary coverage for help discovery,
  uninitialized and valid workspaces, corrupt manifest/policy, missing Git,
  symlinked `.agent`, tracked `.agent`, missing ignore rules, and a valid Git
  workspace. Each case compares a complete workspace tree (file bytes,
  directories, and symlink targets) before and after doctor, including `.git`
  and `.agent`; no workspace audit or registry file may appear. The symlink
  creation case runs on Unix; cross-platform execution remains blocked.
- Left `src/mcp/tool_broker.rs` unchanged. Its three-entry resource-precheck
  table is an MCP dispatcher pre-gate, not the policy tool catalog. Filesystem
  edits authorize through `EditService`/`AuthorizationService`.

These changes are formatted and source-reviewed but **not compiled or run** in
this environment.

## 3. CLI surface

Implementation is in `src/main.rs` and the named adapters. Unless stated
otherwise, test evidence is real-binary integration coverage using
`tests/common/mod.rs`.

| Public command | Implementation and dependencies | Test evidence | Security boundary / source status | Disposition |
|---|---|---|---|---|
| `awh init [--path]` | `src/main.rs`, `src/services/init.rs`, `src/core/identity.rs`, `src/mcp/store_lock.rs`; serde/chrono/tempfile | `tests/init_cli.rs`, `tests/foundation_cli.rs`, `tests/workspace_runtime_cli.rs` | Canonical manifest, real-path binding, locking, atomic persistence, ignore rules. Implemented | **Keep/improve P0** |
| `awh status` | `src/main.rs` | `tests/foundation_cli.rs` | Read-only, but currently prints bootstrap status rather than loading deep health/provenance | **Keep/improve P0**; do not substitute for doctor |
| `awh doctor [--path]` | `src/cli/doctor.rs`, `src/services/init.rs`, `src/core/policy.rs`, Git CLI | New `tests/doctor_cli.rs`; no prior dedicated suite | Early dispatch before registry/audit initialization; read-only manifest/policy/symlink/Git/ignore checks. Source reviewed; execution blocked | **Keep/improve P0** |
| `awh mcp serve [--transport] [--host] [--port] [--api-key-env] [--tls-cert] [--tls-key]` | `src/main.rs`, `src/mcp/server.rs`, `src/mcp/http.rs`, `src/mcp/sse.rs`, `src/mcp/dispatcher.rs` | `tests/mcp_executable.rs`, `tests/mcp_protocol.rs`, `tests/mcp_http.rs`, `tests/sec_002_public_mcp_tls_guard.rs`, SDK harnesses | Stdio opens no network listener; HTTP requires bearer auth and supports TLS, sessions, tool/trust/capability/policy enforcement | **Keep P0**: stdio first; SSE after core proof |
| `awh mcp {list,add,remove,enable,disable,search,install,update,uninstall,trust,block,revoke,status,permissions} …` | `src/main.rs`, `src/mcp/custom_mcp.rs`, `src/mcp/global_mcp.rs`, `src/mcp/community_registry.rs`, `src/mcp/permissions.rs`, `src/mcp/trust_store.rs`, `src/mcp/cli_trust.rs` | `tests/mcp_security.rs`, `tests/mcp_dynamic_tools.rs`, `tests/mcp_server.rs` | Registration is not trust; explicit permissions, env/secret constraints, transport validation | **Hide from default; defer breadth** |
| `awh fs {replace,insert,delete-range,patch,apply-diff,verify,history,rollback} …` | `src/cli/fs_edit.rs`, `src/services/edit.rs`, `src/services/authorization.rs`, `src/services/snapshot.rs`, `src/core/files.rs` | `tests/cli_fs_edit.rs`, `tests/acceptance_editing.rs`, `tests/acceptance_edit_gates.rs`, `tests/rollback_recovery_boundaries.rs`, `tests/snapshot_provenance_boundaries.rs` | Canonical edit/recovery lifecycle; operator or agent principal, policy/capability checks, expected-state guards where expressible, verified atomic writes, snapshots, provenance, audit | **Keep/improve P0** |
| `awh policy {deny,list,remove} …` | `src/main.rs`, `src/core/policy.rs`, `src/mcp/store_lock.rs` | `tests/mcp_policy_gate.rs`, `tests/capability_policy_boundaries.rs`, new `tests/policy_edit_cli.rs` | Workspace-local DENY-only rules; safe IDs, canonical tool list, lock + atomic persistence. CLI parity fixed in this review | **Keep/improve P0** |
| `awh agent {create,list,inspect,grant,revoke,show,start,stop,restart,status,enable,disable,session} …`; `awh agent session {open,list,show,resolve,pause,resume,stop} …` | `src/main.rs`, `src/services/agent_runtime.rs`, `src/core/agents.rs`, `src/core/sessions.rs`, `src/core/capability_grants.rs` | `tests/agent_cli.rs`, `tests/agent_runtime_cli.rs`, `tests/capability_policy_boundaries.rs`, `tests/mcp_agent_routes.rs` | Agent identity is not ambient authority; scoped grants, session/route binding, audit. There is no `agent run` | **Keep P1** as safety/control-plane support |
| `awh worktree {create,list,inspect,remove} …` | `src/cli/worktree.rs`, `src/services/worktree.rs`, Git CLI | `tests/worktree_cli.rs`, `tests/worktree_boundaries.rs`, `tests/git_failure_injection.rs`, `tests/trust_wedge_e2e.rs` | Real Git worktrees; containment, ownership, dirty preservation, no automatic merge. No status/merge commands | **Keep P1**, after single-workspace editing proof |
| `awh context {show,save,update,clear,search} …` | `src/cli/context.rs`, `src/context/engine.rs`, `src/core/context.rs` | `tests/context_cli.rs`, `tests/context_engine_boundaries.rs`, `tests/store_convergence.rs` | Scoped context, budgets, atomic persistence, audit | **Keep P1** for inspection/handoff; not prerequisite to first edit |
| `awh memory {list,get,search,add,update,delete} …` | `src/cli/memory.rs`, `src/core/memory.rs`, `src/mcp/store_lock.rs` | `tests/memory_cli.rs`, `tests/memory_boundaries.rs`, `tests/store_convergence.rs` | Scoped memory, safe IDs, lock + atomic persistence, audit | **Keep P1** for handoff |
| `awh task {list,show,create,update,cancel,assign,delete} …` | `src/cli/tasks.rs`, `src/core/tasks.rs`, `src/mcp/store_lock.rs` | `tests/tasks_cli.rs`, `tests/store_convergence.rs` | Task state/assignment validation, lock + atomic persistence, audit | **Keep P1**; hide from minimal quickstart |
| `awh skill {create,list,show,read,add,remove,enable,disable,project,search,install,uninstall} …` | `src/main.rs`, `src/skills/mod.rs`, `src/skills/registry.rs`, `src/skills/project.rs`, `src/skills/installer.rs` | `tests/skills_boundaries.rs`, skill unit tests | Package/ref validation, SHA-256 integrity, project scoping, enablement is not arbitrary execution | **Hide from default; defer** |
| `awh registry {add,remove,list,search} …` | `src/main.rs`, `src/skills/registries.rs`, `src/skills/registry_client.rs` | `tests/skills_boundaries.rs`, registry unit tests | Remote registry URLs, transport/integrity, local persistence | **Hide from default; defer** |
| `awh terminal {run,list,kill} …` | `src/cli/terminal.rs`, `src/services/terminal.rs` | `tests/terminal_cli.rs`, `tests/mcp_builtin_tool_gate.rs` | Direct argv (no shell), timeout/output caps, audit. Lifecycle is ephemeral: list is empty and kill rejects every ID. **No OS sandbox or command allowlist** | **Hide from default; defer** |
| `awh collaboration {agents,status,assign,activate,handoff,accept,release,conflicts,events} …` | `src/cli/collaboration.rs`, `src/core/collaboration.rs`, `src/services/collaboration.rs` | `tests/collaboration_cli.rs`, collaboration unit tests | Ownership/revision/session/task/worktree consistency; not a merge workflow | **Hide from default; defer** |
| `awh serve [--host] [--port] [--api-key-env]` | `src/main.rs`, `src/api/control.rs` | `src/api/control.rs` route/auth tests, `tests/foundation_cli.rs` | Authenticated Control API except health; body/rate/timeout layers. Separate from MCP auth/policy path | **Hide from default; defer** |
| `awh tui [--remote] [--api-key] [--api-key-env]` | `src/main.rs`, `src/tui/mod.rs`, `src/tui/remote.rs` | TUI unit tests | Local or bearer-authenticated remote API client; not an agent execution engine | **Hide from default; defer** |
| `awh tunnel {start,status} [--provider] …` | `src/main.rs`, `src/tunnel/mod.rs` | Tunnel unit tests, `tests/foundation_cli.rs` | Explicit external process/tunnel, env token handling; a tunnel is transport exposure, not authentication | **Defer; opt-in only** |
| `awh --help`, `awh --version` | clap in `src/main.rs`, Cargo package metadata | `tests/foundation_cli.rs`, new doctor help assertion | Discovery only; no workspace mutation | **Keep P0** |

Not implemented as public CLI commands despite broader roadmap vocabulary:
`workspace`, `git`, `capability`, `snapshot`, `audit`, `logs`, `connector`,
`config`, `completion`, a general `agent run`, or basic `fs
read/write/stat/search/hash`. Planned commands in `docs/FEATURES.md` are not
treated as existing.

## 4. MCP tools

The canonical registry is `src/mcp/tool_registry.rs`; request dispatch and
schemas are in `src/mcp/dispatcher.rs`. There are **73 registered tools**: 61
core plus 12 runtime-optional `github.*` tools. GitHub execution requires its
token/configuration. Common dependencies are `serde_json`, `jsonschema`,
canonical stores/services, and the dispatcher's trust/capability/policy/audit
boundaries.

Catalog/schema coverage comes from `tests/mcp_executable.rs`,
`tests/mcp_protocol.rs`, `tests/mcp_dynamic_tools.rs`, and family-specific
suites below. Historical SDK interoperability is recorded in `docs/mcp.md`, not
rerun here.

| Family and every registered tool | Implementation and dependencies | Test evidence | Security boundary / status | Disposition |
|---|---|---|---|---|
| `workspace.context`, `workspace.read_file`, `workspace.write_file`, `workspace.delete_file`, `workspace.list_files` | `src/mcp/workspace.rs`, `src/mcp/dispatcher.rs`, `src/services/files.rs`, `src/core/files.rs` | `tests/mcp_server.rs`, `tests/mcp_builtin_tool_gate.rs`, `tests/fs_basic_boundaries.rs`, `tests/mcp_policy_gate.rs` | Workspace containment, symlink/path checks, size/depth limits; mutations cross trust/capability/policy/audit. Full-file write is not the controlled snapshot edit path | **Keep P0** for context/read/list; use `filesystem.*` for controlled mutation |
| `filesystem.replace`, `filesystem.insert`, `filesystem.delete_range`, `filesystem.patch`, `filesystem.apply_diff`, `filesystem.rollback` | `src/mcp/dispatcher.rs`, `src/services/edit.rs`, `src/services/authorization.rs`, `src/services/snapshot.rs` | `tests/acceptance_editing.rs`, `tests/acceptance_edit_gates.rs`, `tests/rollback_recovery_boundaries.rs`, `tests/snapshot_provenance_boundaries.rs`, `tests/mcp_policy_gate.rs`, `tests/trust_wedge_e2e.rs` | Canonical mutation/recovery path. MCP schemas do **not** expose `ExpectedState`; stale-state protection is only fully expressible where the interface supplies it | **Keep/improve P0**; close expected-state gap before unattended concurrent editing |
| `memory.store`, `memory.search`, `memory.get`, `memory.update`, `memory.delete` | `src/mcp/memory.rs`, `src/mcp/dispatcher.rs`, `src/core/memory.rs` | `tests/memory_boundaries.rs`, `tests/mcp_server.rs`, `tests/store_convergence.rs` | Scoped persistence and validation, lock/atomic writes, audit | **Keep P1** |
| `tasks.create`, `tasks.list`, `tasks.get`, `tasks.update`, `tasks.delete` | `src/mcp/tasks.rs`, `src/mcp/dispatcher.rs`, `src/core/tasks.rs` | `tests/mcp_server.rs`, `tests/tasks_cli.rs`, `tests/store_convergence.rs` | Task validation and workspace-scoped atomic persistence | **Keep P1; hide from minimal quickstart** |
| `context.assemble`, `context.get`, `context.insert`, `context.offload`, `context.optimize`, `context.protect`, `context.remove`, `context.restore`, `context.search`, `context.status`, `context.unprotect` | `src/mcp/context.rs`, `src/mcp/dispatcher.rs`, `src/context/engine.rs`, `src/core/context.rs` | `tests/context_engine_boundaries.rs`, `tests/mcp_server.rs`, `tests/store_convergence.rs` | Scope/budget limits, canonical context records, atomic state transitions, audit | **Keep P1** |
| `skills.add`, `skills.disable`, `skills.enable`, `skills.list`, `skills.read`, `skills.remove`, `skills.search` | `src/mcp/skills.rs`, `src/mcp/dispatcher.rs`, `src/skills/registry.rs`, `src/skills/project.rs` | `tests/skills_boundaries.rs`, `tests/mcp_dynamic_tools.rs` | Explicit project reference/enablement, package integrity at install, scoped refs; enablement does not bypass other tool authorization | **Hide from default; defer** |
| `git.status`, `git.diff`, `git.log`, `git.branch`, `git.stage`, `git.unstage`, `git.commit` | `src/mcp/dispatcher.rs`, `src/services/git.rs`, Git CLI | `tests/mcp_builtin_tool_gate.rs`, `tests/git_boundaries.rs`, `tests/git_failure_injection.rs` | Git process execution, argument separation, timeout/output limits; mutations require explicit authority and are not sandboxed | **Keep read-only inspection P1; hide/defer index/history mutation** |
| `terminal.run` | `src/mcp/dispatcher.rs`, `src/services/terminal.rs` | `tests/mcp_builtin_tool_gate.rs`, `tests/mcp_policy_gate.rs` | Direct argv, timeout/output caps, audit, policy gate; no OS sandbox | **Hide from default; defer** |
| `connector.composio_accounts`, `connector.composio_link`, `connector.composio_register`, `connector.composio_remove`, `connector.invoke`, `connector.providers`, `connector.tools` | `src/mcp/dispatcher.rs`, `src/mcp/composio.rs`, `src/mcp/composio_auth.rs`, `src/mcp/composio_registry.rs`, `src/mcp/providers.rs`, `src/mcp/tool_broker.rs` | `tests/mcp_builtin_tool_gate.rs`, `tests/mcp_dynamic_tools.rs`, `tests/mcp_agent_routes.rs`, `tests/mcp_sandbox.rs` | Provider registration/invocation crosses network, trust, permission, and audit boundaries; remote data is untrusted | **Hide from default; defer** |
| `connectors.add`, `connectors.disable`, `connectors.enable`, `connectors.get`, `connectors.list`, `connectors.remove` | `src/mcp/connectors.rs`, `src/mcp/dispatcher.rs`, connector metadata store | `src/mcp/connectors.rs` unit tests, `tests/connector_audit.rs`, `tests/mcp_server.rs` | Project-scoped connector metadata and enablement; this is not itself provider invocation authority | **Hide from default; defer** |
| `mcp.status` | `src/mcp/dispatcher.rs`, `src/mcp/server.rs`, registry state | `tests/mcp_server.rs`, `tests/mcp_executable.rs` | Read-only registry/transport status; no registration or grant side effect | **Keep P1 diagnostic** |
| `github.checks_status`, `github.issue_comment`, `github.issue_create`, `github.issue_get`, `github.issue_list`, `github.pr_create`, `github.pr_get`, `github.pr_list`, `github.pr_merge`, `github.pr_review`, `github.release_create`, `github.workflow_dispatch` | `src/mcp/github.rs`, `src/mcp/dispatcher.rs`, GitHub HTTP client/token configuration | `src/mcp/github.rs` unit tests, `tests/mcp_builtin_tool_gate.rs`, `tests/mcp_protocol.rs` | Optional token-scoped remote authority; tool presence does not imply repository write authority. Network and secret handling are outside the minimal local loop | **Hide from default; defer** |

The three `workspace.write_file`, `workspace.delete_file`, and `terminal.run`
entries in `src/mcp/tool_broker.rs` are resource-scoped dispatcher prechecks.
They are deliberately not expanded to look like the nine-tool policy catalog.
The edit service performs the canonical filesystem authorization.

## 5. MCP resources

Resources are read-only. The list cardinality is data-dependent; these are URI
templates/concrete resource namespaces, not a claim that every ID exists.

| Resource | Implementation and dependencies | Test evidence | Security boundary / status | Disposition |
|---|---|---|---|---|
| `awh://context` | `src/mcp/dispatcher.rs`, workspace context reader | `tests/mcp_protocol.rs`, `tests/mcp_server.rs` | Read-only canonical workspace context; no secret registry dump | **Keep P1 inspection** |
| `awh://memory/{id}` | `src/mcp/dispatcher.rs`, `MemoryStore` | `tests/mcp_protocol.rs`, memory boundary tests | Single validated memory ID; rejects separators, traversal, percent-encoding, controls, and overlong URIs | **Keep P1** |
| `awh://skills/{name}` | `src/mcp/dispatcher.rs`, enabled project skills | `tests/mcp_protocol.rs`, skills boundary tests | Enabled, project-scoped named skill only; same URI validation | **Hide from default; defer** |

## 6. Control API routes

`src/api/control.rs` is the authoritative router. All routes are nested under
`/api/v1`; all except `GET /api/v1/healthz` require bearer authentication before
rate limiting. Shared layers include request timeout, body limit, and rate
limiting. Dependencies are `axum`, `tokio`, `tower`/`tower-http`, and the
canonical services/stores. Route/auth/validation tests are inline in
`src/api/control.rs`.

There are **26 authenticated route paths plus one public health path**. Multiple
methods on one path are listed together.

| Route and methods | Implementation / test evidence | Security boundary / status | Disposition |
|---|---|---|---|
| `GET /healthz` | `health`; health test | Public liveness, no secrets | Keep infrastructure; hide server from default |
| `GET /status` | `status`; route tests | Authenticated workspace status | Hide/defer |
| `GET, POST /projects` | `list_projects`, `create_project`; validation/auth tests | Authenticated project enumeration/creation; not agent identity | Hide/defer |
| `GET, DELETE /projects/{name}` | `project`, `delete_project`; validation tests | Name validation and authenticated destructive operation | Hide/defer |
| `GET /files` | `list_files`; file validation tests | Authenticated workspace enumeration | Hide/defer |
| `GET, PUT /files/content` | `read_file`, `write_file`; tests | Authenticated full-file I/O. PUT is not the controlled `EditService` path | Hide/defer; do not use for trust-wedge edit proof |
| `GET /files/search` | `search_files`; tests | Authenticated bounded workspace search | Hide/defer |
| `GET /files/meta` | `file_meta`; tests | Authenticated metadata inspection | Hide/defer |
| `POST, PUT, DELETE /files/entry` | `rename_entry` (POST), `create_dir` (PUT), `delete_entry` (DELETE); validation tests | Authenticated direct filesystem mutation, separate from snapshot editing | Hide/defer |
| `GET /git/status`, `/git/log`, `/git/diff`, `/git/branch`, `/git/branches` | Git handlers; route tests | Authenticated local Git inspection | Hide/defer |
| `POST /git/push`, `/git/pull`, `/git/stage`, `/git/unstage`, `/git/commit` | Git mutation handlers; route tests | Authenticated Git/remote mutation; not an OS sandbox | Hide/defer |
| `POST /terminal/run` | `run_command`; validation/auth tests | Authenticated command execution with limits/audit, no OS sandbox | Hide/defer; high risk |
| `GET, PUT /context` | `read_context`, `write_context`; tests | Authenticated context state mutation | Hide/defer |
| `GET, POST /memory` | `list_memory`, `append_memory`; tests | Authenticated memory list/append | Hide/defer |
| `GET /skills` | `list_skills`; tests | Authenticated installed-skill enumeration | Hide/defer |
| `GET, POST, DELETE /skills/project` | Project-skill handlers; tests | Authenticated project reference mutation | Hide/defer |
| `GET /mcp` | `list_mcp`; tests | Authenticated MCP registry inspection | Hide/defer |
| `GET /audit` | `audit`; tests | Authenticated audit inspection | Keep as supporting evidence, hide server by default |
| `GET /logs` | `logs`; tests | Authenticated operational log inspection; avoid secret exposure | Hide/defer |

### MCP transport HTTP routes (separate server)

Implemented in `src/mcp/http.rs`, with coverage in `tests/mcp_http.rs`,
`tests/sec_002_public_mcp_tls_guard.rs`, `tests/mcp_agent_routes.rs`, and the
SDK SSE harness.
These routes are not part of the Control API's `/api/v1` namespace.

| Route | Boundary / status | Disposition |
|---|---|---|
| `GET /health` | Unauthenticated liveness, no secrets | Keep with MCP server |
| `GET /sse` | Authenticated isolated SSE session; advertises `/mcp` endpoint | Keep after stdio proof |
| `POST /mcp?sessionId=…` | Authenticated JSON-RPC submit bound to an existing SSE session | Keep after stdio proof |
| `GET /{agent}/sse` | Auth plus canonical active-agent route resolution | Keep P1; defer from minimal quickstart |
| `POST /{agent}/mcp?sessionId=…` | Re-resolves agent and verifies session↔agent binding before dispatch | Keep P1; defer from minimal quickstart |

## 7. Cargo features

`Cargo.toml` declares **no `[features]` section and no optional Cargo feature
flags**. There is one `awh` binary and one library target. Optional behavior is
runtime/configuration-dependent (GitHub token, remote transport, connector
configuration, tunnels), not Cargo-feature-dependent.

| Feature surface | Evidence | Disposition |
|---|---|---|
| Default build | Sole implicit build configuration; all non-runtime-optional core modules compile into the same targets | Keep; do not invent feature flags as part of this ticket |
| Optional GitHub tools | Runtime registry behavior, not a Cargo feature | Hide/defer through configuration and docs |
| TLS/HTTP/connectors/tunnel | Runtime options/dependencies, not independently selectable Cargo features | Hide/defer through configuration and docs |

## 8. Smallest verified fixes and remaining core gaps

### Fixed in source by this review

1. **Policy CLI parity:** `awh policy deny` now accepts the same nine tools as
   the canonical store because the store performs the only validation. This
   removes the drifting check; it does not duplicate edit dispatch or broaden
   MCP resource prechecks.
2. **Regression at the real boundary:** the new real-binary test asserts that
   a CLI-created edit denial reaches `EditService`, exits 3, changes no target
   bytes, writes no edit/recovery records, and becomes allowed after removing
   only the matching rule.
3. **Doctor acceptance specification:** the new suite makes read-only behavior
   executable across the complete requested matrix rather than relying on help
   text or source inspection.

These fixes still require compilation and execution; they are not yet a passed
runtime gate.

### Verified limitations, not silently reclassified as passes

- **No current end-to-end run:** initialization, doctor, real SDK client,
  controlled edit, inspection, and rollback have not been executed together on
  this revision in this environment.
- **MCP expected-state gap:** `EditService` and the CLI support expected-state
  guards, but current MCP edit schemas do not expose `ExpectedState`. Existing
  tests explicitly limit stale-state assertions to interfaces where the guard
  is expressible. This should be improved before advertising unattended or
  concurrent agent editing; it is not necessary to invent a second edit engine.
- **`status` is shallow:** it is not a substitute for doctor, audit, snapshot,
  or provenance inspection.
- **Direct file/API writes are separate paths:** `workspace.write_file` and
  Control API file mutation must not be presented as equivalent to controlled
  `filesystem.*` editing and rollback.
- **Terminal and Git mutation are not sandboxed:** keep them outside the
  beginner path even when authenticated.
- **Historical client evidence is bounded:** official SDK harness results do
  not prove OpenCode/Codex compatibility or current-head behavior.

## 9. Next actions, in order

1. Run `cargo fmt --all -- --check`, clippy, build, and all tests in a usable
   Rust environment. Treat any failure as blocking; do not weaken the new
   assertions to obtain a green result.
2. Run `cargo test --test doctor_cli --test policy_edit_cli` against the real
   binary and preserve the actual results. If doctor violates read-only behavior,
   fix that specific path before continuing.
3. Execute the official SDK stdio harness on the current build, then perform a
   serial core loop: initialize → doctor → inspect → denied edit (zero target
   and edit-state mutation) → allowed edit → diff/evidence inspection →
   rollback → verify restored bytes and provenance. Repeat over SSE only after
   stdio passes.
4. Add MCP expected-state inputs through the existing `EditService` contract if
   concurrent editing is in scope, with stale-state and no-partial-mutation
   tests. Do not fork the editing implementation.
5. Update the quickstart to show only the proven core path. Keep optional
   connectors, GitHub, skills, collaboration, terminal, tunnels, TUI, and
   Control API discoverable as explicit advanced surfaces.
6. Reassess removal only after usage and compatibility evidence exists. This
   inventory recommends **no deletion now** and no new infrastructure phase.

Until steps 1–3 pass, the trust wedge remains **blocked**, not partially green.
