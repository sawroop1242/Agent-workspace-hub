# AWE-004 — Implement Multi-Operation `filesystem.patch`

## Issue

GitHub #25 — **AWE-004: Implement multi-operation filesystem.patch**

## Dependencies

This issue depends on:

- **#22 — AWE-001: Design the canonical Edit Transaction model**
- **#23 — AWE-002: Implement safe contextual replacement**
- **#24 — AWE-003: Implement line-range insert and delete operations**

The implementation must build on the canonical edit model and the safe single-operation primitives already implemented by #23 and #24.

Do **not** redesign those primitives.

---

# 1. Objective

Implement the first true multi-operation editing primitive in AWH:

```text
filesystem.patch
```

`filesystem.patch` accepts one canonical `EditTransaction` containing multiple edit operations and applies them as one controlled edit request.

The primary safety property is:

> **Validate every operation before mutating any file, and never leave a partially applied patch when validation/preparation fails.**

Conceptually:

```text
Agent
  ↓
EditTransaction
  ↓
filesystem.patch
  ↓
validate entire transaction
  ↓
read all required file states
  ↓
validate all expected states
  ↓
validate every operation
  ↓
construct all resulting contents
  ↓
commit mutations
  ↓
verify resulting states
  ↓
return complete patch result
```

This issue is about **multi-operation orchestration**.

It is not the full snapshot/rollback/provenance system planned for later issues.

---

# 2. Architectural principle

AWH remains:

> **MCP-first, agent-agnostic workspace infrastructure.**

The agent decides what should change.

AWH is responsible for safely applying the requested changes.

Do not add:

- LLM reasoning
- agent planning
- model providers
- prompt generation
- autonomous conflict resolution
- agent workflow logic

The core implementation belongs in the existing edit service layer.

Future interfaces such as:

```text
MCP
CLI
TUI
Control API
```

must eventually call the same `EditService` implementation.

Do not implement those transports in this issue.

---

# 3. Inspect the repository first

Before modifying code, inspect the current repository state.

At minimum inspect:

```text
AGENTS.md
Cargo.toml
src/services/mod.rs
src/services/edit.rs
src/services/files.rs
```

Also inspect the implementations completed for:

- AWE-001 / #22
- AWE-002 / #23
- AWE-003 / #24

Search for:

- `EditTransaction`
- `EditOperation`
- `ExpectedState`
- `FileState`
- `EditStatus`
- `EditError`
- replacement implementation
- insert implementation
- delete-range implementation
- filesystem write helpers
- atomic-write helpers
- existing tests
- temporary filesystem fixtures

Do not assume the implementation matches an earlier prompt exactly.

The repository's current implementation is authoritative.

---

# 4. Core requirement

Implement a method conceptually equivalent to:

```rust
EditService::patch(transaction)
```

The exact Rust signature should follow the existing service conventions.

It must accept the canonical AWE-001 `EditTransaction`.

Do not create a second `PatchRequest`, `PatchOperation`, or `PatchTransaction` model if the canonical edit types already represent the required data.

The existing `EditOperation` must remain the source of truth.

Supported operations for this issue are the operations already completed by #23 and #24:

```text
Replace
Insert
DeleteRange
```

---

# 5. Patch semantics

A patch represents an ordered collection of operations:

```text
EditTransaction {
    id,
    operations: [
        operation_1,
        operation_2,
        ...
        operation_N
    ],
    expected,
    ...
}
```

The patch must have deterministic behavior.

Operations must be interpreted in the order they occur in the transaction.

Do not reorder operations merely for optimization.

If two operations target the same file, their ordering must remain explicit and deterministic.

---

# 6. Validate the entire transaction before mutation

This is the most important requirement of AWE-004.

The implementation must **not** do:

```text
apply operation 1
apply operation 2
discover operation 3 is invalid
return error
```

That would leave a partial patch.

Instead:

```text
read
validate operation 1
validate operation 2
validate operation 3
construct operation 1 result
construct operation 2 result
construct operation 3 result
--------------------------------
only now begin mutation
```

Therefore:

> **No filesystem mutation may occur while the patch is still in its validation/preparation phase.**

Any failure during validation/preparation must leave the workspace unchanged.

