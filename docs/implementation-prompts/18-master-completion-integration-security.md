# Prompt 18 — AWH Master Completion: Integration, Testing, Security & Release Readiness

## Mission

Complete the implementation phase of Agent Workspace Hub (AWH) after the individual implementation prompts have been implemented.

This is the **master completion prompt**. It does not introduce another product feature and does not replace the existing implementation contracts. Its purpose is to determine what is still incomplete in the actual Rust implementation and then close the remaining gaps across:

- cross-feature integration;
- architectural convergence;
- end-to-end behavior;
- functional and failure-path testing;
- concurrency and recovery testing;
- security and threat-model verification;
- interface consistency;
- persistence/migration validation;
- performance/resource controls;
- documentation accuracy;
- release-readiness.

Treat the current `rust` branch as the source of truth.

The individual implementation prompts are **contracts that have already been implemented**. Do not re-implement them blindly. Inspect the actual code, tests, and interfaces first. Where an implementation is partial, duplicated, inconsistent, insufficiently tested, or inconsistent with its contract, converge and complete it.

The goal is:

```
implemented prompts
      ↓
actual repository audit
      ↓
integration/convergence
      ↓
complete test coverage
      ↓
security verification
      ↓
recovery/concurrency verification
      ↓
release-readiness
```

Do not claim AWH is complete merely because all prompt files exist or because individual unit tests pass.

---

## 1. Scope

This prompt owns the remaining work required to establish that the implemented AWH system works as one coherent runtime.

It covers:

1. repository-wide implementation audit;
2. contract-to-code verification;
3. service/store convergence;
4. cross-feature integration;
5. end-to-end tests;
6. adversarial/security tests;
7. authorization and identity propagation tests;
8. persistence and restart/recovery tests;
9. concurrency/race testing;
10. interface convergence across CLI/MCP/TUI/Control API;
11. Git/worktree integration;
12. context/memory/skills/tasks/terminal/collaboration/remote/connectors integration where implemented;
13. resource and timeout controls;
14. CI verification;
15. documentation consistency;
16. release-readiness evidence;
17. final implementation gap report.

This prompt does **not** authorize:

- a new agent orchestration system;
- a new model/provider router;
- a second identity system;
- a second session system;
- a second authorization engine;
- a second task system;
- a second memory system;
- a second audit store;
- a second Git/worktree engine;
- a rewrite of working services without evidence;
- speculative features not required by existing contracts.

---

## 2. Repository source of truth

Work only from the current `rust` branch.

Before modifying code, inspect:

- `docs/FEATURES.md`;
- `docs/PROJECT_CONTEXT.md`;
- `docs/architecture.md`;
- `docs/security.md`;
- `docs/threat-model.md`;
- `docs/roadmap/GROWTH_STRATEGY.md`;
- `docs/roadmap/PROJECT_ROADMAP.md`;
- `docs/implementation-prompts/README.md`;
- implementation prompts 01–17;
- the feature implementation prompts under:
  - `docs/context/`
  - `docs/memory/`
  - `docs/skills/`
  - `docs/tasks/`
  - `docs/terminal/`
  - `docs/tui/`
  - `docs/collaboration/`
  - `docs/control-api/`
  - `docs/remote/`
  - `docs/connectors/`
  - `docs/distribution/`
  - `docs/advanced-infrastructure/`

Inspect the actual source tree, including at minimum:

- `src/main.rs`;
- `src/lib.rs`;
- `src/core/`;
- `src/services/`;
- `src/mcp/`;
- `src/context/`;
- `src/models/`;
- `src/skills/`;
- `src/api/`;
- `src/tui/`;
- CLI command modules;
- persistence/state modules;
- Git/worktree code;
- authorization and identity code;
- audit code;
- existing tests;
- CI workflows.

Search the complete repository for duplicate implementations, TODOs, stubs, unreachable production paths, direct persistence writes, direct Git invocation, direct filesystem mutation, and interface-specific business logic.

---

## 3. Required audit before implementation

Create an internal implementation matrix before making changes.

For every implemented prompt and feature family, classify:

