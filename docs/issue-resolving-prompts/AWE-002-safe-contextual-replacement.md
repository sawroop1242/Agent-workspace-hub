# AWE-002 / #23 — Safe Contextual Replacement

> **Type:** P0 agent-grade editing foundation  
> **Dependency:** AWE-001 / #22  
> **Primary scope:** production safe replacement executor  
> **Branch:** `rust`  
> **Status:** Issue-resolution master prompt  

## 1. Mission

Implement the first production-grade agent editing primitive for AWH: a **safe, contextual, conflict-aware replacement operation** built on the canonical AWE-001 edit transaction model.

The implementation must never silently overwrite stale or unexpectedly changed file content. It must validate the target state completely before mutation, prepare the complete replacement in memory, perform one canonical atomic filesystem write, and return accurate before/after state information.

This is the foundation that later AWE issues will consume. Do not solve later milestones inside this issue.

The implementation must fit the AWH architecture described by:

- `docs/PROJECT_CONTEXT.md`
- `docs/PROJECT_ROADMAP.md`
- `docs/PROJECT_ROADMAP_STATUS.md`
- `docs/PROJECT_STATUS.md`
- `docs/FEATURES.md`
- `docs/CLI.md`
- `docs/architecture.md`
- `docs/mcp.md`

The roadmap explicitly places agent-grade editing after `EditTransaction` and before snapshots/provenance/rollback. All editing interfaces are eventually required to use one shared edit service rather than implementing their own semantics.

## 2. Read Before Editing

Before changing code, inspect the current `rust` branch and understand the existing implementation rather than creating parallel abstractions.

At minimum inspect:

```text
src/services/edit.rs
src/services/
src/core/
src/mcp/
src/models/
Cargo.toml
```

Also inspect the existing filesystem/path-security implementation and any tests covering canonical workspace containment, atomic writes, symlink handling, UTF-8, and file metadata.

Search the entire repository for:

```text
EditTransaction
EditOperation::Replace
ExpectedState
FileState
validate_path
atomic write
canonical path
workspace root
filesystem service
```

Do not assume a filename from documentation is still authoritative. Verify the actual branch tree before adding or moving code.

## 3. Current Forensic Reality

AWE-001 established the canonical domain vocabulary in `src/services/edit.rs`, including `EditId`, `EditOperation::Replace`, `ExpectedState`, `FileState`, `EditStatus`, `EditTransaction`, and structured edit errors. There was no production executor when this issue was defined.

The important AWE-001 contract is:

```text
EditTransaction
    ↓
EditOperation::Replace
    ↓
ExpectedState
    ↓
FileState before mutation
    ↓
validated mutation
    ↓
FileState after mutation
```

The existing `ExpectedState.context` field is not allowed to be decorative. AWE-002 is the first issue that must give that field real enforcement semantics.

Do not re-create the AWE-001 model in another module. Extend or consume the canonical model instead.

## 4. Product Contract

The target replacement flow is:

```text
caller
  ↓
canonical EditTransaction
  ↓
validate transaction shape
  ↓
resolve/validate workspace-relative path
  ↓
read current file
  ↓
construct actual FileState
  ↓
validate expected hash/size/line-count/context
  ↓
locate exact replacement matches
  ↓
validate occurrence semantics
  ↓
prepare complete resulting content in memory
  ↓
perform canonical atomic write
  ↓
re-read/observe resulting state as required
  ↓
return before + after FileState
```

Any failure before the atomic write must leave the target file unchanged.

The implementation must not mutate the file and then discover that an expected-state or match condition was invalid.

## 5. Required Production API

Create or extend a service boundary appropriate to the existing architecture. Prefer an existing application/filesystem service if one already owns file mutations.

The service must provide a reusable replacement operation that can be consumed by later edit layers.

Conceptually the operation is:

```rust
replace(transaction: EditTransaction) -> Result<EditResult, EditError>
```

The exact Rust signature must follow the repository's existing service conventions rather than being copied blindly from this prompt.

The result must expose enough structured information for later milestones to consume:

- edit/transaction identity where available
- target path
- operation result
- before `FileState`
- after `FileState` on success
- deterministic error/conflict information on failure

Do not expose transport-specific MCP or CLI types from this service.

## 6. Exact Replacement Semantics

### 6.1 Read the current file first

Read the target file as bytes and decode it according to the repository's existing text-file conventions.

The operation is a text replacement primitive. Do not silently reinterpret arbitrary binary data as text.

If the existing filesystem abstraction has a documented text/binary policy, follow it. If it does not, fail clearly rather than inventing an undocumented binary mutation behavior.