---

# 7. Two-phase patch architecture

Implement the patch operation in two conceptual phases.

## Phase A — Prepare

```text
Transaction
   ↓
Structural validation
   ↓
Resolve all paths
   ↓
Read required files
   ↓
Capture original FileState
   ↓
Validate expected states
   ↓
Apply operations in memory
   ↓
Validate all resulting contents
   ↓
Prepare mutation set
```

No persistent filesystem mutation should occur during this phase.

## Phase B — Commit

```text
Prepared mutation set
        ↓
write target files
        ↓
verify resulting files
        ↓
return result
```

Keep these phases logically separate even if the implementation uses small internal helper structures.

---

# 8. Reuse #23 and #24 semantics

Do not duplicate or subtly change the semantics already established by #23 and #24.

### #23 — Safe contextual replacement

Reuse its:

- exact replacement behavior
- occurrence semantics
- expected-state checking
- contextual matching
- conflict behavior
- file-state calculation
- security boundary
- UTF-8 handling

### #24 — Line-range insert/delete

Reuse its:

- line numbering
- insertion semantics
- deletion semantics
- EOF behavior
- newline handling
- expected-state checking
- validation behavior
- Unicode behavior

The multi-operation patch should compose these operations rather than implement alternative versions.

If the existing service exposes reusable internal helpers, use them.

If the existing API only exposes mutation methods, refactor minimally so patch preparation can reuse the underlying operation logic **without performing filesystem writes during preparation**.

Do not create a second implementation of replacement/insert/delete.

---

# 9. Critical design requirement: operations must compose in memory

A multi-operation patch may contain operations targeting the same file.

Example:

```text
file.rs

Operation 1:
replace "foo" → "bar"

Operation 2:
insert a line before line 20

Operation 3:
delete lines 30..32
```

The operations must compose against the state produced by previous operations in the same patch.

Conceptually:

```text
original content
      ↓
operation 1
      ↓
intermediate content 1
      ↓
operation 2
      ↓
intermediate content 2
      ↓
operation 3
      ↓
final content
```

Do **not** write each intermediate state to disk.

Instead:

```text
disk state
   ↓
in-memory content
   ↓
operation 1
   ↓
operation 2
   ↓
operation 3
   ↓
final content
   ↓
single controlled write
```

This is essential for correctness and for the later transaction/rollback architecture.

---

# 10. Same-file operation semantics

Multiple operations may target the same file.

The implementation must define:

> Later operations observe the in-memory result of earlier operations in the same patch.

Example:

```text
Original:
A
B
C

Operation 1:
replace B → X

Operation 2:
insert before line 3:
Y
```

Operation 2 operates against:

```text
A
X
C
```

not the original content.

Document this behavior in code-level documentation/tests.

---

# 11. Expected-state semantics

AWE-004 must carefully distinguish transaction-level and operation/file-level expected state where supported by AWE-001.

If individual operations carry expected state, honor those semantics as defined by #23/#24.

The important rule is:

> Expected-state validation must happen against the state that the operation is logically expected to observe.

For multiple operations on the same file, do not blindly compare every operation's expected hash against the original disk hash.

Example:

```text
original hash = H1

operation 1
expected = H1
produces H2

operation 2
expected = H2
produces H3
```

This should be valid.

But:

```text
operation 1
expected = H1
produces H2

operation 2
expected = H1
```

must produce a conflict if operation 2 explicitly expects the original state.

Do not silently reinterpret expected-state contracts.

---

# 12. External stale-state detection

Before preparing a patch, capture the current file state.

If the file has changed externally before patch preparation completes:

```text
expected state != actual state
```

return a structured conflict.

No mutation.

Example:

```text
Agent reads file
      ↓
External process modifies file
      ↓
Agent submits patch
      ↓
AWH detects stale state
      ↓
PATCH REJECTED
      ↓
workspace remains externally modified
```

AWH must never overwrite the external modification silently.

---

# 13. Filesystem security

All target paths must remain subject to the existing `FilesService` security boundary.

Reuse the existing mechanisms for:

- workspace-root enforcement
- absolute-path rejection
- `../` traversal rejection
- nested traversal rejection
- symlink escape prevention
- file-size limits
- UTF-8 file handling