| Area | Contract | Code | Tests | Integration | Security | Recovery | Status |
|---|---|---|---|---|---|---|---|
| Init/runtime | required | verify | verify | verify | verify | verify | |
| Agent identity/profile/registry/session | required | verify | verify | verify | verify | verify | |
| MCP routing | required | verify | verify | verify | verify | verify | |
| Authorization/policy | required | verify | verify | verify | verify | verify | |
| Edit transaction/engine/safety | required | verify | verify | verify | verify | verify | |
| Snapshots/provenance | required | verify | verify | verify | verify | verify | |
| Rollback/recovery | required | verify | verify | verify | verify | verify | |
| Persistent audit | required | verify | verify | verify | verify | verify | |
| MCP editing/client validation | required | verify | verify | verify | verify | verify | |
| CLI | required | verify | verify | verify | verify | verify | |
| Worktree isolation | required | verify | verify | verify | verify | verify | |
| Filesystem coordination | required | verify | verify | verify | verify | verify | |
| Service/store convergence | required | verify | verify | verify | verify | verify | |
| Testing/acceptance | required | verify | verify | verify | verify | verify | |
| Context | required | verify | verify | verify | verify | verify | |
| Memory | required | verify | verify | verify | verify | verify | |
| Skills | required | verify | verify | verify | verify | verify | |
| Tasks | required | verify | verify | verify | verify | verify | |
| Terminal | required | verify | verify | verify | verify | verify | |
| TUI | required | verify | verify | verify | verify | verify | |
| Collaboration | required | verify | verify | verify | verify | verify | |
| Control API | required | verify | verify | verify | verify | verify | |
| Remote | required | verify | verify | verify | verify | verify | |
| Connectors | required | verify | verify | verify | verify | verify | |
| Distribution | required | verify | verify | verify | verify | verify | |
| Advanced infrastructure | required | verify | verify | verify | verify | verify | |

Use these statuses only:

- **PASS** — implementation and evidence satisfy the contract;
- **PARTIAL** — implementation exists but one or more contract requirements remain;
- **DUPLICATED** — more than one competing authority exists;
- **UNTESTED** — implementation appears present but lacks adequate evidence;
- **INTEGRATION-GAP** — isolated implementation works but does not converge through the canonical runtime;
- **SECURITY-GAP** — security invariant is missing or unproven;
- **RECOVERY-GAP** — restart/crash/corruption behavior is incomplete;
- **MISSING** — required implementation is absent.

Do not infer PASS from the existence of source files.

---

## 4. Canonical architecture to enforce

The completed runtime should converge conceptually to:

```
CLI ────────┐
MCP ────────┤
TUI ────────┤
Control API ┤
Connectors ─┤
Remote ─────┘
       ↓
Canonical AWH runtime/services
       ↓
Identity + Session
       ↓
Authorization + Policy
       ↓
Canonical domain services
       ↓
Canonical persistence / Git / filesystem
       ↓
Canonical audit
```

Domain services include, where implemented:

```
Edit
Snapshot
Rollback
Worktree
Context
Memory
Skills
Tasks
Terminal
Collaboration
Connectors
```

Interfaces must remain thin adapters.

Do not allow an interface to silently become a second business-logic implementation.

---

## 5. Architecture convergence

Search for and resolve competing authorities.

There must be one authoritative owner for each of:

- workspace identity;
- agent identity;
- agent registry;
- agent session;
- authorization;
- capability/policy evaluation;
- edit transactions;
- file mutation;
- snapshots;
- rollback;
- audit;
- Git execution;
- worktree lifecycle;
- context state;
- memory;
- skills;
- tasks;
- terminal execution;
- collaboration lifecycle;
- connector authorization.

Known historical convergence risks must be explicitly investigated, including multiple implementations of:

- memory;
- tasks;
- skills;
- Git/worktree behavior;
- session/task state;
- MCP/domain models.

If multiple implementations exist:

1. identify the canonical owner;
2. migrate/adapt callers;
3. remove or isolate the competing authority;
4. add regression tests;
5. document any intentionally retained compatibility adapter.

Do not solve duplication by simply renaming classes.

---

## 6. Identity propagation invariant

Every consequential operation must preserve the relevant execution identity.

The effective flow is:

```
caller
  ↓
transport/interface
  ↓
AgentIdentity
  ↓
AgentSession
  ↓
Workspace
  ↓
Capability/Policy
  ↓
domain operation
  ↓
persistence/Git/filesystem
  ↓
audit
```

Verify that identity is not reconstructed from:

