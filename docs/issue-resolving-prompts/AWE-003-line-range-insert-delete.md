# AWE-003 / #24 — Line-Range Insert/Delete

> **Type:** P0 agent-grade editing foundation  
> **Dependency:** AWE-001 / #22; consumes the AWE-002 safety contract  
> **Primary scope:** production line-oriented insert and inclusive delete-range executors  
> **Branch:** `rust`  
> **Status:** Issue-resolution master prompt  

## 1. Mission

Implement deterministic, production-grade line-oriented editing on the canonical AWH edit service:

- insert text at a documented line boundary;
- delete an inclusive line range;
- validate the complete expected file state before mutation;
- reject stale or ambiguous edits instead of silently overwriting newer content;
- preserve the repository's newline and UTF-8 semantics;
- apply changes through the canonical atomic filesystem boundary;
- return verified before/after `FileState` information.

This issue is the second concrete agent-grade editing primitive after safe contextual replacement. It must build on AWE-001's canonical transaction model and AWE-002's expected-state/conflict discipline. Do not create a parallel editing abstraction.

The implementation must fit the architecture and contracts documented in:

- `docs/PROJECT_CONTEXT.md`
- `docs/PROJECT_ROADMAP.md`
- `docs/PROJECT_ROADMAP_STATUS.md`
- `docs/PROJECT_STATUS.md`
- `docs/FEATURES.md`
- `docs/CLI.md`
- `docs/architecture.md`
- `docs/mcp.md`

The roadmap identifies controlled editing as a high-priority workspace-runtime capability and requires one shared application service behind CLI, MCP, TUI, and Control API. Those interfaces are not part of this issue; only the reusable service/domain implementation belongs here.

## 2. Hard Scope Rule

**Modify only what is necessary to implement AWE-003 correctly.**

Do not implement or expose in this issue:

- MCP tools/routes;
- CLI commands;
- TUI screens;
- Control API endpoints;
- snapshots or persistent snapshot storage;
- provenance persistence;
- audit persistence;
- worktree management;
- multi-agent orchestration;
- generic workflow/DAG functionality;
- unified-diff parsing/application;
- full rollback/history features;
- a new policy engine;
- a new filesystem security subsystem;
- a new edit transaction model.

If a later milestone needs a hook, expose the smallest transport-independent service contract required by that milestone. Do not implement the later feature itself.

## 3. Read and Audit Before Editing

Before modifying code, inspect the current `rust` branch rather than relying on historical documentation.

At minimum inspect:

```text
src/services/edit.rs
src/services/
src/core/
src/models/
src/mcp/
src/cli/
Cargo.toml
```

Locate the actual filesystem service and existing canonical path/containment and atomic-write helpers.

Search for:

```text
EditTransaction
EditOperation::Insert
EditOperation::DeleteRange
ExpectedState
FileState
EditError
EditStatus
replace
insert
delete_range
validate_path
atomic
workspace root
filesystem
```

Determine whether AWE-002's executor and helpers already exist on the branch. Reuse them. If a needed helper is missing, add the smallest reusable domain/service helper rather than duplicating it inside AWE-003.

Do not assume documentation filenames are implementation filenames. Verify the actual branch tree and current callers before adding abstractions.

## 4. Current Forensic Reality

The AWE-003 issue states that line operations were originally model-only and that the existing whole-file write path could silently overwrite newer content. The acceptance target is a real production executor with expected-state conflict protection, atomic application, real temporary-filesystem tests, and reuse by AWE-004. citeturn31file0

The canonical model currently defines:

```rust
EditOperation::Insert {
    path: String,
    line: usize,
    content: String,
}

EditOperation::DeleteRange {
    path: String,
    start_line: usize,
    end_line: usize,
}
```

The model currently documents `Insert.line` as a one-based boundary where `0` means the beginning of the file, and `DeleteRange` as an inclusive one-based range. Preserve the canonical model if AWE-001 has established this contract; do not introduce a competing numbering system. fileciteturn29file0

## 5. Canonical Contract

The production operation should follow this conceptual pipeline:

```text
caller
  ↓
EditTransaction
  ↓
validate transaction shape
  ↓
validate workspace-relative path
  ↓
read current file
  ↓
construct actual FileState
  ↓
validate ExpectedState hash/size/line_count/context
  ↓
parse/locate canonical line boundaries
  ↓
validate requested line semantics
  ↓
prepare complete resulting content in memory
  ↓
atomic write through canonical filesystem service
  ↓
verify resulting content/state
  ↓
return before + after FileState
```

No mutation may occur before all preconditions and line-boundary validation have succeeded.

