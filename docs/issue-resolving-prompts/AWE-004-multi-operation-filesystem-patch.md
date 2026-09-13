# AWE-004 / #25 — Multi-Operation `filesystem.patch`

## Role
You are the implementation agent responsible for resolving **AWE-004 / GitHub issue #25** on the `rust` branch of Agent Workspace Hub.

This is a **P0 agent-grade editing foundation task**. Treat this document as the complete implementation contract. Do not implement future editing milestones merely because they are adjacent to this work.

---

## 1. Mission

Implement the canonical multi-operation patch executor that composes the already-defined AWE-002 safe contextual replacement and AWE-003 line-range insert/delete primitives into **one transaction-level edit service**.

The central invariant is:

> **Prepare and validate every operation and every affected file before mutating any file.**

A request containing operations for files A, B, and C must never leave A or B changed merely because a later operation fails during validation, conflict detection, parsing, or in-memory preparation.

The implementation must be transport-independent and live below MCP/CLI/TUI/Control API layers.

Issue contract: AWE-004 must provide one canonical patch executor, deterministic same-file composition, multi-file preparation before the first write, per-file before/after state, one stable `EditId`, stale-state protection, structured failures, and zero mutation on preparation failure. fileciteturn42file0

---

## 2. Required pre-flight reading

Before changing code, inspect the current `rust` branch rather than relying on historical assumptions.

Read:

- `docs/PROJECT_CONTEXT.md`
- `docs/PROJECT_ROADMAP.md`
- `docs/PROJECT_ROADMAP_STATUS.md`
- `docs/PROJECT_STATUS.md`
- `docs/FEATURES.md`
- `docs/CLI.md`
- `docs/architecture.md`
- `docs/mcp.md`
- `docs/issue-resolving-prompts/AWE-001-canonical-edit-transaction-model.md`
- `docs/issue-resolving-prompts/AWE-002-safe-contextual-replacement.md`
- `docs/issue-resolving-prompts/AWE-003-line-range-insert-delete.md`
- this file

Then inspect the actual implementation, especially:

- `src/services/edit.rs`
- existing filesystem/path-security services
- existing atomic-write helpers
- existing workspace/root resolution
- relevant tests
- module exports
- any current callers of `EditTransaction`, `EditOperation`, `FileState`, or edit errors

Do not infer implementation from filenames alone. Trace actual call paths.

The current canonical model is deliberately transport-independent and contains `EditId`, `EditOperation`, `ExpectedState`, `FileState`, lifecycle status, and structured edit errors. The model itself explicitly leaves mutation logic to later `EditService` operations. fileciteturn63file0

The repository already treats canonical path validation and atomic filesystem writes as shared security infrastructure. Reuse those facilities rather than creating another filesystem-security implementation. fileciteturn64file0

---

## 3. Forensic reality — verify before coding

Do not assume that the existence of types means AWE-004 is implemented.

The forensic issue establishes that before this task:

- `EditTransaction` is not a production transaction executor.
- There is no canonical `filesystem.patch` executor.
- AWE-002 and AWE-003 are the primitive operation layers.
- AWE-004 is the first transaction boundary combining those primitives.
- AWE-006 will later add semantic rollback safety.

Therefore this task must create the **transaction preparation and commit orchestration**, not merely add another enum variant or wrapper.

Do not duplicate the AWE-002 replacement algorithm or AWE-003 line-edit algorithm inside AWE-004. AWE-004 is an orchestrator/composer over canonical primitives.

---

## 4. Canonical architecture

The intended flow is:

```text
caller
  ↓
EditTransaction
  ↓
shape validation
  ↓
resolve + validate every affected path
  ↓
load every affected file
  ↓
capture current FileState for every affected file
  ↓
validate every ExpectedState / contextual precondition
  ↓
prepare every operation in memory
  ↓
compose same-file operations deterministically
  ↓
compute final content for every affected file
  ↓
compute final FileState for every affected file
  ↓
ONLY NOW begin filesystem mutation
  ↓
commit final content using canonical atomic-write primitive
  ↓
verify actual on-disk state
  ↓
return one transaction result
```

The critical phase boundary is:

```text
PREPARE / VALIDATE
        |
        |  no filesystem mutation permitted above this line
        v
COMMIT
```

No operation may write to disk while another operation is still capable of failing preparation.

---

## 5. Transaction identity

Use the canonical `EditTransaction.id` / `EditId`.