- URL paths;
- filenames;
- branch names;
- user-controlled IDs;
- arbitrary headers;
- filesystem locations.

Unknown, disabled, expired, mismatched, or unauthorized identities must fail closed.

---

## 7. Authorization integration

Use the existing canonical authorization boundary.

Verify authorization occurs before consequential mutation for:

- edits;
- snapshots;
- rollback;
- worktree create/remove;
- Git mutation;
- terminal execution;
- task mutation;
- skill installation/use where capability-sensitive;
- connector operations;
- collaboration operations;
- remote operations;
- administrative Control API operations.

A path, WorktreeId, task ID, skill name, connector name, or URL namespace must never become an authorization mechanism.

Test both:

- direct service calls;
- calls through public interfaces.

Direct internal calls must not silently bypass authorization.

---

## 8. End-to-end Trust Wedge verification

Prove the complete core lifecycle:

```
awh init
  ↓
AgentProfile
  ↓
AgentRegistry
  ↓
AgentSession
  ↓
agent-scoped MCP
  ↓
capability/policy authorization
  ↓
EditTransaction
  ↓
Snapshot / provenance
  ↓
Edit
  ↓
Audit
  ↓
Rollback
  ↓
Worktree isolation
```

The test must exercise actual persisted state and real filesystem boundaries.

Do not replace the complete test with mocked service calls.

Verify both success and denial paths.

---

## 9. Cross-feature integration

Test important interactions rather than only individual modules.

At minimum verify:

### Identity × authorization
An authorized agent can perform permitted operations; an unauthorized agent cannot.

### Identity × worktree
A session resolves only its own effective worktree.

### Worktree × filesystem
File operations remain inside the assigned root.

### Worktree × edit
EditService operates against the session's effective root.

### Worktree × snapshot
Snapshots record the correct workspace/worktree provenance.

### Worktree × rollback
Rollback never escapes the assigned worktree and preserves rollback authorization.

### Edit × audit
Successful and failed consequential edits produce canonical audit outcomes.

### Authorization × audit
Denied operations are recorded without leaking secrets.

### Context × memory
Context retrieval must not silently become persistent memory mutation.

### Skills × authorization
Skill declarations/requirements cannot grant themselves capabilities.

### Tasks × sessions
Task lifecycle must remain distinct from execution/session lifecycle while preserving correct ownership.

### Terminal × authorization
Terminal execution must use the canonical policy boundary and resource controls.

### Collaboration × tasks/sessions/worktrees
Collaboration must reuse existing task/session/worktree authorities rather than creating replacements.

### Control API × canonical services
API endpoints must call canonical services rather than duplicate business logic.

### TUI × canonical services
TUI actions must produce the same state transitions as service/CLI calls.

### MCP × canonical services
MCP tool calls must use the same authorization and service boundaries.

### Connectors × audit
Consequential connector operations must be attributable and redacted.

### Remote × local runtime
Remote access must not create a second identity or authorization system.

---

## 10. Required test strategy

Use layered tests.

### Layer A — Unit tests

Test:

- pure models;
- validation;
- path containment;
- state transitions;
- authorization decisions;
- serialization;
- parsing;
- deterministic error mapping;
- capability evaluation.

Unit tests must not be used as the only evidence for filesystem/Git/security behavior.

### Layer B — Service integration tests

Use temporary isolated workspaces and repositories.

Test:

- real persistence;
- real filesystem operations;
- real Git repositories;
- real lifecycle transitions;
- audit writes;
- restart/reload;
- corruption handling.

### Layer C — Interface integration tests

Where interfaces exist, test:

- CLI → canonical service;
- MCP → canonical service;
- Control API → canonical service;
- TUI action → canonical service;
- connector → canonical authorization/audit boundary.

### Layer D — End-to-end tests

Build realistic workflows using multiple AWH components.

At least one complete workflow must cover:

```
init
→ agent registration
→ session
→ worktree
→ authorization
→ edit
→ snapshot
→ audit
→ rollback
→ cleanup
```

### Layer E — Adversarial tests

Treat all externally supplied:

- paths;
- IDs;
- branch names;
- refs;
- URLs;
- connector parameters;
- terminal arguments;
- task identifiers;
- skill names;

as untrusted.

---

## 11. Security test matrix

Required tests include:

### Filesystem