For every failure before the atomic commit boundary:

```text
bytes_after == bytes_before
```

This is a core acceptance invariant, not merely an implementation preference.

## 6. Shared-Service Architecture

The implementation must belong to the existing canonical editing/application service.

Conceptually, the service may expose operations equivalent to:

```rust
insert(transaction: EditTransaction) -> Result<EditResult, EditError>
delete_range(transaction: EditTransaction) -> Result<EditResult, EditError>
```

The exact names/signatures must follow the repository's actual service architecture. Do not blindly copy these signatures if the existing AWE-002 service uses another coherent API.

The service must remain transport-independent.

CLI, MCP, TUI, and Control API must eventually call the same service rather than reimplementing line parsing or mutation semantics. This follows AWH's shared-service architecture and CLI contract. fileciteturn34file0

## 7. Operation Semantics — Line Numbering

### 7.1 One convention only

The implementation must document one canonical line convention and use it everywhere.

For AWE-003, preserve the existing AWE-001 contract:

- `DeleteRange.start_line` and `end_line` are **1-based inclusive line numbers**;
- `start_line == 0` is invalid for deletion;
- `start_line <= end_line` is required;
- `Insert.line` uses the existing model's documented boundary semantics, where `0` means beginning-of-file and a positive value identifies the line boundary before which insertion occurs.

Do not introduce zero-based deletion indices, mixed inclusive/exclusive APIs, or transport-specific numbering.

If the existing branch has already changed this contract through an authoritative implementation/test, preserve that actual canonical behavior and update documentation/tests consistently rather than creating two interpretations.

### 7.2 Boundary table

The implementation must explicitly define and test at least:

| Input | Required meaning |
|---|---|
| insert line `0` | insert at beginning of file |
| insert line `1` | insert before first logical line |
| insert line `N` | insert before logical line N |
| insert at EOF boundary | deterministic EOF behavior defined by the canonical model |
| delete `1..=1` | delete first line |
| delete `N..=N` | delete one line |
| delete `1..=N` | delete all existing lines |
| delete beyond last line | structured invalid-range error unless the canonical contract explicitly defines another behavior |
| start > end | structured invalid-range error |
| line `0` in delete | structured invalid-range error |

Do not silently clamp an invalid line number to the nearest valid line.

## 8. Insert Semantics

### 8.1 Literal insertion

`content` is literal text.

Do not:

- trim it automatically;
- normalize Unicode;
- normalize all line endings;
- interpret regex syntax;
- perform fuzzy matching;
- silently modify unrelated file content.

### 8.2 Insert position

Calculate the insertion point from the current file content only after expected-state validation has succeeded.

Insertion must be based on logical lines, not arbitrary byte offsets.

The implementation must handle:

- empty file;
- one-line file;
- first line;
- middle line;
- last line;
- EOF;
- file with trailing newline;
- file without trailing newline.

### 8.3 Content/newline behavior

Define how inserted content interacts with surrounding line separators.

The implementation must not create accidental concatenation such as:

```text
existing line + inserted line
```

when the semantic operation is intended to add a separate line.

At the same time, it must not blindly append an extra newline when the caller intentionally supplies one.

Use a deterministic normalization rule for the insertion boundary and test it thoroughly. Preserve the existing file's line-ending style unless the operation explicitly requires otherwise.

### 8.4 EOF insertion

EOF insertion must be explicit and deterministic.

Test all combinations of:

- empty file;
- non-empty file with trailing newline;
- non-empty file without trailing newline;
- inserted content with trailing newline;
- inserted content without trailing newline.

The final result must be predictable and documented by tests.

## 9. Delete-Range Semantics

`DeleteRange` removes an **inclusive** range of logical lines.

For a file with logical lines:

```text
1: alpha
2: beta
3: gamma
4: delta
```

`2..=3` must produce:

```text
alpha
delta
```

The implementation must not accidentally delete line 4 or retain line 3.

### Required cases

Test:

- first line only;
- last line only;
- middle line;
- multiple middle lines;
- first through last line;
- single-line file;
- deleting the only line;
- empty file;
- range equal to the entire file;
- invalid start/end ordering;
- start or end beyond the available line range.

### Delete-all result

Deleting all lines must have deterministic empty-file semantics.

Do not accidentally leave stale content, a partial newline, or unrelated bytes.

The expected resulting `FileState` must be calculated from the actual resulting bytes.

## 10. Empty-File Semantics

An empty file has zero logical lines under the canonical `FileState` semantics.

Therefore:

- deleting any positive line range from an empty file must fail deterministically;
- insertion at the beginning/EOF must remain possible according to the insert contract;
- no invalid deletion may mutate the empty file.