Requirements:

- One logical patch request gets **one stable `EditId`**.
- Do not generate a new transaction ID for each operation.
- Do not create unrelated per-file transaction identities.
- If operation-level helper calls require internal identifiers, they must remain subordinate to the outer transaction and must not replace the canonical transaction identity.
- Preserve the existing `EditTransaction` lifecycle semantics.

The result must make it possible to correlate every affected file with the same logical patch.

---

## 6. Input validation

Validate the entire transaction before mutation.

At minimum validate:

- transaction is non-empty
- operation count and expected-state mapping are consistent
- every path is valid
- no forbidden absolute/traversal path is accepted
- operation-specific arguments are valid
- AWE-002 replacement requirements are valid
- AWE-003 line/range requirements are valid
- referenced files are valid targets
- duplicate/overlapping operations have deterministic semantics
- unsupported operation types fail explicitly rather than being ignored

Do not silently skip malformed operations.

If operation N is invalid, operations 1..N-1 must not have changed any file.

---

## 7. Expected-state and stale-state discipline

AWE-004 inherits the stale-state contract from AWE-002/AWE-003.

For every affected file:

1. Read the current bytes/content.
2. Build its canonical `FileState`.
3. Validate the caller's expected state before preparing mutation.
4. Reject stale/conflicting state deterministically.
5. Never silently overwrite a newer version.

Expected-state checks may include the canonical:

- SHA-256 hash
- byte size
- line count
- contextual expectation

Do not treat `ExpectedState.context` as decorative metadata.

When multiple operations affect one file, establish a clear rule for expected-state validation:

- caller expectations describe the state at the transaction boundary;
- they must be checked against the original file state, not against an already-mutated intermediate state;
- later operations are prepared against the transaction's in-memory state produced by earlier operations;
- never re-read the partially prepared state from disk and accidentally turn an in-memory transaction into a sequence of independent commits.

If the current model's expected-state mapping is insufficient for multiple operations on one file, make the smallest backward-compatible domain clarification required. Do not invent a second transaction model.

---

## 8. Same-file operation composition

Multiple operations targeting the same file must compose **entirely in memory**.

Example:

```text
file A:
  operation 1: replace
  operation 2: insert
  operation 3: delete range
  operation 4: replace
```

Required behavior:

```text
disk original
     ↓
read once
     ↓
in-memory state
     ↓
apply op 1
     ↓
apply op 2
     ↓
apply op 3
     ↓
apply op 4
     ↓
final in-memory content
     ↓
one final filesystem commit
```

Do **not** write after each operation.

The transaction order must be deterministic and documented. Unless an existing canonical contract says otherwise, preserve the order of `EditTransaction.operations`.

Do not reorder operations merely for optimization if doing so changes semantics.

---

## 9. Line-operation interaction

AWE-003 defines line-oriented insert/delete semantics. AWE-004 must consume those semantics rather than recreate them.

Be particularly careful when an earlier operation changes line numbers used by a later operation.

The contract must explicitly define that subsequent operations are applied to the **current in-memory transaction state**, not the original disk state.

Test examples such as:

- insert then delete
- delete then insert
- replace then insert
- insert then replace
- multiple deletes
- multiple inserts at the same boundary
- replace followed by delete of affected lines
- multiple operations targeting the first and last lines

For every such case, assert deterministic final content.

---

## 10. Multi-file preparation

For a transaction touching multiple files:

```text
A valid
B invalid
C valid
```

must result in:

```text
A unchanged
B unchanged
C unchanged
```

This is a mandatory acceptance invariant.

The executor must load, validate, and prepare **all affected files** before the first filesystem write.

Do not use a loop shaped like:

```text
for operation in operations {
    validate(operation);
    write(operation);
}
```

unless the write phase is strictly separated from the complete preparation phase.

Prefer an explicit two-phase internal design:

```text
PreparationPlan
    affected files
    original states
    final contents
    final states

CommitPlan
    atomic writes
    verification
```

Names may differ, but the architectural separation must remain clear.

---

## 11. Preparation data model

Use existing domain types where possible.

A prepared internal representation should be capable of holding, at minimum:

- canonical target path
- original content/bytes as required for state and conflict validation
- original `FileState`
- final prepared content
- final `FileState`
- operations applied to that file, if useful for diagnostics
- transaction ID

Do not expose an unnecessary new public API merely to implement internal orchestration.