- `../`;
- absolute paths;
- encoded traversal;
- mixed separators;
- symlink escape;
- symlink replacement;
- sibling-prefix confusion;
- workspace-root targeting;
- cross-worktree access;
- cross-workspace access;
- race between validation and mutation.

### Git

- arguments beginning with `-`;
- malformed branch/ref names;
- shell metacharacters;
- unexpected repository paths;
- branch collision;
- concurrent worktree creation;
- removal of modified worktree;
- attempted destructive cleanup.

### Authorization

- unknown agent;
- disabled agent;
- invalid session;
- expired session;
- wrong workspace;
- wrong worktree;
- missing capability;
- denied policy;
- direct service bypass;
- interface spoofing.

### MCP

- unauthenticated request;
- invalid bearer;
- wrong agent route;
- wrong workspace;
- hidden consequential tool;
- tools/list filtering;
- invocation after session termination;
- URL namespace manipulation.

### Terminal

- command injection;
- unsafe environment propagation;
- timeout;
- output limits;
- process-tree cleanup;
- working-directory escape;
- unauthorized execution.

### Connectors

- unauthorized connector;
- secret exposure;
- malicious connector output;
- invalid remote target;
- audit redaction.

### Persistence

- truncated state;
- malformed JSON;
- unknown schema;
- partial write;
- stale record;
- concurrent update;
- corruption during restart.

---

## 12. Crash and recovery testing

Inject failures at critical boundaries.

At minimum:

```
before mutation
during mutation
after mutation before persistence
after persistence before audit
during audit
during cleanup
during snapshot
during rollback
during worktree creation
during worktree removal
```

After restart, verify:

- no false success;
- no fabricated active state;
- no lost ownership;
- no unauthorized adoption;
- no duplicate resources;
- recovery-required state when ambiguity exists;
- deterministic reconciliation.

Never convert an ambiguous state into success merely to simplify recovery.

---

## 13. Concurrency testing

Test concurrent operations involving:

- two agents editing the same file;
- two agents creating worktrees;
- simultaneous snapshot/rollback;
- simultaneous task updates;
- concurrent audit writes;
- session termination during an operation;
- restart during persistence;
- concurrent Control API and MCP calls.

Verify:

- no data corruption;
- no lost updates;
- no cross-agent ownership;
- no duplicate active resources;
- deterministic conflict reporting;
- atomic state publication.

Use the smallest existing synchronization mechanism required.

Do not introduce distributed locking unless the repository contract explicitly requires it.

---

## 14. Persistence and migration

For every durable store:

1. identify the canonical owner;
2. identify schema/version;
3. verify atomic writes;
4. verify corruption handling;
5. verify restart behavior;
6. verify migration behavior where versions exist;
7. verify retention/cleanup policy;
8. verify bounded record size;
9. verify secrets are excluded.

Test upgrade scenarios from supported prior formats where applicable.

A failed migration must fail closed and preserve recoverable evidence.

Do not silently discard old state.

---

## 15. Audit verification

Use only the canonical audit system.

Verify audit coverage for:

- authentication/identity failures where applicable;
- authorization denial;
- edit;
- snapshot;
- rollback;
- worktree lifecycle;
- terminal execution;
- task mutation;
- skill installation/security-sensitive use;
- connector operations;
- collaboration state changes;
- administrative operations.

Verify:

- stable event identity;
- timestamp/order semantics;
- correlation IDs;
- agent/session/workspace identity;
- success/failure outcome;
- redaction;
- restart durability;
- bounded queries.

Never record:

- API keys;
- bearer tokens;
- passwords;
- private keys;
- unnecessary environment variables;
- full file contents;
- raw secrets returned by connectors.

---

## 16. Interface consistency

For equivalent operations, CLI, MCP, TUI, and Control API must agree on:

- authorization;
- identity;
- state transitions;
- errors;
- persistence;
- audit;
- resource ownership.

Differences in presentation are allowed.

Differences in security semantics are not.

If an interface exposes an operation that has no canonical service behind it, either wire it correctly or remove/disable the unsupported surface. Do not implement a parallel business path merely to make the interface appear complete.

---

## 17. Resource and performance controls

Verify bounded behavior for:

- request size;
- file size;
- patch size;
- context size;
- memory records;
- task records;
- audit queries;
- terminal output;
- process execution time;
- Git command duration;
- connector responses;
- Control API payloads;
- MCP payloads.

