# AWE Sequential Implementation Master Prompt

## Purpose

Use this document as the master instruction for an AI coding agent working on the Agent Workspace Hub repository.

The agent must execute the existing AWE issue-resolving prompts **one by one in dependency order**, not attempt all changes as one uncontrolled rewrite.

Target sequence:

```text
AWE-001 / #22
      ↓
AWE-002 / #23
      ↓
AWE-003 / #24
      ↓
AWE-004 / #25
```

The repository's current code is authoritative. Existing implementations may differ from the original prompts. Adapt carefully rather than blindly overwriting working code.

---

# 1. Role and architectural boundary

You are implementing the AWH editing subsystem as an **MCP-first, agent-agnostic workspace infrastructure component**.

AWH is not the agent.

Do not add:

- LLM reasoning
- autonomous planning
- model providers
- prompt orchestration
- autonomous conflict resolution
- agent workflow logic

External agents decide what should change. AWH safely executes and verifies requested workspace mutations.

The intended architecture is:

```text
External Agent
      ↓
MCP / CLI / TUI / Control API
      ↓
EditService
      ↓
Capability + Policy boundary
      ↓
Workspace filesystem
```

For this sequence, implement the core editing service only. Do not prematurely add MCP, CLI, TUI, snapshots, audit, or rollback unless the current issue explicitly requires it.

---

# 2. Mandatory execution order

Execute these documents exactly in order:

1. `docs/issue-resolving-prompts/AWE-001-canonical-edit-transaction-model.md`
2. `docs/issue-resolving-prompts/AWE-002-safe-contextual-replacement.md`
3. `docs/issue-resolving-prompts/AWE-003-line-range-insert-delete.md`
4. `docs/issue-resolving-prompts/AWE-004-multi-operation-filesystem-patch.md`

Do not start the next prompt until the current prompt's implementation, tests, review, and verification are complete.

Before each prompt:

1. Read the entire prompt.
2. Inspect the current repository state.
3. Inspect the previous implementation.
4. Inspect related tests.
5. Identify what is already implemented.
6. Make the smallest coherent changes needed.

Never assume an earlier prompt was implemented exactly as written.

---

# 3. Repository inspection protocol

Before changing anything, inspect at minimum:

```text
AGENTS.md
Cargo.toml
src/services/mod.rs
src/services/edit.rs
src/services/files.rs
```

Also inspect relevant tests, service conventions, error handling, serialization, filesystem helpers, and any existing editing implementation.

Search for:

```text
EditTransaction
EditOperation
ExpectedState
FileState
EditStatus
EditError
EditService
```

Do not duplicate existing abstractions.

Prefer extension/refactoring over replacement.

---

# 4. Editing strategy

Use **small, reviewable patches**.

Do not rewrite complete source files merely because a new function is needed.

Preferred process:

```text
inspect
→ identify exact insertion/change points
→ patch minimally
→ format
→ compile
→ test
→ review diff
→ commit
```

Preserve unrelated code and existing behavior.

Do not modify unrelated modules.

Do not introduce speculative abstractions.

---

# 5. Prompt-specific execution

## Stage 1 — AWE-001 / #22

Implement the canonical transport-independent edit model.

Verify:

- `EditId`
- `EditOperation`
- `ExpectedState`
- `FileState`
- `EditStatus`
- `EditTransaction`
- structured `EditError`
- serialization where appropriate
- structural validation
- focused unit tests

Do not implement filesystem mutation in this stage.

Gate:

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Review the diff before moving on.

---

## Stage 2 — AWE-002 / #23

Implement safe contextual replacement using the canonical model.

Requirements:

- exact deterministic matching
- occurrence semantics
- zero-match error
- ambiguous-match protection
- expected-state validation
- stale-state conflict detection
- path security through `FilesService`
- in-memory preparation before mutation
- safe controlled write
- before/after state
- post-write verification
- UTF-8/newline handling
- focused tests

Do not add MCP/CLI/TUI/snapshot/audit/policy/rollback systems.

Gate:

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Review the diff and run relevant tests before proceeding.

---

## Stage 3 — AWE-003 / #24

Implement safe line-range insertion and deletion.

Requirements:

- deterministic 1-based line semantics
- explicit insertion behavior
- inclusive deletion range
- EOF behavior
- expected-state validation
- stale-state protection
- UTF-8/Devanagari/emoji
- LF/CRLF
- trailing/no trailing newline
- invalid range rejection without mutation
- focused tests

Reuse the canonical edit model and `FilesService` security boundary.

Do not add later milestone systems.

Gate:

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Review the diff before AWE-004.

---

## Stage 4 — AWE-004 / #25

Implement multi-operation `filesystem.patch`.

Primary invariant:

> Prepare and validate every operation and every affected file before mutating any file.

The architecture must be:

```text
EditTransaction
      ↓
validate
      ↓
load all required files
      ↓
capture FileState
      ↓
validate expected states
      ↓
apply operations in memory, in transaction order
      ↓
prepare final state for every affected file
      ↓
commit final mutations
      ↓
verify
      ↓
return result
```

