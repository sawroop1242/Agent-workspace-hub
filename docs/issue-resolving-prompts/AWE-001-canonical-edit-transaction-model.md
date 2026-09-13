# AWE-001 / #22 — Canonical Edit Transaction Model — Full Issue-Resolution Master Prompt

## Mission

Implement and verify the canonical, transport-independent edit transaction domain model for Agent Workspace Hub (AWH).

This is the **first issue in the agent-grade editing sequence**. The implementation must establish the stable domain contract that AWE-002, AWE-003, AWE-004, snapshots, provenance, rollback, audit, MCP, CLI, TUI, Control API, and policy enforcement can consume later.

Do not solve later issues inside AWE-001. Do not build a second edit system. Do not recreate types that already exist. Inspect the current Rust implementation first, reconcile it with this prompt and GitHub issue #22, then make the smallest complete change that establishes an unambiguous canonical model.

---

## 1. Source of truth and architectural context

Read these documents before changing code:

```text
docs/PROJECT_CONTEXT.md
docs/PROJECT_ROADMAP.md
docs/PROJECT_ROADMAP_STATUS.md
docs/PROJECT_STATUS.md
docs/FEATURES.md
docs/CLI.md
docs/architecture.md
docs/mcp.md
```

The relevant architectural rules are:

- `rust` is the active/canonical engineering track.
- AWH is an agent-agnostic, local-first workspace runtime, not an agent framework.
- CLI, MCP, TUI, and Control API are interfaces over shared application/domain services.
- Agent-grade editing must eventually follow a controlled transaction flow rather than whole-file blind rewrites.
- The intended edit flow is conceptually:

```text
request
  -> identity
  -> capability/policy authorization
  -> locate/read
  -> expected-state/context validation
  -> conflict detection
  -> atomic apply
  -> verification
  -> snapshot/provenance
  -> audit
```

AWE-001 defines only the **domain transaction model and reusable validation/state primitives**. It must not implement that entire runtime pipeline.

The roadmap explicitly identifies agent-grade editing as a P0 capability and requires all editing interfaces to share one canonical edit service/model rather than developing separate semantics per transport.

---

## 2. Current forensic reality

The current `src/services/edit.rs` already contains the AWE-001 vocabulary:

- `EditId`
- `EditOperation`
- `ExpectedState`
- `FileState`
- `EditStatus`
- `EditTransaction`
- `EditError`
- path/hash/state helper functions

The current implementation is **not the completed AWE-001 contract**.

Known gaps that must be verified against the actual branch before coding:

1. `EditTransaction` currently does not carry caller identity fields such as `agent_id`, `session_id`, and `workspace_id`.
2. `ExpectedState.context` exists but is not included in `FileState::matches()`, so its semantics are currently decorative rather than enforced.
3. Snapshot/provenance/audit/policy references are not yet represented as a clean optional domain-level reference contract.
4. Lifecycle statuses exist, but their legal meaning and deterministic transition expectations need to be made explicit without turning this module into an executor.
5. There are currently no production callers constructing/processing `EditTransaction`; do not mistake the existence of the types for an integrated mutation pipeline.
6. Existing filesystem APIs and later edit implementations must remain compatible.

These facts are based on the current repository/issue evidence and must still be confirmed by inspecting the branch before modification.

---

## 3. Non-negotiable architectural constraints

### 3.1 One canonical model

There must be exactly one canonical edit transaction vocabulary for AWH.

Do not create parallel models such as:

```text
McpEditRequest
CliEditRequest
TuiEditRequest
ApiEditRequest
FilesystemEditTransaction
```

Transport adapters may have request/response DTOs later, but they must map into this canonical domain model rather than redefine its semantics.

### 3.2 Transport independence

The model must not depend on:

- MCP types
- Axum
- CLI/Clap
- TUI framework
- HTTP request types
- connector-specific types
- storage implementations
- filesystem executors
- Git implementations
- database types

The domain model may use normal Rust/Serde/error primitives already accepted by the project.

### 3.3 No mutation executor in AWE-001

Do not implement:

- actual file writes
- atomic file replacement
- diff application
- rollback execution
- snapshots
- provenance persistence
- audit persistence
- MCP tools
- CLI commands
- policy authorization logic
- worktree management
- Git operations

Only provide reusable domain structures and deterministic validation/state semantics required by later milestones.

### 3.4 Preserve security invariants

Never weaken existing path validation, fail-closed behavior, serialization validation, or error handling merely to make the model easier to use.

Do not add a new filesystem/path-security implementation if an existing project primitive can be reused or referenced by later service code.

### 3.5 Minimal, coherent patch

