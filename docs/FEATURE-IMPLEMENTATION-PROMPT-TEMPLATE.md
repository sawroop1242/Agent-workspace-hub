# AWH Feature Implementation Prompt Template

> Reusable template for standalone implementation prompts under `docs/<feature>/`.
> Replace bracketed placeholders with feature-specific, repository-grounded content.

---

## Prompt identity

**Prompt:** [PROMPT NUMBER / FEATURE ID]  
**Feature:** [FEATURE NAME]  
**Primary roadmap area:** [ROADMAP PHASE / FEATURE FAMILY]  
**Target branch:** `rust`  
**Canonical folder:** `docs/[feature-folder]/`

### Mission

Implement and verify **[FEATURE]** as a production-ready AWH subsystem.

The prompt must be standalone:

- Do not require another prompt or PR to be implemented first.
- Inspect the current `rust` branch before changing code.
- Reuse existing repository contracts.
- Do not create duplicate services, models, stores, authorization boundaries, or transport implementations.
- Keep the feature inside AWH's documented product boundary.

---

# 1. Scope

## In scope

Explicitly list what this prompt owns:

- [Capability 1]
- [Capability 2]
- [Capability 3]
- [State/persistence owned by this feature]
- [Interfaces owned by this feature]

## Out of scope

Explicitly prevent scope expansion:

- [Unrelated capability]
- [Another subsystem's responsibility]
- [Future/optional functionality]
- [Duplicate implementation]

## Product boundary

State what AWH owns for this feature and what remains owned by external agents or other subsystems.

---

# 2. Required repository forensics

Before modifying anything, inspect the current repository and relevant documentation.

At minimum:

```text
Cargo.toml
README.md
AGENTS.md
docs/PROJECT_CONTEXT.md
docs/FEATURES.md
docs/architecture.md
docs/security.md
docs/threat-model.md
docs/roadmap/PROJECT_ROADMAP.md
docs/roadmap/GROWTH_STRATEGY.md
```

Inspect:

```text
src/[relevant modules]
tests/[relevant tests]
```

Search for existing implementations before adding anything:

```bash
rg -n "[existing symbols / commands / routes / stores]"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\\(|expect\\("
```

Document the result:

| Existing contract | Location | Action | Reason |
|---|---|---|---|
| [contract] | `src/...` | Reuse / Extend / Replace | [reason] |

**Rule:** missing prompt coverage does not imply missing Rust implementation.

---

# 3. Architecture

## Canonical responsibility

Define exactly what this subsystem owns.

```text
[input]
   |
   v
[validation]
   |
   v
[canonical feature service]
   |
   +--> [existing policy/authorization]
   +--> [existing persistence/state]
   +--> [existing audit/observability]
   |
   v
[stable result]
```

Adapt the diagram to the feature.

## Architectural invariants

The implementation must preserve:

1. One canonical service boundary.
2. One canonical data model.
3. One authoritative persistence mechanism where persistence is required.
4. Existing workspace/agent/session identity boundaries.
5. Existing capability/policy enforcement.
6. Existing audit/provenance semantics.
7. Existing error conventions.
8. Existing concurrency/locking primitives.
9. No interface-specific duplicate business logic.

## State and lifecycle

Define:

```text
Created -> Active -> [state] -> [terminal state]
```

Specify valid/invalid transitions, restart behavior, corruption behavior, cleanup, recovery, and concurrency semantics.

---

# 4. Interfaces

Define every public interface affected by the feature.

## Rust/service interface

Specify:

- canonical structs/enums;
- service methods;
- inputs/outputs;
- structured errors;
- serialization;
- versioning/compatibility.

## CLI interface

If applicable:

```text
awh [feature] [command] [arguments]
```

Specify commands, arguments, validation, stdout, stderr, exit codes, and machine-readable output.

## Control API interface

If applicable, specify:

- endpoint and method;
- request/response schema;
- authentication/authorization;
- error schema;
- idempotency;
- concurrency behavior.

## MCP interface

If applicable, specify:

- tool/resource names;
- schemas;
- discovery;
- authorization;
- agent/session identity;
- errors;
- cancellation/timeouts.

## TUI interface

If applicable, specify:

- screen/view;
- actions;
- service/backend calls;
- refresh/state handling;
- error display;
- no duplicated business logic.

---

# 5. Security

Security requirements are implementation requirements.

## Authorization

Define:

- required identity;
- required capability;
- applicable policy;
- authorization order;
- fail-closed behavior;
- direct/internal-call behavior.

A route, command, tool name, or internal caller must never grant authority by itself.

## Input/resource safety

Define protections for:

- path traversal and encoded traversal;
- symlink/resource escape;
- invalid identifiers;
- malformed input;
- oversized input;
- resource exhaustion;
- unsafe filesystem/resource access.

## Isolation

Specify boundaries for:

- workspace;
- agent;
- session;
- task;
- worktree;
- project/tenant, where applicable.

Cross-scope access must be rejected unless an explicit existing contract permits it.

## Secrets

Specify:

- sensitive fields;
- redaction;
- persistence restrictions;
- log/audit restrictions;
- error-message restrictions.

## Threat cases

At minimum test:

- unauthorized caller;
- forged/mismatched identity;
- stale state;
- malformed persisted state;
- concurrent mutation;
- resource/path escape;
- replay/repeated request;
- partial failure;
- crash/restart;
- resource exhaustion.

---

# 6. Persistence and recovery

If the feature persists state, define:

- canonical storage location;
- schema/version;
- serialization;
- atomic publication;
- locking/concurrency;
- corruption detection;
- restart loading;
- migration;
- cleanup/retention;
- recovery.

A partially written or corrupted state must not be accepted as valid.

If persistence belongs to another subsystem, explicitly name that subsystem as authoritative.

---

# 7. Integration boundaries

Define how this feature integrates with existing AWH services.

| Boundary | Existing authority | Feature responsibility |
|---|---|---|
| Identity/session | [service] | [integration] |
| Capability/policy | [service] | [integration] |
| Filesystem | [service] | [integration] |
| Snapshot/recovery | [service] | [integration] |
| Audit | [service] | [integration] |
| MCP | [service] | [integration] |
| Git/worktree | [service] | [integration] |

### No-duplication rule

Do not introduce a second identity system, authorization system, policy engine, filesystem safety layer, audit store, snapshot store, edit engine, Git boundary, or other existing canonical subsystem.

Use adapters at interface boundaries.

---

# 8. Testing

Testing must prove the contract, not merely increase line coverage.

## Unit tests

Cover:

- valid inputs;
- invalid inputs;
- state transitions;
- deterministic behavior;
- structured errors;
- serialization/deserialization.

## Integration tests

Use real service boundaries and, where relevant:

- real workspace state;
- real persistence;
- real authorization/policy;
- real transport;
- restart/reload.

## Security/adversarial tests

Cover:

- unauthorized access;
- isolation violations;
- traversal/escape;
- malformed state;
- secret leakage;
- concurrent access;
- replay/idempotency;
- resource limits.

## Failure/recovery tests

Cover:

- crash during persistence;
- partial write;
- corruption;
- stale state;
- interrupted operation;
- restart;
- repeated operation.

## Compatibility tests

Where applicable verify interoperability with:

- CLI;
- MCP clients;
- Control API;
- TUI backend;
- existing persisted state;
- documented schemas.

## Verification gates

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Add feature-specific verification commands as required.

---

# 9. Rollout

## Compatibility

Define:

- backward compatibility;
- persisted-state compatibility;
- CLI/API compatibility;
- migration;
- feature flags/configuration, if applicable.

## Rollout order

Use a concrete sequence:

1. [foundation/service change]
2. [persistence/schema change]
3. [interface integration]
4. [security enforcement]
5. [tests]
6. [documentation/status update]

Do not require another prompt/PR to be merged first.

## Failure behavior

Define behavior when:

- configuration is missing;
- persisted state is incompatible;
- authorization fails;
- an integration is unavailable;
- migration fails;
- recovery cannot be guaranteed.

Consequential operations should fail closed.

## Observability

Specify structured logs, audit events, correlation IDs, metrics if already supported, and secret redaction.

---

# 10. Acceptance criteria

Implementation is complete only when all applicable criteria are satisfied.

### Functional

- [ ] [Feature behavior 1 works]
- [ ] [Feature behavior 2 works]
- [ ] [Feature behavior 3 works]

### Architecture

- [ ] One canonical implementation exists.
- [ ] Existing contracts are reused where applicable.
- [ ] No duplicate business-logic path exists.
- [ ] Interface adapters do not create alternate semantics.

### Security

- [ ] Authorization is enforced at the correct boundary.
- [ ] Invalid identity/scope is rejected.
- [ ] Resource/path isolation is enforced.
- [ ] Secrets are redacted and never persisted in prohibited locations.
- [ ] Consequential failures are fail-closed.

### Persistence/recovery

- [ ] Required state survives restart.
- [ ] Corruption is detected.
- [ ] Partial writes cannot be accepted as valid.
- [ ] Concurrency behavior is deterministic.
- [ ] Recovery semantics are tested.

### Testing

- [ ] Unit tests pass.
- [ ] Integration tests pass.
- [ ] Security/adversarial tests pass.
- [ ] Failure/recovery tests pass.
- [ ] Repository verification passes.

### Interfaces

- [ ] CLI contract is verified, if applicable.
- [ ] MCP contract is verified, if applicable.
- [ ] Control API contract is verified, if applicable.
- [ ] TUI uses the canonical service/backend, if applicable.

### Documentation

- [ ] Feature contract is documented.
- [ ] Roadmap/status documents are updated only where required.
- [ ] Planned functionality is not represented as implemented without verification.

---

# 11. Explicit non-goals

This prompt must **not** implement:

- [Non-goal 1]
- [Non-goal 2]
- [Non-goal 3]

Do not expand into model routing, autonomous orchestration, generic workflow scheduling, or unrelated infrastructure unless explicitly required by this feature contract.

---

# 12. Final implementation report

Before completion, report:

```text
Feature:
Canonical implementation:
Files changed:
Existing contracts reused:
Interfaces added/changed:
Security controls:
Persistence/recovery:
Tests added:
Verification results:
Known limitations:
```

---

## Standalone execution rule

This prompt must be executable independently against the current `rust` branch.

A developer should be able to:

1. read this prompt;
2. inspect the current repository;
3. identify existing implementations;
4. implement only this feature's missing/incomplete contract;
5. run the required tests;
6. verify the acceptance criteria;

without needing another feature prompt, another PR, or undocumented assumptions.
