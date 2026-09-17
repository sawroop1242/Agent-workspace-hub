# AWH Agent Task Template

> Reusable template for external agents implementing, repairing, testing, or reviewing Agent Workspace Hub (AWH).

## 1. Task Identity

- **Task ID:** `<TASK-ID>`
- **Task type:** `feature | bugfix | refactor | security | test | docs | benchmark | review`
- **Priority:** `low | medium | high | critical`
- **Base branch:** `rust`
- **Base revision:** `<COMMIT-SHA>`
- **Agent branch:** `agent/<TASK-ID>`
- **Related issue/PR:** `<ISSUE-OR-PR>`

## 2. Objective

### Problem

Describe the concrete problem or missing behavior.

### Required outcome

Describe exactly what must be true when the task is complete.

### Non-goals

List behavior, files, refactors, or integrations that are explicitly outside this task.

## 3. Scope

### In scope

- `<change 1>`
- `<change 2>`
- `<test/documentation change>`

### Out of scope

- Unrelated refactors
- API changes not required by the task
- Changes to unrelated security boundaries
- Adding external agent/evaluation tools as AWH runtime dependencies

Keep the implementation focused. Do not expand scope because an unrelated improvement is discovered during implementation. Record unrelated findings separately.

## 4. Required Repository Inspection

Before editing, inspect the following in this order as applicable:

1. `docs/PROJECT_CONTEXT.md`
2. `docs/PROJECT_ROADMAP.md`
3. `docs/PROJECT_ROADMAP_STATUS.md`
4. `docs/development.md`
5. `docs/security.md`
6. `docs/error.md`
7. `docs/issue-resolving-prompts/README.md`
8. The issue/task-specific documentation
9. Relevant `src/` modules
10. Relevant unit and integration tests
11. Relevant `.github/workflows/` files
12. Current `git status`, diff, and recent history when behavior is ambiguous

Also inspect any narrower documentation explicitly referenced by the task.

### Files/modules to inspect

- `<path>
- `<path>`
- `<path>`

### Tests to inspect

- `<test path/name>`
- `<test path/name>`

Do not assume a roadmap or agent prompt describes the current implementation accurately. Verify behavior against source and tests.

## 5. Implementation Contract

### Required behavior

- `<requirement 1>`
- `<requirement 2>`
- `<requirement 3>`

### Interfaces affected

- CLI: `<none / commands>`
- MCP: `<none / methods>`
- Filesystem: `<none / paths or operations>`
- Network/API: `<none / endpoints>`
- Configuration: `<none / variables or files>`
- Public Rust API: `<none / symbols>`

### Compatibility requirements

- Preserve existing public behavior unless the task explicitly changes it.
- Preserve stable error/protocol behavior unless a contract change is required.
- Avoid unnecessary dependency additions.
- Do not introduce external development tools into AWH runtime dependencies.

## 6. Security Constraints

The task must preserve AWH's current security model.

### Mandatory invariants

- Deny by default for privileged/high-risk operations.
- Centralized authorization and permission gates remain enforced.
- Filesystem access cannot escape permitted bases through traversal or symlink attacks.
- Subprocess execution remains bounded and sandboxed where required.
- MCP request/body/message limits remain enforced.
- Timeouts and resource limits remain enforced.
- Authentication requirements for remote access remain intact.
- Fail-closed behavior is preserved when security controls cannot be applied.
- Secrets must never appear in source, logs, errors, prompts, patches, or artifacts.
- User-controlled and external input must be validated at security boundaries.

### Security-specific tests

- `<test/command>`
- `<test/command>`

If a requested change conflicts with a security invariant, stop and report the conflict instead of weakening the invariant.

## 7. Required Verification

Run the mandatory AWH verification commands from the repository root:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

For dependency or security-sensitive changes, also run:

```bash
cargo audit
```

For relevant task-specific verification, run:

```text
<additional command 1>
<additional command 2>
```

### Verification rules

- Every applicable mandatory command must exit with status `0`.
- Do not skip or weaken tests to obtain a green result.
- Do not claim a check passed unless it was actually executed.
- If an environment prevents a check, record the exact limitation and rely on the corresponding CI check when applicable.
- A failed mandatory check blocks completion.

## 8. Regression Requirements

If this is a bug fix:

- Add a deterministic regression test when feasible.
- Confirm the test fails for the old behavior or otherwise demonstrates the fixed contract.
- Confirm the complete test suite remains green.

If this changes public behavior:

- Add or update focused tests.
- Update documentation when the externally observable contract changes.

If this changes security-sensitive behavior:

- Add tests for both the permitted and denied cases where practical.

## 9. External Agent / Benchmark Boundary

External development and evaluation systems may be used to build or evaluate AWH, but remain outside the product:

- mini-SWE-agent / SWE-agent: implementation and repair
- SWE-ReX: isolated execution
- SWE-bench: software-engineering evaluation
- SWE-smith: task generation
- CodeClash: long-running engineering evaluation
- sb-cli: remote benchmark execution

These tools must not be added to AWH `Cargo.toml`, runtime source, or product image merely because an agent workflow uses them.

Model credentials must be supplied through CI secrets and must never be committed, logged, or copied into artifacts.

## 10. Change Review Checklist

Before opening the PR, confirm:

- [ ] Scope is limited to the task.
- [ ] Current source and tests were inspected before editing.
- [ ] Relevant documentation was inspected.
- [ ] Required implementation behavior is present.
- [ ] Regression tests were added/updated where appropriate.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo check --all-targets` passes.
- [ ] `cargo test --all-targets` passes.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo audit` passes when applicable.
- [ ] Security invariants remain intact.
- [ ] No secrets are present in changes or artifacts.
- [ ] No unrelated files were modified.
- [ ] External agent/benchmark tools were not added as AWH runtime dependencies.
- [ ] Final `git diff --check` passes.

## 11. Completion Criteria

The task is complete only when all applicable criteria below are satisfied:

1. The requested behavior is implemented correctly.
2. Relevant tests verify the behavior.
3. Required AWH verification passes, or every environment-limited check is explicitly documented.
4. No security invariant has been weakened.
5. No unrelated changes remain.
6. Documentation is updated when the observable contract changed.
7. The task branch contains focused commits.
8. The branch is pushed without modifying `rust` or `main` directly.
9. A PR is prepared against `rust`.
10. The PR description contains the verification results and any known limitations.

## 12. Agent Completion Report

Use this structure in the final task/PR report:

```text
Task: <TASK-ID>
Base revision: <COMMIT-SHA>
Agent branch: <BRANCH>

Implemented:
- <item>
- <item>

Files changed:
- <path>
- <path>

Tests:
- cargo fmt --all -- --check: PASS/FAIL
- cargo check --all-targets: PASS/FAIL
- cargo test --all-targets: PASS/FAIL
- cargo clippy --all-targets --all-features -- -D warnings: PASS/FAIL
- cargo audit: PASS/FAIL/SKIPPED
- Additional checks: <results>

Security review:
- <result>

Known limitations/failures:
- <none or exact details>

PR target: rust
Status: READY FOR REVIEW / BLOCKED
```

Never mark the task `READY FOR REVIEW` when a mandatory verification check has failed or when required evidence is missing.