Prefer a focused implementation over speculative architecture.

Do not introduce a large dependency, generic framework, database layer, event bus, or capability engine for this issue.

---

## 4. Required canonical model

The final model must be capable of representing all of the following without transport-specific extensions.

### 4.1 Transaction identity

Every transaction must have a stable `EditId`.

The model must support caller/runtime identity where available:

```text
agent_id
session_id
workspace_id
```

Use the repository's existing identity types if appropriate. If those types are not suitable or do not exist at this layer, use a small transport-independent representation consistent with the existing domain model.

Identity fields should be optional where the transaction may legitimately originate before a complete runtime session exists. Do not invent fake identities.

The identity data must be serializable and inspectable so future audit/provenance layers can associate an edit with its caller.

### 4.2 Target resource

The transaction must represent its target resource/path through the operation model or a canonical target abstraction.

Keep path/resource semantics transport-independent.

For current filesystem editing, relative workspace paths are the expected representation. Do not introduce absolute host paths as a new public contract.

### 4.3 Edit operations

The canonical operation enum must represent:

```text
Replace
Insert
DeleteRange
Patch
ApplyDiff
```

Preserve the existing operation fields unless inspection demonstrates that they are incorrect or incapable of satisfying the issue.

Do not implement the actual semantics of applying these operations in AWE-001.

`ApplyDiff` may remain a domain representation of a future unified-diff operation; parsing/application belongs to its later milestone.

### 4.4 Expected state

`ExpectedState` must represent optional preconditions such as:

```text
hash
context
size
line_count
```

The important requirement is that these fields have **defined semantics**.

An expected-state field must never exist merely because it sounds useful.

---

## 5. Define exact ExpectedState matching semantics

This is the most important correction in AWE-001.

The existing `ExpectedState.context` must receive a precise contract.

A recommended contract, subject to repository evidence and tests, is:

- `hash`, when supplied, must equal the complete current file-content SHA-256.
- `size`, when supplied, must equal the current file size in bytes.
- `line_count`, when supplied, must equal the current line count according to the canonical project line-count helper.
- `context`, when supplied, must be present in the **current content at the intended edit location**, not merely somewhere unrelated in the file.

Do not implement a vague global `contains(context)` check if the operation requires a location-specific match.

The model itself should define the data contract needed to express this requirement. Actual filesystem reading/location resolution belongs to the later EditService.

If the current model cannot express location-specific context matching without coupling the domain model to filesystem logic, document that boundary explicitly and provide the smallest reusable representation needed by later services.

### Required conflict semantics

Later services must be able to distinguish at least:

```text
expected state matched
expected state conflicted
expected state malformed
```

Do not silently treat an unmet expectation as a successful edit.

Do not silently overwrite stale state.

---

## 6. FileState contract

`FileState` must represent the observed state of a file/resource at a transaction boundary.

At minimum retain:

```text
path
hash
size
line_count
```

It should remain immutable in meaning: it describes an observed state, not an instruction to mutate a file.

`FileState` matching must be deterministic.

Tests must prove matching and non-matching cases for each supported expected-state component.

If context matching requires content that is intentionally not part of `FileState`, do not store the entire file content merely for convenience. Keep the domain boundary clear and let the service layer perform location/content checks.

---

## 7. Lifecycle/status contract

The transaction must expose an explicit lifecycle status.

The existing status vocabulary may be retained and refined, but each status must have a clear meaning.

The conceptual lifecycle is:

```text
Requested
   -> Authorized
   -> Located
   -> Validated
   -> Snapshotted
   -> Applied
   -> Verified
   -> Committed
```

Failure/recovery states must be distinguishable, including the existing concepts such as:

```text
Rejected
Conflict
ValidationFailed
ApplyFailed
VerificationFailed
RolledBack
```

Do not add arbitrary states just to make the enum larger.

AWE-001 does not need to implement a complete state-machine executor, but the model must not make impossible lifecycle interpretations ambiguous.

If transition validation is added, it must remain a pure domain operation with no I/O or external side effects.

Document whether status is:

- a requested lifecycle state,
- an observed execution state,
- or a combination.

The preferred design is that the transaction model records lifecycle state while execution services remain responsible for performing transitions.

---

## 8. Snapshot / provenance / audit / policy references

The issue requires optional references for future cross-system correlation.

Represent these as lightweight domain references only.

The model may contain optional identifiers/references such as:

```text
snapshot_id
provenance_id
audit_id / audit_event_id
policy_decision_id
```

Do not import or embed snapshot/audit database structures into `services/edit.rs`.