### 6.2 Exact old-content matching

`old` must be matched exactly.

Do not:

- trim whitespace
- normalize Unicode
- normalize line endings before matching
- perform case-insensitive matching
- collapse whitespace
- perform fuzzy matching
- silently interpret regex syntax

The caller's `old` string is literal content unless the existing AWE-001 contract explicitly defines another behavior.

### 6.3 Occurrence semantics

AWE-001 provides an optional `occurrence` field for `Replace`.

Define and enforce deterministic semantics:

- `occurrence = None`: replacement is valid only when exactly one match exists.
- `occurrence = Some(n)`: replace the explicitly selected occurrence using the repository's documented indexing convention.
- zero matches: structured failure.
- multiple matches with no explicit occurrence: structured ambiguity failure.
- selected occurrence outside the available match range: structured failure.

Do not silently replace all matches unless the canonical model is explicitly changed in a later architectural decision. This issue must remain conservative and safe.

If the existing model or tests already establish an occurrence indexing convention, preserve it. If not, document and test the chosen convention clearly.

### 6.4 Match offsets

When reporting match failures, provide deterministic location information where practical:

- byte offset
- character/Unicode-aware position where useful
- line number
- column/offset within the line where deterministically calculable
- total match count

Do not report misleading character offsets when the implementation actually uses byte offsets. Name the coordinate system explicitly.

## 7. Expected-State Enforcement

Expected state is a precondition, not metadata.

### 7.1 Hash

If `ExpectedState.hash` is present:

1. read the current file;
2. calculate its canonical SHA-256 using the existing AWH helper/model;
3. compare it before mutation;
4. reject the operation on mismatch.

The operation must never continue to matching or writing after a failed required hash precondition unless the architecture explicitly defines independent diagnostic validation without mutation.

### 7.2 Size

If `ExpectedState.size` is present, compare it against the current `FileState.size` before mutation.

Mismatch must produce a deterministic conflict/precondition error.

### 7.3 Line count

If `ExpectedState.line_count` is present, compare it against the current `FileState.line_count` before mutation.

Mismatch must produce a deterministic conflict/precondition error.

### 7.4 Context

`ExpectedState.context` must be enforced.

The implementation must define the context contract precisely and test it. At minimum, the context must be verified against the current file before the mutation and must not be treated as a field that can be ignored.

The implementation must answer deterministically:

- Is context required to occur exactly once?
- Is context a literal substring or a structured location assertion?
- What happens when context is absent?
- What happens when context occurs multiple times?
- How is context related to `old`?
- What location information is returned on context failure?

Prefer a semantics that prevents stale or ambiguous edits rather than weakening the safety guarantee.

If the existing repository or AWE-001 implementation establishes a context contract, preserve it. If the contract is still incomplete, implement the smallest coherent contract that makes `context` an actual precondition and document it in code/tests. Do not expand this issue into a general contextual-search engine.

### 7.5 Conflict result

Hash/context/size/line-count mismatches must return a structured conflict/precondition result containing enough information to diagnose:

```text
expected state
actual state
path
failure category
relevant location/context information when safe
```

Never expose private file contents unnecessarily in errors.

## 8. Validation Ordering

Validation ordering is part of the safety contract.

Use an order equivalent to:

```text
1. Validate transaction shape.
2. Validate operation type.
3. Validate path syntax and workspace containment.
4. Read current content.
5. Build actual FileState.
6. Validate expected hash/size/line_count/context.
7. Locate literal old-content matches.
8. Validate occurrence semantics.
9. Prepare complete resulting content in memory.
10. Perform atomic write through the canonical filesystem boundary.
11. Verify resulting state.
12. Return structured result.
```

Do not write before all preconditions have passed.

## 9. No-Mutation-on-Failure Invariant

This is mandatory.

For every failure before the atomic commit boundary:

```text
file bytes before == file bytes after
```

Test this explicitly.

Failure cases must include at least:

- invalid path
- missing file
- unreadable file
- invalid transaction shape
- empty old string
- zero matches
- unexpected multiple matches
- invalid selected occurrence
- expected hash mismatch
- expected context mismatch
- expected size mismatch
- expected line-count mismatch
- invalid UTF-8 according to the chosen file policy
- malformed replacement request
- atomic write preparation failure

Do not rely only on an absence of an obvious write call. Tests must compare actual file contents/state.

## 10. Atomic Write Requirements

After every precondition and replacement calculation succeeds, write the complete new file through the **existing canonical filesystem service**.

Do not create a second filesystem implementation.