Do not invent a phantom line `1` merely to make deletion easier.

## 11. Expected-State Discipline

AWE-003 must consume the same expected-state safety contract as AWE-002.

Before calculating or applying a line edit:

1. read the current file;
2. construct the actual `FileState`;
3. validate supplied expected hash;
4. validate supplied expected size;
5. validate supplied expected line count;
6. validate supplied expected context according to the canonical AWE-002 semantics;
7. only then locate line boundaries and prepare the mutation.

A stale expected state must produce a structured conflict, not a best-effort edit.

Never silently refresh expected state from the current file and continue.

Never ignore a supplied context precondition.

Do not create another `ExpectedState` type.

## 12. Context Must Be Real, Not Decorative

If the caller supplies `ExpectedState.context`, it must participate in the same precondition mechanism established by AWE-002.

The line operation must not use context as an optional log message or diagnostic hint.

The implementation must verify context before mutation and return a deterministic conflict when it fails.

Do not invent a separate line-operation context matcher. Reuse the canonical helper/semantics from the existing edit service whenever available.

## 13. FileState Requirements

Before mutation, return/retain the actual:

```text
path
hash
size
line_count
```

After successful mutation, return/retain a verified resulting `FileState`.

At minimum verify:

```text
before.hash == SHA256(original bytes)
after.hash  == SHA256(resulting bytes)
```

Also verify size and line count according to the canonical model.

Do not manufacture an `after` state from expected arithmetic alone. Prefer deriving it from the actual resulting bytes and, where appropriate, a post-write observation.

## 14. UTF-8 and Unicode Safety

AWH's editing layer is text-oriented. The implementation must safely support valid UTF-8 containing:

- ASCII;
- accented Latin characters;
- Devanagari/Hindi;
- CJK;
- emoji;
- combining characters.

Do not slice a UTF-8 string at arbitrary byte positions.

Line boundaries may be found using byte indices, but every resulting string slice must remain on valid UTF-8 boundaries.

The operation must not normalize Unicode merely because it edits the file.

Tests must verify that unrelated Unicode content is byte-for-byte preserved where possible.

## 15. Newline Semantics

Explicitly support and test:

- LF (`\n`);
- CRLF (`\r\n`);
- files without a trailing newline;
- files with a trailing newline;
- empty files.

Do not silently convert a CRLF file to LF merely because a line was inserted or deleted.

Do not silently convert LF to CRLF.

Preserve the existing line-ending representation outside the actual edited region.

If the implementation needs a canonical internal representation, conversion must be reversible and the final serialization must follow the documented policy.

Tests must compare actual bytes, not only parsed lines.

## 16. Parsing and Line-Boundary Model

Create one internal, deterministic representation of line boundaries and reuse it for both insertion and deletion.

The representation must answer:

```text
number of logical lines
start byte of each line
end byte of each line
line terminator length
whether the file has a trailing newline
```

Do not create separate, subtly different line parsers for insert and delete.

The helper should be small, well-tested, transport-independent, and reusable by later editing milestones.

Do not prematurely build a generalized text editor/parser framework.

## 17. Validation Ordering

The implementation must validate in a safe order equivalent to:

```text
1. Validate transaction shape.
2. Validate operation type.
3. Validate workspace-relative path.
4. Read current file.
5. Build actual FileState.
6. Validate ExpectedState.
7. Build deterministic line map.
8. Validate line/boundary arguments.
9. Validate content/newline constraints.
10. Prepare complete resulting bytes/content in memory.
11. Apply through canonical atomic-write boundary.
12. Verify resulting FileState.
13. Return structured result.
```

Do not write before steps 1–10 succeed.

## 18. Zero-Mutation-on-Failure

This invariant must be proven by tests.

For every failure before commit, the original file must remain unchanged.

Required failure tests include:

- invalid transaction;
- invalid path;
- missing file;
- read error;
- stale hash;
- stale context;
- stale size;
- stale line count;
- invalid line `0` for deletion;
- start > end;
- out-of-range deletion;
- invalid insertion boundary;
- unsupported/invalid text content according to the existing policy;
- malformed insertion content if the canonical contract rejects it;
- atomic-write preparation/commit failure where realistically testable.

Compare original and final bytes/state in the real filesystem tests.

## 19. Atomic Filesystem Mutation

Use the repository's existing canonical atomic-write and path-security implementation.

Do not add a second implementation of:

- workspace containment;
- canonical path validation;
- symlink checking;
- atomic replacement.

The operation should prepare the entire new content first and then pass it through the existing mutation boundary.