Do not make AWE-001 depend on the implementation of later phases.

The goal is correlation, not ownership of those systems.

If the repository already has canonical ID types, reuse them. Otherwise use transport-independent identifiers consistent with the existing project conventions.

---

## 9. Validation requirements

`EditTransaction` must have deterministic structural validation.

At minimum validate:

- transaction is not empty
- expected-state cardinality is valid
- operation-specific required fields are valid
- paths are valid according to the existing canonical relative-path rules
- line ranges are valid
- replacement/patch match text is not empty when empty matching is invalid
- unified diff payload is not empty
- identity/reference data is structurally valid if the chosen representation requires validation

Validation must be side-effect free.

It must never perform filesystem mutation.

Do not duplicate downstream filesystem containment checks if the existing service layer already owns them. Structural validation and security/path resolution are related but not identical responsibilities.

---

## 10. Structured errors

Use the existing structured `EditError` approach where appropriate.

Errors must be:

- deterministic
- machine-readable
- meaningful to callers
- serializable if required by the existing architecture
- free of secrets or sensitive file contents

Do not replace typed errors with strings merely to simplify implementation.

Do not create an enormous error hierarchy for future phases.

Only introduce categories needed to distinguish actual AWE-001 validation/state failures.

---

## 11. Serialization contract

The model is shared across interfaces, so serialization compatibility matters.

Verify Serde behavior for:

- `EditId`
- every `EditOperation` variant
- `ExpectedState`
- `FileState`
- `EditStatus`
- `EditTransaction`
- any new identity/reference types
- relevant `EditError` representation if it is intentionally serialized

Use explicit serde attributes where needed to keep the wire representation stable and unambiguous.

Do not make serialization dependent on MCP JSON-RPC structures.

Add round-trip tests for representative transactions.

Include at least:

1. one single-operation transaction
2. one multi-operation transaction
3. expected state populated
4. caller identity populated
5. optional references populated
6. every operation variant serialized/deserialized successfully

---

## 12. Backward compatibility

Before modifying public fields or serialization:

1. inspect existing callers
2. inspect tests
3. inspect any documentation/examples using the model
4. inspect the current Rust branch for serialization assumptions

Because the forensic report indicates no production caller currently constructs `EditTransaction`, compatibility risk may be low, but this must be verified rather than assumed.

Do not break unrelated filesystem APIs.

Do not modify MCP/CLI/TUI/API behavior merely to prove that the model exists.

If a compatibility concern is discovered, prefer a minimal migration that preserves the canonical model rather than introducing a second model.

---

## 13. Implementation workflow

Follow this exact sequence.

### Step 1 — Repository inspection

Inspect:

```text
src/services/edit.rs
src/services/files.rs
src/services/
existing identity/domain models
existing error definitions
existing tests
```

Search for:

```text
EditTransaction
EditOperation
ExpectedState
FileState
EditStatus
EditError
```

Also search for any existing edit/patch/replace/insert/delete/apply-diff abstractions.

Do not assume the prompt is newer than the source code.

### Step 2 — Identify duplication

Before creating any type, search the repository for an equivalent.

If an existing type is suitable, extend it rather than creating another one.

### Step 3 — Write down the contract

Before coding, establish the exact semantics for:

- identity
- operation list
- expected-state matching
- file state
- lifecycle status
- optional correlation references
- structural validation
- serialization

Keep the contract small enough for AWE-002 and AWE-003 to consume directly.

### Step 4 — Implement only the domain model

Modify the canonical module.

Keep mutation/execution logic out.

### Step 5 — Add focused tests

Tests must cover both valid and invalid behavior.

### Step 6 — Run repository gates

Run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

If the project has a documented additional test/security command relevant to the changed module, run it too.

### Step 7 — Inspect the diff

Run:

```bash
git diff --check
git diff -- src/services/edit.rs
```

Confirm no unrelated files or architectural behavior were changed.

### Step 8 — Verify downstream readiness

Confirm that AWE-002 and AWE-003 can reference the same:

```text
EditTransaction
EditOperation
ExpectedState
FileState
EditStatus
EditError
```

without recreating those concepts.

Do not implement AWE-002/AWE-003 while doing this verification.

---

## 14. Required test matrix

### Transaction construction

- empty transaction rejected
- one operation accepted
- multiple operations accepted
- stable unique `EditId`

### Operation validation

For every operation:

- valid input accepted
- invalid path rejected
- malformed operation rejected
- invalid line range rejected where applicable
- empty match rejected where prohibited
- empty diff rejected

### Expected state

Test independently:

- hash matches
- hash conflicts
- size matches
- size conflicts
- line count matches
- line count conflicts
- context contract is represented and tested according to its defined semantics
- multiple expectations combine deterministically
- no expectation means no expectation check

### Identity

Test:

- no identity
- agent-only identity
- session/workspace identity
- complete identity
- serialization round-trip

### References

Test:

- no optional references
- individual references
- all references
- serialization round-trip

### Lifecycle

Test:

- initial state is deterministic
- success-state values serialize correctly
- failure/conflict states serialize correctly
- any implemented pure transition validation rejects invalid transitions

### Serialization

Round-trip every operation variant and a representative complete transaction.

### Regression

Ensure all existing tests continue to pass.

---

## 15. What NOT to implement in AWE-001

Do not add any of the following unless an existing compile/test dependency absolutely requires a tiny compatibility change:

```text
filesystem.patch executor
filesystem.replace executor
filesystem.insert executor
filesystem.delete_range executor
filesystem.apply_diff executor
filesystem.rollback
snapshot creation
snapshot restore
provenance storage
persistent audit storage
PolicyEngine
CapabilityEngine
MCP tools
CLI commands
TUI screens
Control API endpoints
Git/worktree integration
agent orchestration
remote editing
full-file editor replacement
```

Those belong to later milestones.

AWE-001 is successful when it gives all of them a stable domain contract.

---

## 16. Failure handling

If compilation, tests, Clippy, formatting, or diff validation fails:

1. diagnose the actual failure
2. fix only what is necessary for AWE-001
3. rerun the failed gate
4. rerun the complete relevant gate set
5. do not mark the issue complete while any required gate is failing

If a discovered architectural conflict cannot be safely resolved without changing later milestones, stop and report the conflict rather than silently inventing a second architecture.

If an existing field/type conflicts with this prompt, prefer repository evidence and canonical architecture over speculative redesign. Document the decision in the final implementation report.

---

## 17. Definition of done

AWE-001 is complete only when all of these are true:

- [ ] One canonical transport-independent `EditTransaction` model exists.
- [ ] `EditId` is stable and serializable.
- [ ] Caller identity can be carried where available: `agent_id`, `session_id`, `workspace_id`.
- [ ] All required edit operation variants exist.
- [ ] Target/resource representation is unambiguous.
- [ ] `ExpectedState` has explicit, documented matching semantics.
- [ ] `ExpectedState.context` is not decorative; its role is defined and covered by tests.
- [ ] `FileState` has deterministic hash/size/line-count semantics.
- [ ] Lifecycle status is explicit and deterministic.
- [ ] Structured edit errors remain typed and meaningful.
- [ ] Optional snapshot/provenance/audit/policy references can be correlated without coupling the model to storage.
- [ ] Structural validation is side-effect free.
- [ ] Serialization round-trip tests exist.
- [ ] Malformed/incomplete transaction tests exist.
- [ ] No production filesystem mutation executor was added to AWE-001.
- [ ] No duplicate edit transaction model was introduced elsewhere.
- [ ] Existing security/path invariants remain intact.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo check --all-targets` passes.
- [ ] `cargo test --all-targets` passes.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes.
- [ ] `git diff --check` passes.
- [ ] The final diff contains only changes justified by AWE-001.
- [ ] AWE-002 and AWE-003 can consume the canonical model without duplicating identity/state/error concepts.

---

## 18. Final implementation report required from the coding agent

At the end of the task, report exactly:

### Changed

List the files changed and summarize the domain-model changes.

### Contract decisions

State the exact semantics chosen for:

- caller identity
- `ExpectedState.context`
- `FileState`
- lifecycle statuses
- optional correlation references

### Tests

List the new/updated tests and their purpose.

### Verification

Report the actual result of:

```text
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Do not report a command as passing unless it was actually executed.

### Scope check

Explicitly confirm that no AWE-002/AWE-003 executor, snapshot system, provenance store, audit store, MCP editor, CLI editor, or policy engine was implemented as part of AWE-001.

### Downstream readiness

Explain briefly why AWE-002 and AWE-003 can now build on the canonical model without redefining it.

---

## 19. Stop condition

Once AWE-001 satisfies the Definition of Done and all required verification gates pass, **STOP**.

Do not automatically continue to AWE-002.

The next issue must be started only when explicitly requested.

The intended sequence is:

```text
AWE-001  ← current task
   ↓
AWE-002  ← start only after explicit instruction
   ↓
AWE-003
   ↓
AWE-004
   ↓
remaining issue-resolution prompts
```

This sequential boundary is intentional: each issue must be implemented, tested, verified, and reconciled with the repository before the next issue changes the architecture.