Do not implement a second path resolver in `EditService`.

Do not bypass `FilesService` security merely because the patch needs lower-level filesystem access.

If a lower-level atomic write helper is required, it must operate only on already validated paths.

---

# 14. Path grouping

The implementation may group operations by target path internally, but grouping must not change semantic ordering.

For example:

```text
operations:
1 A
2 B
3 A
```

The implementation may internally represent:

```text
A → [1, 3]
B → [2]
```

but operation semantics must remain:

```text
1 → 2 → 3
```

Do not reorder operations based on filename, path, or optimization.

---

# 15. Preparation data structure

Introduce a small internal preparation representation if needed.

Conceptually:

```rust
PreparedFileMutation {
    path,
    before: FileState,
    original_content,
    final_content,
    after: FileState,
}
```

Exact naming is flexible.

The purpose is to separate:

```text
what the patch intends to do
```

from:

```text
what will actually be written
```

Do not expose internal preparation structures as public transport APIs unless required by the existing architecture.

---

# 16. Duplicate target handling

Multiple operations may target the same file.

The implementation must not accidentally treat each operation as an independent file transaction.

Instead:

```text
one logical patch
→ one final content state per target file
```

For example:

```text
A: operation 1
A: operation 2
A: operation 3
```

should normally produce:

```text
one final mutation for A
```

after all operations have been prepared in memory.

This avoids writing intermediate states.

---

# 17. Multiple-file patch

Support transactions such as:

```text
file_a.rs
file_b.rs
file_c.rs
```

with operations across multiple files.

All files must be completely prepared before any mutation begins.

Example:

```text
file A valid
file B valid
file C invalid
```

must result in:

```text
A unchanged
B unchanged
C unchanged
```

No partial patch.

This is a critical acceptance test.

---

# 18. Atomicity boundary

AWE-004 must provide **pre-commit all-or-nothing behavior for validation/preparation failures**.

However, do not falsely claim that AWE-004 implements crash-safe multi-file transactional commits if the repository does not yet have the required infrastructure.

Full transaction-level atomicity and rollback belongs to:

> **#27 — AWE-006: Make edits atomic and rollback-safe**

Therefore clearly distinguish:

### Required now

```text
validation failure
→ zero filesystem mutation
```

### Later

```text
commit starts
→ process crashes halfway through multi-file commit
→ automatic recovery/rollback
```

The second behavior belongs to #27.

If existing infrastructure already provides stronger guarantees, reuse it, but do not redesign the snapshot/rollback system in this issue.

---

# 19. Mutation strategy

For each final target file:

```text
prepared original
        ↓
prepared final content
        ↓
controlled filesystem write
```

Do not write intermediate operation states.

If the repository already has an atomic file-write helper, reuse it.

If it does not, use the narrowest safe mechanism compatible with the existing service architecture and clearly document that full transaction rollback remains #27.

Do not introduce a giant transaction framework.

---

# 20. Validation order

Use a deterministic validation sequence:

```text
1. Validate transaction is non-empty
2. Validate operation structure
3. Validate all paths
4. Load all required original file contents
5. Capture original FileState
6. Validate expected states
7. Apply operations in memory in transaction order
8. Validate all resulting contents
9. Build final mutation set
10. Commit filesystem mutations
11. Verify final states
12. Return result
```

Do not mutate at steps 1–9.

---

# 21. Failure behavior

If any preparation step fails, use the canonical AWE-001 status/error taxonomy.

Prefer:

```text
invalid path
→ ValidationFailed

invalid line range
→ ValidationFailed

zero replacement matches
→ ValidationFailed

unexpected replacement occurrence count
→ ValidationFailed

expected hash mismatch
→ Conflict

expected size mismatch
→ Conflict

expected line-count mismatch
→ Conflict
```

Use the existing structured error taxonomy where possible.

Do not use `ApplyFailed` for errors that occur before mutation.

---

# 22. Empty transaction

A patch with zero operations must be rejected consistently with AWE-001.

Do not treat an empty transaction as a successful no-op unless the canonical model explicitly defines that behavior.

Prefer the existing `EmptyTransaction` semantics.