The security boundary must remain centralized.

If the current filesystem layer has known TOCTOU limitations, do not falsely claim AWE-003 eliminates them. Preserve the existing guarantees and leave OS-level safe-open/race-resistant improvements to the appropriate filesystem hardening milestone.

## 20. Concurrency and Stale Writes

AWE-003 must never knowingly implement:

```text
read file
→ ignore changed state
→ overwrite file
```

The minimum safety sequence is:

```text
read
→ expected-state validation
→ locate/prepare
→ atomic apply
```

If AWE-002 provides a shared mutation-coordination or locking primitive, use it.

Do not create a second lock manager in AWE-003.

Do not claim full compare-and-swap or race-free semantics unless the underlying implementation actually provides and tests that guarantee.

AWE-007 and filesystem TOCTOU work may strengthen this later; AWE-003 must establish the correct safety boundary now.

## 21. Error Contract

Use the canonical structured edit-error architecture.

At minimum distinguish:

```text
invalid transaction
invalid path
file not found
read failure
invalid line range
invalid insertion boundary
line out of range
empty-file deletion
expected-state conflict
context conflict
unsupported/invalid text
write failure
verification failure
```

Do not return ambiguous string-only errors when callers need to branch on the failure class.

Errors should contain safe diagnostic data where appropriate:

```text
edit id
path
operation
requested range/boundary
actual line count
expected state summary
actual state summary
```

Do not include unnecessary private file contents or secrets.

## 22. Transaction Lifecycle

Reuse AWE-001's `EditStatus` vocabulary.

A successful line edit should represent the actual lifecycle, approximately:

```text
Requested
→ Located
→ Validated
→ Applied
→ Verified
→ Committed
```

Do not claim a status that did not occur.

On expected-state mismatch use the existing conflict status.

On malformed line input use the existing validation-failure semantics.

Do not add a new line-edit-specific state machine.

## 23. Test Matrix — Unit Tests

Add focused unit tests for the line-boundary helper and operation semantics.

### Insert

- empty file;
- insert at `0`;
- insert before first line;
- insert before middle line;
- insert before last line;
- EOF insertion;
- inserted content with newline;
- inserted content without newline;
- repeated inserts;
- Unicode insertion;
- CRLF insertion;
- LF insertion;
- file with/without trailing newline.

### Delete

- first line;
- middle line;
- last line;
- single line;
- multiple lines;
- delete all;
- empty file;
- invalid zero line;
- reversed range;
- end beyond EOF;
- Unicode/Devanagari/emoji;
- CRLF;
- LF;
- missing trailing newline.

### Expected state

- hash match;
- hash mismatch;
- size mismatch;
- line-count mismatch;
- context match;
- context mismatch;
- no expected state;
- multiple simultaneous mismatches with deterministic reporting.

## 24. Real Temporary-Filesystem Integration Tests

Unit tests alone are insufficient.

Use the actual production edit service with temporary workspace roots.

At minimum verify:

1. successful insert;
2. successful delete;
3. insert at beginning;
4. insert at EOF;
5. delete first/last/middle line;
6. delete entire file;
7. stale expected-state conflict;
8. zero mutation after stale conflict;
9. invalid range leaves bytes unchanged;
10. invalid path is rejected;
11. traversal is rejected by the canonical security layer;
12. symlink/containment behavior follows the existing security boundary;
13. CRLF bytes remain correct;
14. LF bytes remain correct;
15. missing final newline remains correctly represented;
16. Unicode/Devanagari/emoji remain valid;
17. before/after hashes are correct;
18. before/after line counts are correct;
19. repeated/edge insertions are deterministic;
20. real atomic write behavior is exercised.

## 25. Property-Oriented Testing

If the repository already has property-test infrastructure, extend it with focused invariants.

Useful properties include:

```text
invalid line operations never mutate the original file
successful delete removes exactly the requested inclusive logical lines
successful insert changes only the intended boundary/content region
reported after.hash equals SHA-256(resulting bytes)
reported before.hash equals SHA-256(original bytes)
Unicode input remains valid UTF-8
CRLF policy remains stable outside the edited boundary
```

Do not build a large fuzzing framework solely for this issue.

## 26. AWE-004 Compatibility

AWE-004 will combine multiple filesystem patch operations.

Therefore AWE-003 must expose reusable semantics rather than code that only works through one direct test path.

AWE-004 must be able to compose the same insert/delete primitives without copying their:

- line parsing;
- expected-state validation;
- path validation;
- atomic-write logic;
- error classification.

Do not implement AWE-004 in this issue.

## 27. CLI/MCP/TUI Boundary

