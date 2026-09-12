# AWE-001 — Canonical Edit Transaction Model

**GitHub issue:** #22  
**Milestone:** Agent-Grade Editing  
**Priority:** P0  
**Status:** Implementation prompt

## Master implementation prompt

You are implementing **AWE-001 / GitHub Issue #22: Design the canonical Edit Transaction model** in the AWH Rust repository.

The goal is to establish the transport-independent domain model that every later edit operation will use. This is foundational infrastructure for AWH's Agent-Grade Editing milestone.

### 1. First inspect the repository

Before changing code, inspect:

- `AGENTS.md`
- `Cargo.toml`
- `src/services/mod.rs`
- `src/services/files.rs`
- existing error/domain/model conventions
- existing serialization conventions
- existing IDs and state-machine types
- tests related to filesystem/workspace/security

Do not assume the architecture. Reuse existing abstractions wherever appropriate.

### 2. Scope

Implement the canonical edit domain model, preferably in `src/services/edit.rs`, and register it through `src/services/mod.rs`.

The model must be **transport-independent** and usable by MCP, CLI, TUI, Control API, and future internal services.

Do **not** implement actual file mutations, MCP tools, CLI commands, TUI screens, snapshots, audit events, rollback execution, Git operations, or agent reasoning in this issue.

### 3. Required domain types

Implement the following canonical types, adapting naming to existing AWH conventions where necessary.

#### `EditId`

A strongly typed identifier for an edit transaction.

Requirements:

- newtype rather than a raw `String`
- serializable/deserializable with `serde`
- cloneable/equatable/hashable as appropriate
- safe and deterministic formatting
- easy to use in logs, API responses, audit records, and rollback references
- generate unique IDs without introducing unnecessary dependencies

#### `EditOperation`

Represent the intended mutation without performing it.

Required variants:

- `Replace`
- `Insert`
- `DeleteRange`
- `Patch`
- `ApplyDiff`

Design payloads from the existing AWH filesystem model and future issues AWE-002 through AWE-005. Do not over-design fields that are not needed yet.

#### `ExpectedState`

Represent the state the caller expects before mutation.

Support optional validation information such as:

- expected SHA-256
- expected size
- expected line count
- contextual content / expected context where justified

The model must make stale-state detection possible without silently overwriting newer file contents.

#### `FileState`

Represent the observed state of a file. At minimum:

- relative path
- SHA-256 hash
- size
- line count

Reuse existing hashing/file-state helpers if AWH already provides them. Do not create duplicate security or filesystem implementations unnecessarily.

#### `EditStatus`

Represent the edit lifecycle.

Required successful lifecycle states:

- `Requested`
- `Authorized`
- `Located`
- `Validated`
- `Snapshotted`
- `Applied`
- `Verified`
- `Committed`

Required failure/recovery states:

- `Rejected`
- `Conflict`
- `ValidationFailed`
- `ApplyFailed`
- `VerificationFailed`
- `RolledBack`

Keep the model suitable for future state-transition validation without prematurely implementing the complete edit state machine.

#### `EditTransaction`

Create the canonical transaction object containing, at minimum:

- `edit_id`
- `operation`
- `expected_state`
- `status`

Include `created_at`, `before_state`, or `after_state` only if existing AWH conventions make them appropriate and they do not couple the model unnecessarily to snapshots/audit/session infrastructure.

Future issues must be able to extend this model with provenance, snapshot, agent, session, workspace, and rollback references without replacing the canonical model.

#### `EditError`

Create structured domain errors following existing AWH error conventions.

Errors should distinguish structural validation failures from future runtime failures such as invalid edit ID, invalid operation, missing/invalid path information, invalid line range, missing expected state, and invalid transaction state.

Do not use `anyhow` as the domain error abstraction if the repository already has structured error conventions.

### 4. Validation responsibilities

AWE-001 should perform **structural/domain validation only**.

Validate things such as:

- edit transaction contains a valid operation
- paths are represented consistently
- line ranges are structurally valid
- required operation fields are present
- transaction state is internally coherent
- operation ordering/data structures are deterministic

Do not duplicate `FilesService` workspace-root, traversal, symlink, or filesystem security logic. Actual path authorization/security remains with the existing filesystem/capability/policy infrastructure and later issues.

### 5. Serialization

All public domain types needed by transports should use `serde` consistently with the rest of AWH.

Test JSON serialization, JSON deserialization, round-trip equality where appropriate, and stable representation of IDs/status/operations.

The model must remain independent of MCP-specific request/response types.

### 6. Tests

Add focused unit tests for:

1. `EditId` creation and equality
2. ID serialization/deserialization
3. every `EditOperation` variant
4. `ExpectedState` serialization and optional fields
5. `FileState` serialization
6. every `EditStatus` variant
7. `EditTransaction` construction
8. transaction validation
9. invalid operation/transaction cases
10. JSON round trips
11. deterministic operation representation/order
12. future rollback/provenance compatibility assumptions

Tests must test behavior and invariants, not implementation details that unnecessarily constrain future design.

### 7. Required invariants

The implementation must preserve these architectural invariants:

- There is one canonical edit transaction model.
- MCP, CLI, TUI, and Control API must be able to consume the same model later.
- The model must not perform filesystem mutation.
- The model must support stale-state detection in later edit services.
- The model must support atomic multi-operation edits later.
- The model must support rollback references later.
- The model must support snapshot/provenance/audit integration later.
- The model must not contain LLM/agent reasoning.
- The model must not become an agent framework abstraction.
- The model must not duplicate filesystem sandbox/path-security logic.
- Operation ordering must be deterministic.
- No design decision should force later issues to rewrite the canonical model unnecessarily.

### 8. Reuse existing AWH infrastructure

Before implementing helpers, inspect whether the repository already has SHA-256 hashing, file metadata/state calculation, structured errors, timestamps, ID generation, path normalization, serialization helpers, and service-layer conventions.

Prefer existing infrastructure over duplicate implementations.

`FilesService` remains the owner of filesystem security and actual filesystem access. The edit model should describe and validate an edit transaction, not replace `FilesService`.

### 9. Keep the change narrow

Do not solve future issues inside AWE-001.

Specifically do not implement:

- `filesystem.patch`
- `filesystem.replace`
- `filesystem.insert`
- `filesystem.delete_range`
- `filesystem.apply_diff`
- `filesystem.rollback`
- atomic file writes
- conflict detection against live files
- snapshots
- provenance persistence
- audit events
- capability/policy enforcement
- MCP registration
- CLI commands
- TUI commands
- Git integration

Those belong to later issues in the Agent-Grade Editing dependency chain.

### 10. Verification

Run all applicable repository checks:

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

If any command fails:

1. inspect the actual failure,
2. fix only issues relevant to this change,
3. rerun the failed checks,
4. do not claim completion while required checks remain failing.

Also inspect the final diff to ensure the implementation is scoped to AWE-001.

### 11. Commit

Recommended commit message:

```text
feat(edit): add canonical edit transaction model
```

### 12. Final implementation report

After implementation, report:

- files changed
- new domain types
- validation rules
- serialization behavior
- tests added
- commands executed and their actual results
- existing AWH abstractions reused
- architecture decisions
- unresolved issues, if any
- explicit confirmation that no actual filesystem mutation was implemented in AWE-001

Do not claim tests or checks passed unless they were actually executed.

## Definition of Done

AWE-001 is complete only when:

- the canonical edit model exists in the service/domain layer,
- all required types are implemented,
- serialization works,
- structural validation works,
- unit tests cover the required invariants,
- existing AWH abstractions are reused where appropriate,
- no future issue is prematurely implemented,
- the project builds cleanly,
- formatting/checks/tests/lint pass as applicable,
- the final diff is narrow and reviewable.

The resulting model becomes the stable foundation for **AWE-002: Safe Contextual Replacement** and all subsequent Agent-Grade Editing issues.