---

# 23. Operation ordering

Operation order is part of the transaction contract.

For example:

```text
[
    Replace,
    Insert,
    DeleteRange
]
```

must execute logically as:

```text
Replace
→ Insert
→ DeleteRange
```

Do not sort operations.

Do not parallelize operations against the same file.

Parallel preparation may be considered for independent files only if it does not complicate deterministic behavior, but correctness is more important than optimization.

Prefer the simplest deterministic implementation for this milestone.

---

# 24. Conflict semantics for same-file operations

Consider:

```text
Replace("foo", "bar")
Replace("foo", "baz")
```

If operation 1 removes the only `foo`, operation 2 should naturally observe the intermediate in-memory state and fail if its matching conditions are no longer satisfied.

Do not automatically reinterpret or merge conflicting operations.

Likewise:

```text
DeleteRange(10..20)
Insert(before line 15)
```

must operate according to the documented sequential semantics.

---

# 25. Result model

Reuse the canonical AWE-001 transaction/result structures.

The result should provide enough information to understand the patch outcome.

At minimum expose or preserve:

```text
edit_id
final status
operations
affected paths
before state
after state
```

Where the existing architecture allows it, identify:

```text
successful operation index
failed operation index
```

This is especially useful for deterministic debugging.

Do not introduce snapshot IDs, audit IDs, provenance IDs, agent IDs, or session IDs solely for this issue.

---

# 26. Verification

After committing the prepared mutations, verify the resulting files.

For every affected path:

```text
file exists
→ file is readable
→ actual content is correct
→ actual hash == reported after hash
```

The patch must not report success based only on write-call return values.

Do not build the full #29 verification framework here.

Implement only the minimal verification needed to guarantee the patch result is truthful.

---

# 27. No full-file rewrite abstraction

AWH's agent-facing editing architecture must remain patch-oriented.

Do not add a new public API such as:

```text
filesystem.write_full_file
filesystem.replace_entire_file
```

as the solution to multi-operation editing.

Internally, constructing complete content in memory before committing it is acceptable and often necessary.

The important distinction is:

```text
internal prepared final content
```

versus:

```text
agent-facing raw full-file rewrite API
```

Do not expose the latter.

---

# 28. Tests — mandatory

Add focused unit and filesystem integration tests.

## Basic patch tests

Test:

1. one replacement operation
2. one insertion operation
3. one delete-range operation
4. multiple operations on one file
5. multiple operations across multiple files
6. mixed operation types
7. deterministic operation order

---

# 29. Same-file composition tests

Mandatory examples:

```text
Replace → Replace
Replace → Insert
Insert → Replace
Insert → DeleteRange
DeleteRange → Insert
Replace → DeleteRange
```

Verify that later operations observe the in-memory state produced by earlier operations.

---

# 30. Multi-file tests

Test:

```text
A valid
B valid
C valid
→ all three modified correctly
```

Then:

```text
A valid
B invalid
C valid
```

Expected:

```text
A unchanged
B unchanged
C unchanged
```

This is one of the most important tests in AWE-004.

---

# 31. Validation-before-mutation tests

Create a transaction where a later operation fails.

Example:

```text
operation 1 = valid
operation 2 = invalid line range
```

Assert:

```text
operation 1 was NOT persisted
```

Then test:

```text
operation 1 = valid
operation 2 = zero replacement matches
```

Assert the same invariant.

---

# 32. Expected-state tests

Test:

- matching hash succeeds
- mismatched hash produces conflict
- mismatched size produces conflict
- mismatched line count produces conflict
- stale external modification produces conflict
- conflict causes zero mutation

For same-file sequential operations, test expected states against the correct logical state at each operation.

---

# 33. Path-security tests

Verify that every operation in a patch remains subject to existing filesystem security.

Test combinations such as:

```text
valid operation
+
../escape operation
```

and:

```text
valid operation
+
absolute-path operation
```

Expected:

```text
whole patch rejected
no mutation
```

Also test symlink escape.

Do not duplicate all `FilesService` security tests; prove that the patch path cannot bypass the existing boundary.

---

# 34. Unicode/newline tests

Use:

- UTF-8
- Devanagari/Hindi
- emoji
- multiline strings
- empty lines
- LF
- CRLF where supported
- final newline
- no final newline

Verify that unrelated content remains unchanged.

---

# 35. Result-state tests

After a successful patch:

```text
reported before state
    == actual original state

reported after state
    == actual final state
```

Verify hashes independently.

For multiple files, verify every affected file.

---

# 36. Failure invariant

Add a reusable test assertion where practical:

```text
workspace_before == workspace_after
```

for every pre-commit failure.

This should cover:

- invalid path
- invalid operation
- invalid line range
- replacement mismatch
- expected-state conflict
- oversized resulting content
- missing target
- malformed transaction

The invariant is:

> **No preparation failure may mutate the workspace.**

---

# 37. Error attribution

When an operation fails, the result/error should identify enough information to diagnose the failure.

Prefer:

```text
edit_id
operation index
path
error category
```

Avoid including entire file contents in errors.

Do not leak sensitive data through error messages.

---

# 38. Performance

Do not prematurely optimize.

Correctness is the priority.

Reasonable implementation:

```text
read target files
→ maintain in-memory content
→ apply operations
→ write final content once per affected file
```

Avoid repeated persistent reads/writes for every operation.

For a patch containing 100 operations on one file, the implementation should not perform 100 persistent writes.

---

# 39. Concurrency

Do not implement a complete locking system in this issue.

However:

- expected-state validation must remain authoritative
- external changes must not be silently overwritten
- operations targeting the same file must use one coherent in-memory state
- do not concurrently mutate the same file

Advanced workspace locking can be addressed later if required.

---

# 40. Scope boundaries

Strictly out of scope:

### #26 — AWE-005

Do not implement unified diff parsing.

### #27 — AWE-006

Do not implement full crash-safe multi-file rollback, transaction journals, or automatic recovery.

### #28 — AWE-007

Do not build a generalized advanced conflict-resolution framework. Only implement conflict behavior required by the existing expected-state model.

### #29 — AWE-008

Do not implement the complete verification framework.

### #30

Do not expose MCP tools yet.

### #31

Do not implement edit rollback.

### #32

Do not redesign capability/policy enforcement.

### #33

Do not integrate snapshots/provenance.

### #34

Do not add the complete audit lifecycle.

### #35

Do not add CLI commands.

### #36

Do not attempt to build the complete final editing test suite.

### #37

Do not perform real MCP-client interoperability testing as part of this issue.

### #38

Do not build the complete end-to-end acceptance workflow.

### #39

Do not write the final public editing contract yet.

Keep AWE-004 focused on the multi-operation core.

---

# 41. Compatibility with previous issues

Before coding, verify that:

```text
#22 canonical model
        ↓
#23 Replace
        ↓
#24 Insert/DeleteRange
        ↓
#25 Patch
```

forms one coherent abstraction.

The final architecture should conceptually look like:

```text
                 EditTransaction
                       │
              ┌────────┴────────┐
              │                 │
        single operation    multiple operations
              │                 │
        ┌─────┴─────┐           │
        │     │     │           │
     Replace Insert Delete       │
        │     │     │           │
        └─────┴─────┴─────┬─────┘
                          │
                    EditService
                          │
                    FilesService
```

There must be one canonical editing model.

---

# 42. Refactoring rule

If #23/#24 currently contain mutation logic that cannot be safely reused for a prepare-then-commit patch, refactor it minimally.

Preferred architecture:

```text
operation validation
        ↓
operation transformation
        ↓
in-memory content
        ↓
commit
```

rather than:

```text
operation
    ↓
filesystem write
```

Do not duplicate the entire implementations.

A small internal helper such as:

```rust
apply_operation_to_content(...)
```

may be appropriate if it matches the existing architecture.

The helper should transform in-memory content and perform no persistent filesystem mutation.

---

# 43. Security invariant

For any patch:

```text
if any operation is invalid or unsafe
    → no operation is persisted
```

A patch must never allow:

```text
safe operation
+
unsafe operation
=
safe operation persisted
```

The entire patch must fail before commit.

Capability/policy enforcement itself belongs to #32, but the patch architecture must leave one clean boundary where those checks can later be applied.

