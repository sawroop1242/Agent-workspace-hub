# TW-008 — Complete Trust-Wedge Acceptance Suite

## Master implementation prompt

Build the executable end-to-end acceptance suite for the Trust Wedge. This issue is standalone: it must test whatever is actually present on the current `rust` branch and must not assume another unmerged issue/PR exists.

The suite validates behavior; it must not replace missing production implementation with mocks.

### 1. Repository forensics

Inspect:

- current Trust-Wedge service boundaries;
- CLI entry points;
- MCP transport/tool execution;
- agent/profile/session models;
- capability/policy authorization;
- EditService;
- file snapshots/provenance;
- rollback/recovery;
- persistent audit;
- existing unit/integration tests and test utilities.

Map existing behavior before adding tests. Reuse production services and existing test helpers where they preserve real invariants.

### 2. Canonical end-to-end scenario

Create an executable scenario equivalent to:

`awh init -> agent/profile/session -> agent-scoped MCP -> discovery -> authorization -> read -> safe edit -> expected-state validation -> snapshot -> apply -> verify -> provenance/audit -> rollback -> verify -> restart -> reconstruct`

Adapt individual command names to the current repository, but preserve the security and state-transition semantics.

The final state must be proven from actual persisted/runtime state, not merely from returned success values.

### 3. Mandatory happy path

At minimum demonstrate:

1. fresh isolated workspace initialization;
2. valid agent registration/profile;
3. valid active session bound to agent/workspace;
4. agent-scoped MCP route;
5. authorized tool discovery/read;
6. authorized edit through the canonical EditService;
7. exact pre-edit snapshot/provenance;
8. post-edit verification;
9. persistent audit linkage;
10. conflict-safe rollback;
11. exact restoration of original bytes;
12. restart/reload;
13. reconstruction of the relevant lifecycle from durable state.

### 4. Mandatory denial matrix

Test at least:

| Scenario | Required result |
|---|---|
| unknown agent | rejected before tool execution |
| inactive/disabled agent | rejected |
| missing capability | rejected |
| expired capability | rejected |
| scope mismatch | rejected |
| wrong workspace | rejected |
| policy denial | rejected even with a grant |
| invalid/stopped session | rejected |
| unauthorized edit | zero protected mutation |
| unauthorized rollback | zero protected mutation |

Every consequential denial must assert actual protected state is unchanged.

### 5. Mandatory recovery/conflict matrix

Cover:

- malformed/corrupt authoritative state;
- stale expected edit state;
- external file modification before rollback;
- missing snapshot;
- corrupt snapshot;
- verification failure/failure injection where supported;
- lock/contention behavior where supported;
- restart between lifecycle stages;
- repeated rollback/recovery calls;
- audit persistence failure according to the repository's documented policy.

Rejected recovery must be non-destructive.

### 6. Mandatory multi-agent isolation

Use at least two agents and two sessions.

Prove:

- distinct identity;
- distinct authorization context;
- agent A cannot use agent B's session;
- agent A cannot use agent B's workspace scope;
- one agent's route/capability cannot silently authorize another agent;
- audit/provenance retains the correct caller identity.

### 7. Persistence and restart

Tests must exercise real durable state.

At least one scenario must:

1. perform a meaningful lifecycle action;
2. terminate/recreate the runtime or persistence instance;
3. reload state;
4. continue or inspect the lifecycle;
5. verify stable IDs and durable records.

Corrupt or incomplete authoritative state must produce explicit failure/recovery behavior, not silently create a fresh workspace/history.

### 8. Test quality rules

Prefer:

- real temporary directories/files;
- production services;
- real persistence;
- real CLI/MCP boundaries where practical;
- deterministic fixtures;
- byte/hash comparisons for filesystem recovery;
- explicit assertions on side effects.

Avoid:

- mocks that bypass authorization;
- fake file systems for safety-critical tests unless specifically testing an adapter;
- assertions that only inspect status strings;
- duplicating every service unit test in the acceptance suite.

Keep focused unit tests in their owning modules and reserve this suite for cross-boundary behavior.

### 9. Security gates

The acceptance suite must prove:

- authentication/identity resolution precedes consequential authorization;
- authorization precedes mutation/tool execution;
- denied operations cause zero protected mutation;
- route names do not grant permission;
- workspace boundaries hold;
- multi-agent isolation holds;
- corrupt/stale state fails safely;
- rollback never silently overwrites newer external state;
- sensitive file contents/secrets are not emitted through ordinary audit/errors.

### 10. Required repository checks

Run the repository's applicable checks, including where supported:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo test --all-targets`
- relevant `cargo clippy` checks
- relevant dependency/security audit checks
- real CLI smoke tests
- real MCP/SSE smoke tests where supported.

Do not report a check as passed if it was skipped; record the reason.

### 11. Definition of done

The acceptance issue is complete only when executable tests demonstrate:

- initialization;
- stable agent/session identity;
- agent-scoped routing;
- capability/policy authorization;
- safe edit;
- snapshot/provenance;
- conflict-aware rollback;
- durable audit;
- persistence/restart;
- multi-agent isolation;
- denial with zero mutation;
- recovery without destructive overwrite.

All tests must pass against production implementations, and the suite must fail if a Trust-Wedge invariant is removed.

Final report must include changed test files, scenarios covered, exact commands and results, skipped checks with reasons, known limitations, and evidence that assertions inspect real state.

### Non-goals

Do not implement missing production behavior inside tests, weaken assertions to make the suite pass, add external-agent execution infrastructure, or perform unrelated refactoring.