Prevent unbounded loops, recursive directory traversal, unbounded subprocess output, and uncontrolled persistence growth.

Use existing timeout/limit mechanisms where present.

Do not optimize prematurely. Fix correctness and safety first.

---

## 18. Static and repository-wide quality audit

Search for:

- `TODO`;
- `FIXME`;
- `unwrap()`;
- `expect()`;
- `panic!`;
- `unsafe`;
- `Command::new`;
- shell invocation;
- direct filesystem writes;
- direct writes to canonical persistence files;
- duplicate model definitions;
- duplicate service names;
- dead feature flags;
- placeholder implementations;
- test-only bypasses accidentally compiled into production;
- disabled security checks.

Every finding must be classified as:

- safe/intentional;
- test-only;
- bounded and documented;
- required fix.

Do not mechanically remove every `unwrap()`; fix findings based on actual failure semantics.

---

## 19. CI verification

The implementation must pass:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Also run:

- repository-specific integration tests;
- security tests;
- Git integration tests;
- persistence/recovery tests;
- interface tests;
- any existing CI workflow checks.

Do not weaken CI, disable tests, remove warnings, or relax security checks merely to obtain green CI.

If a test is platform-dependent, document the platform and expected behavior rather than hiding the test.

---

## 20. Documentation verification

Compare implementation against:

- `docs/FEATURES.md`;
- architecture documentation;
- security documentation;
- threat model;
- roadmap;
- implementation prompt contracts.

Correct documentation that falsely claims unsupported behavior.

Do not mark a roadmap item complete without implementation evidence.

Do not document:

- distributed orchestration;
- automatic conflict resolution;
- universal sandboxing;
- remote hosting;
- model intelligence;
- unsupported Git operations;

as complete unless actually implemented and tested.

---

## 21. Release-readiness criteria

A component is release-ready only when:

- implementation exists;
- canonical ownership is established;
- public interfaces are wired;
- success path is tested;
- denial path is tested;
- persistence is tested;
- restart/recovery is tested where stateful;
- concurrency is tested where shared state exists;
- security invariants are tested;
- audit behavior is verified;
- resource limits are verified;
- documentation matches reality;
- CI passes.

The complete system is release-ready only when no critical or high-severity integration/security/recovery gap remains.

Do not use a numeric score to hide unresolved critical failures.

---

## 22. Required implementation order

Execute in this order:

1. **Repository forensics**
2. **Prompt-to-code audit**
3. **Duplicate-authority/convergence audit**
4. **Identity/authorization propagation audit**
5. **Core Trust Wedge end-to-end integration**
6. **Cross-feature integration**
7. **Persistence/recovery testing**
8. **Concurrency testing**
9. **Security/adversarial testing**
10. **Interface convergence**
11. **Resource/performance verification**
12. **CI/static quality audit**
13. **Documentation consistency**
14. **Release-readiness audit**
15. **Fix remaining gaps**
16. **Run the complete verification suite again**
17. **Produce final evidence report**

Do not begin with cosmetic UI or documentation changes while core integration/security gaps remain.

---

## 23. Change-control rules

Every code change must be justified by one of:

- closing an identified contract gap;
- fixing an integration failure;
- fixing a security failure;
- fixing a recovery/concurrency failure;
- removing a duplicate authority;
- adding required regression evidence;
- correcting an inaccurate public interface;
- enforcing a documented resource limit.

Do not add unrelated features.

Do not refactor stable code solely for style.

Do not change external agent/provider integrations unless required to complete an AWH runtime contract.

Do not modify implementation-prompt contracts merely to make an implementation pass. If a contract is genuinely contradictory or obsolete, document the discrepancy and handle it as a separate contract decision rather than silently rewriting requirements.

---

## 24. Required final acceptance matrix

Produce a final matrix:

| Requirement | Implementation evidence | Test evidence | Security evidence | Recovery evidence | Interface evidence | Status |
|---|---|---|---|---|---|---|
| Runtime foundation | | | | | | |
| Agent identity/session | | | | | | |
| MCP routing | | | | | | |
| Authorization | | | | | | |
| Editing | | | | | | |
| Snapshots | | | | | | |
| Rollback | | | | | | |
| Audit | | | | | | |
| Worktrees | | | | | | |
| Filesystem safety | | | | | | |
| Context | | | | | | |
| Memory | | | | | | |
| Skills | | | | | | |
| Tasks | | | | | | |
| Terminal | | | | | | |
| Collaboration | | | | | | |
| Control API | | | | | | |
| TUI | | | | | | |
| Connectors | | | | | | |
| Remote | | | | | | |
| Distribution | | | | | | |
| Advanced infrastructure | | | | | | |