Avoid cloning very large buffers unnecessarily. However, correctness and isolation take priority over premature optimization.

If the repository already has a suitable prepared-edit representation, reuse it.

---

## 12. Deterministic grouping and ordering

Operations may target the same or different files.

Create a deterministic affected-file grouping strategy.

Requirements:

- no nondeterministic `HashMap` iteration may determine observable mutation semantics
- operation application order remains explicit
- final result ordering is deterministic
- per-file before/after states have stable ordering
- diagnostics identify the failing operation and target path where possible

Do not rely on incidental filesystem or map iteration order.

---

## 13. Atomic commit boundary

After preparation succeeds for every operation and every file, commit the final content.

Reuse the repository's canonical atomic filesystem write implementation.

Do not introduce another atomic-write implementation inside AWE-004.

Where practical, perform one final write per affected file rather than writing intermediate states.

Do not claim that this provides semantic multi-file rollback or crash consistency. Those guarantees belong to AWE-006.

AWE-004 guarantees:

> **No partial mutation caused by validation/conflict/preparation failure.**

It does **not** yet guarantee:

> **Automatic restoration if a later filesystem commit or post-write verification fails.**

That distinction must remain explicit.

---

## 14. Commit failure semantics

Design the commit phase so its limitations are explicit.

If file A commits successfully and file B's commit subsequently fails, AWE-004 must not falsely report the transaction as fully committed.

Return a structured apply/commit failure that identifies:

- transaction ID
- affected path where failure occurred
- operation/phase when known
- original/prepared state where safe
- whether any files had already been committed

Do not implement rollback here just to hide this failure mode.

AWE-006 is responsible for turning the transaction boundary into rollback-safe multi-file mutation.

---

## 15. Post-commit verification

After commit, verify the actual filesystem state.

For every affected file, recompute the actual `FileState` and compare it with the prepared final state.

The returned result must report actual observed before/after states, not merely the states the code expected to write.

If verification fails:

- return a structured verification failure
- identify the affected file when possible
- do not claim `Committed`/`Verified` incorrectly
- do not silently retry or overwrite without an explicit contract
- do not add rollback behavior from AWE-006

---

## 16. Result contract

Return one transaction-level result associated with the canonical `EditId`.

It must provide enough information for callers to determine:

- transaction identity
- final lifecycle status
- affected files
- before state per file
- after state per file when commit/verification succeeds
- failure class and useful diagnostics when unsuccessful

Use the existing `EditTransaction`, `FileState`, `EditStatus`, and error vocabulary wherever possible.

Do not create a second competing edit result model unless the current architecture genuinely requires one.

---

## 17. Structured failure taxonomy

At minimum distinguish:

### Validation failure
Malformed transaction or invalid operation/path/range.

### Conflict failure
Expected hash/context/size/line state no longer matches the current file.

### Preparation failure
A required file cannot be read or an operation cannot be safely prepared.

### Apply failure
A filesystem commit fails after preparation.

### Verification failure
The observed post-write state does not match the prepared final state.

Errors should be machine-readable and include useful target/operation information without leaking sensitive file contents unnecessarily.

---

## 18. Path and filesystem security

Reuse the existing canonical workspace containment and path-security services.

Requirements:

- reject absolute paths
- reject `..` traversal
- enforce workspace containment
- preserve symlink-escape protections provided by the canonical helper
- use canonical atomic-write support
- do not add a second path validator
- do not bypass existing filesystem security by opening paths directly in a new helper

The project explicitly treats path validation and atomic filesystem operations as shared security infrastructure. fileciteturn64file0

Do not weaken existing security checks for convenience.

---

## 19. Concurrency and TOCTOU boundaries

AWE-004 must not claim stronger concurrency guarantees than the repository actually provides.

At minimum:

- validate expected state before mutation
- avoid unnecessary read/write gaps
- reuse existing workspace/file locking if an appropriate primitive already exists
- do not silently overwrite a file changed since the transaction's observed state

If the existing locking architecture cannot provide a full compare-and-swap guarantee, document the residual race rather than pretending it is solved.

Do not implement the full AWE-007 stale-state mutation coordinator here.

---

## 20. Encoding and newline correctness

The transaction must preserve the established AWE-002/AWE-003 text-edit contract.

Test and correctly handle:

- UTF-8
- Devanagari
- emoji / multibyte Unicode
- LF
- CRLF
- missing final newline
- empty file
- one-line file
- first line
- last line
- EOF