Do not add CLI or MCP exposure here.

However, ensure the service is suitable for eventual commands/tools such as:

```text
awh fs insert
awh fs delete-range
filesystem.insert
filesystem.delete_range
```

Those interfaces must eventually translate requests into the canonical edit service and must not reproduce the semantics independently. AWH's target CLI explicitly places `fs insert` and `fs delete-range` in the agent-grade editing layer. fileciteturn34file0

## 28. Security Requirements

AWH is security-first and fail-closed.

The implementation must never:

- bypass workspace containment;
- bypass expected-state checks;
- silently clamp invalid line numbers;
- silently overwrite stale content;
- downgrade an atomic write to a direct unsafe write merely to make a test pass;
- expose secrets in errors/logs/tests;
- introduce a transport path that bypasses the service security boundary.

The canonical architecture requires explicit policy/security boundaries and shared application services rather than interface-specific behavior. fileciteturn30file0

## 29. Performance and Resource Behavior

Do not optimize prematurely, but do avoid accidental quadratic work.

For the first production implementation, an in-memory transformation is acceptable within the existing documented file-size limits.

Prefer:

```text
one read
→ one deterministic parse
→ one transformation
→ one atomic write
```

over repeated full-file scans for every line operation.

Do not claim a performance target unless measured against a documented workload.

## 30. Documentation Requirements

Update documentation only when the implementation changes an established public/domain contract.

At minimum, the code and tests must make the following unambiguous:

- line numbering convention;
- insert boundary semantics;
- delete range inclusivity;
- empty-file behavior;
- EOF behavior;
- newline policy;
- expected-state behavior;
- error categories.

Do not modify unrelated documentation or future issue prompts as part of AWE-003.

## 31. Verification Commands

Run the complete repository gates relevant to the Rust branch:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Then run the focused AWE-003 tests and real temporary-filesystem tests explicitly.

If the repository contains platform-specific test requirements, run the available local checks and clearly report unavailable platforms rather than claiming cross-platform verification.

## 32. Diff and Scope Audit

Before completion:

```bash
git status --short
git diff --check
git diff --stat
git diff
```

Confirm that:

- only intended implementation/test files changed;
- no generated artifacts were committed;
- no unrelated refactor slipped in;
- no duplicate filesystem/edit abstraction was introduced;
- no AWE-004+ feature was implemented accidentally.

## 33. Definition of Done

AWE-003 is complete only when all are true:

- [ ] production insert executor exists;
- [ ] production inclusive delete-range executor exists;
- [ ] one canonical line-numbering convention is documented and tested;
- [ ] insert boundary/EOF semantics are deterministic;
- [ ] delete range semantics are inclusive and deterministic;
- [ ] empty-file behavior is explicit;
- [ ] expected hash/context/size/line-count validation occurs before mutation;
- [ ] stale state produces a structured conflict;
- [ ] invalid ranges produce structured errors;
- [ ] canonical path/security helpers are reused;
- [ ] canonical atomic-write boundary is reused;
- [ ] no mutation occurs on validation/precondition failure;
- [ ] before/after `FileState` is accurate;
- [ ] UTF-8/Devanagari/emoji cases pass;
- [ ] LF/CRLF/trailing-newline cases pass;
- [ ] real temporary-filesystem integration tests exist;
- [ ] relevant property tests exist where practical;
- [ ] AWE-004 can reuse the line-edit primitives without duplication;
- [ ] no CLI/MCP/TUI/snapshot/worktree/orchestration scope leaked into the issue;
- [ ] formatting/check/tests/Clippy pass;
- [ ] final diff has been manually inspected.

## 34. Required Final Report

When the implementation is complete, report:

```text
AWE-003 implementation summary

Files changed:
- ...

Insert semantics:
- ...

Delete semantics:
- ...

Line-numbering convention:
- ...

Expected-state enforcement:
- ...

Newline/UTF-8 policy:
- ...

Tests added:
- ...

Verification:
- cargo fmt ...
- cargo check ...
- cargo test ...
- cargo clippy ...

Known limitations:
- ...

AWE-004 readiness:
- ...

Git diff reviewed:
- yes/no
```

Do not claim a test or platform verification that was not actually run.

## 35. Hard Stop

After AWE-003 is implemented, tested, and verified:

**STOP.**

Do not automatically continue to AWE-004 or modify the next issue-resolution prompt.

The next issue must be started only by an explicit instruction from the repository owner.

AWE-003 should leave AWH with one reliable, reusable line-editing primitive that follows the same safety model as contextual replacement and is ready to become a building block for the later multi-operation editing transaction.