Every status must be evidence-based.

Allowed final statuses:

- **COMPLETE**
- **COMPLETE WITH DOCUMENTED LIMITATION**
- **REQUIRES FIX**
- **NOT IMPLEMENTED**

---

## 25. Completion definition

The master completion work is complete only when:

1. all implemented prompt contracts have been audited against actual code;
2. all critical domain services have one canonical authority;
3. no unresolved critical duplicate implementation remains;
4. identity and authorization propagate correctly;
5. the Trust Wedge works end-to-end;
6. cross-feature integration tests pass;
7. real filesystem/Git integration tests pass;
8. adversarial security tests pass;
9. crash/restart/recovery tests pass for stateful components;
10. concurrency tests pass for shared state;
11. CLI/MCP/TUI/Control API converge on canonical services;
12. persistent stores are corruption-safe and migration-safe where applicable;
13. audit events are complete and redacted;
14. resource limits are enforced;
15. CI verification passes;
16. documentation matches the actual implementation;
17. no critical/high unresolved security or integration gap remains;
18. known platform limitations are explicitly documented;
19. no unrelated implementation prompt or feature is modified;
20. the final report distinguishes verified behavior from assumptions and limitations.

---

## 26. Non-goals

Completion of this prompt does **not** automatically mean:

- AWH has an intelligent agent;
- AWH selects models;
- AWH replaces Claude Code/Codex/OpenCode;
- AWH provides unrestricted sandboxing;
- AWH provides distributed orchestration;
- AWH provides automatic merge conflict resolution;
- AWH provides remote Git hosting;
- every roadmap feature is production-ready;
- every external connector is available;
- every OS has identical capabilities.

AWH remains an agent-agnostic runtime and workspace boundary.

---

## 27. Final implementation report

The final report must contain:

### A. Audit
- prompts audited;
- feature areas audited;
- repository commit/ref used;
- implementation matrix.

### B. Convergence
- duplicate authorities found;
- canonical owners selected;
- migrations/adapters performed;
- remaining compatibility layers.

### C. Integration
- end-to-end workflows tested;
- cross-feature interactions verified;
- interface convergence verified.

### D. Testing
- unit tests;
- integration tests;
- end-to-end tests;
- concurrency tests;
- adversarial tests;
- crash/recovery tests;
- exact commands and results.

### E. Security
- threat-model checks;
- authorization checks;
- filesystem checks;
- Git checks;
- MCP checks;
- terminal checks;
- connector checks;
- persistence checks;
- audit redaction checks.

### F. Performance/resource controls
- limits verified;
- timeout behavior;
- bounded persistence;
- subprocess/resource controls.

### G. Documentation
- documents checked;
- inaccurate claims corrected;
- known limitations.

### H. Remaining gaps
List every unresolved item with:

- severity;
- affected component;
- exact contract;
- current behavior;
- required fix;
- blocking/non-blocking status.

### I. Verification
Report exact results of:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

### J. Changed files
List every changed file and why it changed.

### K. Final declaration
State explicitly:

- whether the implementation is complete;
- what remains;
- what was verified;
- what is platform-dependent;
- whether any critical/high security or integration gap remains;
- whether duplicate authorities remain;
- whether unrelated files/prompts were changed.

Never state “complete” when evidence is missing.

---

## 28. Standalone execution rule

This master prompt is independently executable against the current `rust` branch.

It must not depend on:

- a future implementation prompt;
- a future feature prompt;
- an unmerged branch;
- a specific previous PR;
- an external agent;
- a model provider;
- undocumented manual state.

Reuse existing contracts and implementations where present.

If a required dependency is absent, record it as a concrete implementation gap and implement only the minimum boundary necessary to complete the current master-completion scope.

The objective is not to make the repository appear complete.

The objective is to produce **verifiable evidence that the implemented AWH runtime works coherently, securely, recoverably, and consistently across its supported interfaces.**