Never slice UTF-8 text by arbitrary byte offsets when the operation is defined in characters/lines.

Do not silently normalize newline style unless the existing canonical edit contract explicitly requires it.

---

## 21. Large-file and resource behavior

Respect all existing filesystem/file-size limits.

Do not introduce unbounded memory growth through accidental duplicate copies of every file and every intermediate operation.

However, do not trade away the core no-partial-preparation invariant merely to avoid an in-memory representation. If the repository has a documented size limit that makes the required preparation safe, reuse it.

If a transaction exceeds supported limits, fail before mutation.

---

## 22. Testing requirements

Testing is part of the implementation, not a follow-up task.

### Unit tests
Cover:

- empty transaction
- invalid operation
- invalid path
- expected-state count mismatch
- single-file single-operation success
- same-file multiple operations
- multiple files
- deterministic operation ordering
- expected-state conflict
- context conflict
- final-state calculation
- deterministic result ordering
- unsupported operation handling

### Integration tests with real temporary filesystems
Mandatory scenarios:

1. one-file replace
2. one-file insert
3. one-file delete
4. mixed same-file operations
5. multiple files success
6. **A valid; B invalid; C valid → all unchanged**
7. stale expected state on one file → all unchanged
8. missing file during preparation → all unchanged
9. invalid later operation → earlier valid operations remain unchanged
10. Unicode/Devanagari/emoji
11. LF and CRLF
12. no final newline
13. empty and one-line files
14. first/last/EOF boundaries
15. path traversal rejection
16. symlink escape rejection where supported
17. final hashes and metadata match actual bytes
18. post-commit verification behavior
19. commit failure behavior where safely injectable/testable
20. deterministic multi-file result ordering

### Property-oriented tests where practical
Useful invariants include:

- preparation failure implies zero filesystem mutation
- applying the same valid transaction to the same initial state is deterministic
- before-state hashes describe the bytes actually observed before preparation
- successful after-state hashes describe the bytes actually observed after verification

Do not add a heavyweight dependency merely for trivial property tests if the repository already has an adequate testing mechanism.

---

## 23. Regression compatibility

AWE-004 must preserve the contracts established by:

- AWE-001 canonical edit transaction model
- AWE-002 safe contextual replacement
- AWE-003 line-range insert/delete

AWE-004 must be reusable by future AWE-005 unified-diff application and AWE-006 rollback-safe editing.

AWE-005 must consume this transaction/preparation/commit machinery rather than creating a parallel diff transaction engine. fileciteturn47file0

AWE-006 must extend this boundary rather than receiving a second independent transaction coordinator. fileciteturn46file0

---

## 24. Transport independence

Do not implement MCP/CLI/TUI exposure in this issue.

The service layer must be callable independently of:

- MCP JSON-RPC
- CLI argument parsing
- TUI state
- Control API transport
- any specific AI agent

The roadmap explicitly requires CLI, MCP, TUI, and Control API to be interfaces over shared application services rather than separate filesystem/editing semantics.

---

## 25. Explicit non-goals

Do **not** implement as part of AWE-004:

- MCP `filesystem.patch` exposure
- CLI patch commands
- TUI patch UI
- Control API patch endpoint
- unified-diff parsing/application (AWE-005)
- semantic rollback
- snapshot persistence
- persistent provenance
- audit-history subsystem
- explicit completed-edit rollback
- stale-state coordination service (AWE-007)
- post-edit verification framework beyond the local transaction's required verification
- agent profiles/capabilities/policy architecture
- worktree isolation
- multi-agent orchestration
- remote/cloud editing
- connectors
- a second transaction model
- a second path-security implementation
- a second atomic-write implementation

If a missing prerequisite is discovered, make the smallest necessary compatibility-preserving change and explain why. Do not expand scope silently.

---

## 26. Code-quality requirements

Follow the repository's existing Rust conventions.

Requirements:

- idiomatic ownership/borrowing
- clear separation between preparation and commit
- no unnecessary `clone()` of large content
- structured `thiserror` errors at the service boundary
- `anyhow::Context` only at appropriate application boundaries
- Rustdoc for new public APIs
- no `unwrap()`/`expect()` in production paths unless an invariant is genuinely impossible and already accepted by project conventions
- no dead code
- no speculative abstractions
- no TODO/FIXME placeholders for required behavior