Requirements:

- reuse AWE-001 canonical types
- reuse AWE-002 replacement semantics
- reuse AWE-003 insert/delete semantics
- do not duplicate operation algorithms
- pure in-memory transformation helpers
- same-file sequential composition
- multi-file preparation
- one final mutation per affected file
- deterministic operation ordering
- expected-state validation against the logical state observed by each operation
- stale-state conflict detection
- existing `FilesService` path security
- no mutation during preparation
- no partial mutation from preparation failure
- structured result
- minimal post-commit verification

Critical test:

```text
A = valid operation
B = invalid operation
C = valid operation

Expected:
A unchanged
B unchanged
C unchanged
```

Do not claim crash-safe transaction rollback in AWE-004. That belongs to AWE-006 / #27.

Do not implement MCP/CLI/TUI/snapshot/audit/policy/rollback here.

Use the dedicated documents:

```text
AWE-004-implementation-plan.md
AWE-004-coding-checklist.md
AWE-004-multi-operation-filesystem-patch.md
```

---

# 6. Mandatory test philosophy

Tests must validate behavior, not merely implementation details.

For each stage include:

1. happy path
2. invalid input
3. conflict/stale state
4. filesystem security
5. Unicode/newline edge cases
6. no-mutation-on-failure invariant where applicable

For AWE-004 specifically, test same-file composition and multi-file preparation safety.

Prefer real temporary filesystem integration tests for mutation behavior rather than mocks that cannot detect partial writes.

---

# 7. Required verification after every stage

Run:

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Also inspect:

```bash
git status
git diff --stat
git diff
```

If a command fails:

1. diagnose the actual failure
2. fix the smallest relevant cause
3. rerun the failed command
4. rerun the full gate
5. only then continue

Never report success based on an unrun command.

---

# 8. Commit discipline

Create one focused commit per completed AWE stage.

Recommended commit messages:

```text
feat(edit): add canonical edit transaction model
feat(edit): add safe contextual replacement
feat(edit): add safe line-range insert and delete operations
feat(edit): add multi-operation filesystem patch
```

Do not combine unrelated cleanup with these commits.

If the repository's contribution rules specify a different format, follow those rules.

---

# 9. Stop conditions

Stop before proceeding to the next stage if:

- compilation fails
- tests fail
- clippy fails
- formatting fails
- a security invariant is broken
- expected-state conflict behavior is ambiguous
- existing semantics would be silently changed
- a requirement requires architecture belonging to a later AWE issue

Do not work around failures by weakening tests or disabling lints.

Do not delete tests merely to make the suite pass.

---

# 10. Final AWE-004 acceptance workflow

At the end, demonstrate the complete core workflow:

```text
create temporary workspace
      ↓
create multiple files
      ↓
construct EditTransaction
      ↓
include multiple operations
      ↓
include same-file sequential operations
      ↓
include multiple files
      ↓
prepare patch
      ↓
validate expected states
      ↓
apply all transformations in memory
      ↓
commit final contents
      ↓
verify final FileState
      ↓
confirm expected result
```

Then execute a deliberate failure case:

```text
valid file A
invalid operation on file B
valid file C
```

Confirm that A and C were **not changed**.

Also execute at least one stale-state conflict and confirm that no AWE-004 preparation mutation occurs.

---

# 11. Final report

After all four stages, report:

## Implemented

List each completed AWE stage and the important functions/modules changed.

## Tests

List the exact commands run and their outcomes.

## Security

Explain path traversal, symlink, expected-state, and no-partial-preparation protections verified.

## Behavioral validation

Describe the real temporary-workspace workflow and failure scenarios tested.

## Files changed

List every changed file.

## Commits

List the commit SHA and message for each AWE stage.

## Remaining work

Explicitly state what remains for later issues, especially:

- AWE-005 unified diff
- AWE-006 crash-safe atomic/rollback behavior
- AWE-009 MCP exposure
- AWE-011 capability/policy enforcement
- AWE-012 snapshots/provenance
- AWE-013 audit
- AWE-014 CLI
- AWE-015 broader test suite

Do not claim those features are complete merely because the core edit service exists.

---

# 12. Absolute rules

1. Repository state is authoritative.
2. Execute prompts sequentially.
3. Never skip verification.
4. Never silently overwrite stale content.
5. Never mutate during AWE-004 preparation.
6. Never duplicate single-operation algorithms unnecessarily.
7. Never bypass `FilesService` path security.
8. Never introduce unrelated architecture.
9. Never weaken tests to obtain green CI.
10. Never claim a test passed unless it was actually run.
11. Prefer minimal patches over full-file rewrites.
12. Keep AWH MCP-first and agent-agnostic.
13. Keep later milestone responsibilities out of earlier issues.
14. Preserve deterministic operation ordering.
15. Treat no-partial-preparation as a hard safety invariant.