Do not directly scatter `std::fs::write` or equivalent mutation calls throughout the edit service if AWH already has a canonical atomic-write boundary.

The write must preserve the existing filesystem security contract, including workspace containment and the repository's established atomic replacement behavior.

The implementation must clearly distinguish:

```text
prepare in memory
        ≠
commit to filesystem
```

A failed preparation must never partially modify the target.

## 11. Path Security

Reuse the canonical workspace path validation/containment primitive.

Reject at the canonical filesystem boundary:

- absolute paths
- `..` traversal
- root/prefix escapes
- paths escaping the active workspace
- symlink-based escapes where the existing security layer detects them

Do not add another independent `validate_path` or containment algorithm merely for AWE-002.

The edit service should pass a validated workspace-relative resource into the canonical filesystem layer.

### TOCTOU limitation

The forensic work identified a final-component TOCTOU concern in filesystem path handling.

AWE-002 must account for this concern where the existing architecture permits, but **must not claim race-free filesystem security unless a tested OS-level guarantee actually exists**.

If the current filesystem layer cannot provide a stronger guarantee yet:

- preserve its current security behavior;
- do not weaken it;
- document the remaining limitation in the implementation/tests if relevant;
- leave stronger safe-open/race-resistant primitives to the dedicated filesystem hardening work.

Do not solve the entire TOCTOU problem inside this issue unless it is required to safely integrate the existing canonical write boundary.

## 12. Before/After FileState

Before mutation, construct the actual `FileState` from the bytes currently read.

After successful mutation, obtain the resulting state from the resulting content or a verified post-write observation.

The hashes must represent the real file state.

Tests must verify:

```text
before.hash == SHA256(original bytes)
after.hash  == SHA256(resulting bytes)
```

Also verify size and line count according to the canonical `FileState` semantics.

Do not fabricate `after` state from an assumed transformation without verification when the filesystem boundary permits a post-write observation.

## 13. Transaction Lifecycle

Use the AWE-001 lifecycle vocabulary rather than inventing a second state machine.

At minimum, the implementation should move through the appropriate states for:

```text
Requested
→ Located
→ Validated
→ Applied
→ Verified
→ Committed
```

Exact state transitions must reflect the actual implementation. Do not mark a transaction `Applied`, `Verified`, or `Committed` when the corresponding operation did not occur.

Failure paths should use the existing structured failure/conflict states rather than inventing transport-specific statuses.

Do not implement snapshot or rollback lifecycle stages here beyond preserving compatibility with AWE-001's vocabulary.

## 14. Error Contract

Errors must be structured and machine-readable.

At minimum distinguish:

```text
invalid transaction
invalid path
file not found
read failure
invalid/unsupported text content
empty match
zero matches
ambiguous match
invalid occurrence
expected-state conflict
context conflict
write failure
verification failure
```

Do not use string-only errors where callers need to branch on the failure category.

Use the repository's established `thiserror`/domain-error conventions.

Include safe diagnostic fields where appropriate:

```text
path
operation/edit id
expected state summary
actual state summary
match count
location
```

Never include secrets or unrelated private data.

## 15. Unicode and Line-Ending Semantics

The implementation must explicitly handle:

- UTF-8
- Unicode characters
- Devanagari
- emoji
- ASCII
- CRLF files
- LF files
- files without a trailing newline
- files with a trailing newline
- empty files
- one-line files
- repeated replacement text

Literal matching must preserve the original content outside the selected replacement.

Do not silently normalize the entire file from CRLF to LF merely because a replacement occurred.

For example, if the file is CRLF and only a substring changes, the resulting file should preserve the existing line-ending representation outside the replacement.

Do not use byte slicing at arbitrary character boundaries. If offsets are byte offsets, ensure all slicing boundaries are valid UTF-8 boundaries.

## 16. Large Files and Limits

Respect the repository's existing filesystem/read/write limits and configuration.

Do not introduce an unlimited read-all/write-all path if the canonical filesystem layer already defines bounded operations.

For the first production primitive, it is acceptable to operate in memory only within the existing documented limits.

If the file exceeds the supported safe edit size, fail deterministically before mutation rather than silently switching to an unsafe implementation.

Do not implement streaming patching in AWE-002 unless it is already required by the existing filesystem architecture.

## 17. Concurrency and Stale State

The core safety property is that an edit based on stale state must not silently overwrite newer content.

At minimum:

```text
read current state
→ validate expected state
→ prepare replacement
→ commit atomically
```

If the existing filesystem architecture provides a locking or mutation-coordination primitive, use it rather than creating another one.

If it does not, document the remaining race window honestly and do not claim full concurrent compare-and-swap semantics.