Do not add ad-hoc authorization logic here.

---

# 44. Status lifecycle

Use the canonical AWE-001 status model.

A reasonable lifecycle is:

```text
Requested
   ↓
Located
   ↓
Validated
   ↓
Applied
   ↓
Verified
   ↓
Committed
```

However:

> Do not claim `Authorized` or `Snapshotted` unless those states are backed by real infrastructure.

Those belong to later capability/snapshot integration.

Failure states should accurately distinguish:

```text
ValidationFailed
Conflict
ApplyFailed
VerificationFailed
```

---

# 45. Verification commands

Run:

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Also run the focused edit tests independently.

If any command fails:

- report the exact failure
- do not claim completion
- fix only issues relevant to AWE-004 unless an existing regression blocks the work

---

# 46. Code-review checklist

Before marking AWE-004 complete:

- [ ] AWE-001 canonical `EditTransaction` is reused.
- [ ] #23 replacement implementation is reused.
- [ ] #24 insert/delete implementation is reused.
- [ ] No duplicate editing model exists.
- [ ] Multiple operations are supported.
- [ ] Operation order is deterministic.
- [ ] Same-file operations compose in memory.
- [ ] Multiple files are supported.
- [ ] All operations are validated before mutation.
- [ ] A later validation failure cannot persist earlier operations.
- [ ] Expected-state checks are respected.
- [ ] Stale external changes are detected.
- [ ] FilesService path security remains authoritative.
- [ ] No intermediate file states are written.
- [ ] One final mutation per affected file is preferred.
- [ ] Before/after states are accurate.
- [ ] Successful results are verified.
- [ ] Failure errors identify the relevant operation where practical.
- [ ] Unicode/newline behavior is tested.
- [ ] Multi-file partial-failure test exists.
- [ ] Same-file composition tests exist.
- [ ] No MCP/CLI/TUI implementation was added.
- [ ] No snapshots/provenance/audit implementation was added.
- [ ] No rollback system was added.
- [ ] No unified-diff parser was added.
- [ ] No capability-policy redesign was added.
- [ ] Formatting/check/test/clippy results are known.

---

# 47. Definition of Done

AWE-004 is complete when an agent can submit:

```text
EditTransaction {
    operations: [
        Replace(...),
        Insert(...),
        DeleteRange(...),
        Replace(...)
    ]
}
```

and AWH can:

```text
1. Validate the complete transaction
2. Resolve every target securely
3. Read all required files
4. Validate expected states
5. Apply operations sequentially in memory
6. Produce one final content state per affected file
7. Detect any preparation failure before persistence
8. Persist only the prepared final states
9. Verify resulting files
10. Return accurate before/after state
```

The central invariant must hold:

```text
ANY PRE-COMMIT FAILURE
        ↓
ZERO PATCH MUTATION
```

And:

```text
VALID PATCH
        ↓
DETERMINISTIC IN-MEMORY COMPOSITION
        ↓
FINAL FILE STATES
        ↓
VERIFIED RESULT
```

The implementation must establish a clean foundation for:

```text
#26 Unified Diff
#27 Atomic/Rollback-safe transactions
#28 Advanced Conflict Detection
#29 Verification
```

without requiring another rewrite of the canonical editing architecture.

---

# 48. Recommended commit

Use a focused commit:

```text
feat(edit): add multi-operation filesystem patch
```

Keep the commit limited to AWE-004 and directly required tests/helpers.

---

# 49. Final implementation report

After implementation, report:

1. Files changed.
2. Exact `filesystem.patch` behavior.
3. How operation ordering works.
4. How same-file operations compose.
5. How multi-file patches are prepared.
6. How expected-state validation works.
7. How stale-state conflicts are handled.
8. How zero-mutation-on-validation-failure is guaranteed.
9. How FilesService security is reused.
10. How final file states are committed.
11. Verification behavior.
12. Tests added.
13. Exact results of `cargo fmt`, `cargo check`, `cargo test`, and `cargo clippy`.
14. Any necessary refactoring to #23/#24.
15. Any intentionally deferred behavior for #26–#39.
16. Commit SHA.

Do not claim AWE-004 is complete unless the required acceptance criteria and checks actually pass.