Do not refactor unrelated modules merely because you notice opportunities while implementing this issue.

---

## 27. Verification commands

Before declaring AWE-004 complete, run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Also inspect the final diff and verify:

```bash
rg "TODO|FIXME|unimplemented!|todo!" <changed-files>
```

There must be no newly introduced placeholders in the implementation.

Run the real temporary-filesystem integration tests, not only pure unit tests.

If any required gate fails, fix the cause and rerun the complete relevant gate before declaring success.

---

## 28. Diff and scope audit

Before finishing, explicitly audit:

```bash
git status --short
git diff --stat
git diff --check
git diff
```

Confirm that every changed file is necessary for AWE-004.

Do not modify this issue-prompt directory's future issue files.

Do not modify AWE-005, AWE-006, or later prompt files.

Do not modify unrelated documentation merely to make the task appear complete.

---

## 29. Definition of Done

AWE-004 is complete only when all of the following are true:

- [ ] canonical `EditService::patch` or equivalent production service exists
- [ ] one stable `EditId` represents the complete logical patch
- [ ] entire transaction shape is validated before mutation
- [ ] every affected file is loaded before the first write
- [ ] every expected-state precondition is checked before the first write
- [ ] same-file operations compose deterministically in memory
- [ ] operation order is explicit and tested
- [ ] multi-file preparation completes before the first write
- [ ] valid later operations cannot cause earlier files to mutate prematurely
- [ ] the A-valid/B-invalid/C-valid invariant is proven by a real filesystem test
- [ ] AWE-002 replacement behavior is reused, not duplicated
- [ ] AWE-003 line editing behavior is reused, not duplicated
- [ ] canonical path/security helpers are reused
- [ ] canonical atomic-write helpers are reused
- [ ] final content and final `FileState` are calculated before commit
- [ ] actual post-write state is verified
- [ ] structured validation/conflict/preparation/apply/verification errors exist
- [ ] before/after `FileState` is accurate
- [ ] UTF-8, Unicode, LF/CRLF, EOF, and boundary cases are tested
- [ ] traversal/symlink security remains enforced
- [ ] commit-failure limitations are explicit
- [ ] no semantic rollback is falsely claimed
- [ ] tests cover both unit and real temporary-filesystem behavior
- [ ] all required cargo gates pass
- [ ] `git diff --check` passes
- [ ] final diff contains only necessary AWE-004 changes

---

## 30. Required final implementation report

At completion, report exactly these categories:

### Implementation
- files changed
- service/API added
- preparation/commit architecture
- operation-composition semantics

### Safety
- expected-state enforcement
- path/security reuse
- atomic-write reuse
- no-partial-preparation guarantee
- concurrency/TOCTOU limitations

### Tests
- unit tests added
- integration tests added
- critical A-valid/B-invalid/C-valid test result
- Unicode/newline/boundary coverage

### Verification
- `cargo fmt` result
- `cargo check` result
- `cargo test` result
- `cargo clippy` result
- `git diff --check` result

### Scope audit
- exact changed-file list
- confirmation that no future AWE prompt was modified
- explicit statement that AWE-004 only was implemented

### Remaining work
List only genuinely remaining work belonging to later milestones, especially rollback/snapshot/advanced transaction coordination. Do not silently implement it.

---

## 31. Hard STOP rules

**STOP immediately and reassess** if implementation requires any of the following:

- a second transaction model
- a second path-security system
- a second atomic-write system
- direct transport-specific filesystem semantics
- silent stale-state overwrites
- writing one operation before preparing the rest of the transaction
- hidden rollback behavior that belongs to AWE-006
- modifying future issue-resolution prompts
- broad unrelated refactoring

If a requirement appears impossible with the current architecture, inspect the actual code and dependencies first. Make the smallest architecture-compatible change necessary, then document the constraint.

---

## Final instruction

Implement **only AWE-004 / GitHub issue #25** on branch `rust`.

Build the first real canonical multi-operation edit transaction boundary using the existing AWE-001 model and AWE-002/AWE-003 primitives.

The non-negotiable invariant is:

> **Validate and prepare the complete transaction in memory before mutating any file.**

Prove that invariant with real temporary-filesystem tests, including the multi-file **A valid → B invalid → C valid** case.

Do not implement future AWE milestones.

**STOP after AWE-004 is complete and verified.**

Do not modify AWE-005 or any other issue-resolution prompt file.