AWE-007 and filesystem TOCTOU work will strengthen these guarantees later. AWE-002 must establish the correct precondition/validation boundary without pretending to implement all future concurrency machinery.

## 18. Testing Strategy

Add tests at the appropriate unit and real-filesystem integration levels.

### 18.1 Domain/unit tests

Test:

- exact single replacement
- zero match
- multiple matches without occurrence
- selected occurrence
- invalid occurrence
- empty old string
- expected hash match
- expected hash mismatch
- expected size match/mismatch
- expected line-count match/mismatch
- expected context match/mismatch
- deterministic error categories
- before/after `FileState`

### 18.2 Real temporary-filesystem tests

Use temporary directories/files and exercise the actual production service.

At minimum cover:

1. successful replacement;
2. zero-match failure;
3. ambiguous-match failure;
4. explicit occurrence success;
5. stale hash conflict;
6. stale context conflict;
7. stale size/line-count conflict;
8. invalid traversal path;
9. absolute path;
10. missing file;
11. Unicode/Devanagari/emoji;
12. CRLF;
13. LF;
14. missing trailing newline;
15. empty file;
16. repeated matches;
17. no mutation after every pre-write failure;
18. correct before/after hashes;
19. atomic write failure handling where realistically testable;
20. workspace containment/symlink behavior using the existing security boundary.

### 18.3 Property-oriented tests

Where practical, add focused property tests for replacement invariants such as:

```text
successful replacement preserves every non-selected byte sequence
failed validation leaves original content unchanged
reported after hash equals resulting content hash
```

Do not create a large fuzzing framework solely for this issue if the repository already has suitable property-test infrastructure that can be extended.

## 19. Regression Tests for the Canonical Model

AWE-002 must remain compatible with AWE-001.

Do not duplicate `EditTransaction`, `ExpectedState`, `FileState`, or `EditOperation` definitions.

Add only the tests required to prove the production executor consumes the canonical model correctly.

The implementation must compile with all existing AWE-001 tests intact.

## 20. Architecture Boundaries — Strictly Enforced

### This issue MAY modify

- the canonical edit service required for replacement;
- closely related domain error/result types when required for AWE-002;
- the existing filesystem service only where a minimal integration change is required;
- AWE-002-specific tests;
- narrowly necessary Rust module exports/imports.

### This issue MUST NOT implement

- MCP replacement tools;
- CLI `fs replace` command;
- TUI editing UI;
- snapshots;
- persistent provenance;
- persistent audit;
- rollback;
- worktrees;
- agent orchestration;
- multi-agent collaboration;
- capability architecture;
- the complete PolicyEngine;
- Git integration;
- remote APIs;
- connector architecture;
- general-purpose diff application;
- line insertion/deletion primitives owned by AWE-003;
- multi-operation filesystem patching owned by AWE-004;
- stale-state coordination owned by AWE-007;
- full snapshot/provenance implementation owned by later milestones.

If an apparently necessary feature belongs to a later issue, implement the smallest compatible seam and stop there.

## 21. Reuse Rules

Before adding any helper, search for an existing implementation.

Prefer existing primitives for:

- workspace root resolution;
- canonical path validation;
- filesystem reads;
- atomic writes;
- SHA-256 hashing;
- line counting;
- structured errors;
- temporary test workspaces.

Do not create:

```text
second path validator
second workspace resolver
second atomic writer
second edit transaction type
second error hierarchy
second file-state implementation
```

AWE-002 should make the architecture more coherent, not introduce another parallel subsystem.

## 22. Verification Procedure

Run the focused tests first, then the complete repository gates.

Recommended sequence:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also run the relevant focused test target(s) while developing so failures are fast and attributable.

If the repository contains integration/security/property tests related to filesystem editing, run them explicitly.

If platform-specific filesystem behavior is involved, run the available CI/platform matrix rather than claiming universal behavior from one operating system.

## 23. Manual Terminal Validation

This issue is not complete from compilation alone.

Create a temporary workspace and manually exercise the production replacement service through the supported developer-facing entry point if one exists.

Validate at least:

```text
successful replacement
stale-state rejection
ambiguous replacement rejection
explicit occurrence replacement
Unicode replacement
CRLF/LF preservation
traversal rejection
```

If no CLI/API surface exists yet because that belongs to a later milestone, invoke the service through a narrowly scoped integration test/example rather than prematurely adding a CLI command.

Do not add a permanent user-facing command solely to demonstrate AWE-002.

## 24. Diff Review

Before completion:

```bash
git status --short
git diff --check
git diff -- src/...
git diff -- tests/...
```

Review for:

- accidental unrelated edits;
- duplicate filesystem logic;
- weakened path validation;
- silent stale-state overwrite;
- incorrect Unicode handling;
- newline normalization;
- incorrect occurrence indexing;
- misleading error messages;
- unnecessary dependencies;
- security regressions;
- changes to later issue scope.

## 25. Documentation/Code Comments

Document non-obvious safety contracts directly next to the implementation, especially:

- occurrence semantics;
- contextual expectation semantics;
- validation-before-write ordering;
- atomic commit boundary;
- stale-state conflict behavior;
- any known TOCTOU limitation.

Do not add broad roadmap claims or mark later features as implemented.

## 26. Definition of Done

AWE-002 is complete only when all of the following are true:

- [ ] A production replacement executor exists behind a reusable service boundary.
- [ ] It consumes the canonical AWE-001 transaction model.
- [ ] Exact literal replacement is implemented.
- [ ] Zero-match behavior is deterministic.
- [ ] Multiple-match behavior is deterministic.
- [ ] Explicit occurrence selection is deterministic and tested.
- [ ] Expected hash is enforced before mutation.
- [ ] Expected size is enforced when supplied.
- [ ] Expected line count is enforced when supplied.
- [ ] Expected context is enforced and documented; it is not ignored.
- [ ] Conflict results distinguish expected and actual state safely.
- [ ] Match failure locations are deterministic where applicable.
- [ ] All validation occurs before the filesystem mutation boundary.
- [ ] The canonical workspace path/security boundary is reused.
- [ ] No second filesystem containment implementation is introduced.
- [ ] Replacement content is prepared completely in memory before commit.
- [ ] Successful mutation uses the canonical atomic-write mechanism.
- [ ] Before `FileState` is accurate.
- [ ] After `FileState` is accurate.
- [ ] No validation/conflict failure mutates the target file.
- [ ] UTF-8, Devanagari, emoji and newline edge cases are tested.
- [ ] Real temporary-filesystem integration tests exist.
- [ ] Relevant regression/property tests pass.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo check --all-targets` passes.
- [ ] `cargo test --all-targets` passes.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes.
- [ ] `git diff --check` passes.
- [ ] No unrelated issue-resolution scope was implemented.
- [ ] Final diff contains only the intended AWE-002 implementation/tests and narrowly required integration changes.

## 27. Final Implementation Report

At the end of the implementation, report:

```text
AWE-002 RESULT

Implemented:
- ...

Files changed:
- ...

Replacement semantics:
- ...

Expected-state semantics:
- hash: ...
- size: ...
- line_count: ...
- context: ...

Security guarantees:
- ...

Known limitations:
- ...

Tests added:
- ...

Verification:
- cargo fmt: PASS/FAIL
- cargo check: PASS/FAIL
- cargo test: PASS/FAIL
- cargo clippy: PASS/FAIL
- git diff --check: PASS/FAIL

Scope audit:
- No MCP changes: PASS/FAIL
- No CLI changes: PASS/FAIL
- No snapshots/rollback implementation: PASS/FAIL
- No unrelated files changed: PASS/FAIL

Ready for AWE-003: YES/NO
```

If any required verification fails, report the exact failure and do not claim AWE-002 is complete.

## 28. Hard Stop Rules

Stop and reassess instead of guessing if:

- the canonical filesystem service cannot safely perform the required atomic write;
- the workspace containment contract is unclear;
- AWE-001's context semantics conflict with the implementation;
- an existing helper appears to duplicate the proposed implementation;
- the replacement requires changing an unrelated subsystem;
- a security invariant would need to be weakened;
- the operation would require silently rewriting stale content;
- a later issue must be implemented to make AWE-002 appear complete.

Do not work around an architectural conflict by creating a parallel implementation.

## 29. Final Instruction to the Implementing Agent

Implement **only AWE-002 / #23**.

Treat this document as the execution contract, but verify every assumption against the current `rust` branch before coding.

The objective is not merely to make a string replacement test pass. The objective is to establish the first trustworthy AWH edit mutation boundary:

```text
canonical transaction
    ↓
current-state observation
    ↓
strict expected-state validation
    ↓
exact contextual replacement
    ↓
complete in-memory preparation
    ↓
canonical atomic commit
    ↓
verified before/after state
```

Never trade correctness or security for convenience. Never silently overwrite stale state. Never duplicate an existing AWH primitive. Never implement later milestones under the name of AWE-002.

When AWE-002 is fully verified, **STOP**. Do not modify AWE-003 or any other issue-resolution prompt.