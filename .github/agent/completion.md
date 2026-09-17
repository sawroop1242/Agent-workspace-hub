# AWH Agent Completion Contract

## Purpose

This document defines the final gate an external agent must satisfy before declaring an AWH task complete or requesting PR review.

The agent infrastructure is external to AWH. Completion means the requested repository change is implemented, verified, secure, and reviewable; it does not mean that an external model, benchmark, or agent reported success.

## 1. Completion states

### READY FOR REVIEW

Use only when:

- the requested behavior is implemented;
- scope is limited to the assigned task;
- required tests and verification checks pass;
- security invariants are preserved;
- no unexplained changes remain;
- the branch is ready for a PR targeting `rust`.

### CONDITIONALLY READY

Use only when a check cannot run locally because of a platform or environment limitation, but the corresponding GitHub Actions check is configured and expected to run. The exact missing check and reason must be recorded.

### BLOCKED

Use when any mandatory check fails, required evidence is missing, a security invariant is violated, the task is underspecified, or the agent cannot establish correctness honestly.

Never convert `BLOCKED` into success by weakening tests, hiding errors, or changing acceptance criteria.

## 2. Implementation gate

Before completion, verify:

- [ ] The requested behavior is implemented.
- [ ] All explicit task requirements are addressed.
- [ ] Non-goals were not accidentally implemented.
- [ ] Public interfaces were changed only when required.
- [ ] Error handling remains correct and informative without leaking secrets.
- [ ] Documentation was updated if externally observable behavior changed.

## 3. Test gate

For normal Rust changes, all of these must pass:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

For dependency or security-sensitive changes:

```bash
cargo audit
```

Additional task-specific tests are mandatory when applicable.

A command that was not executed must never be reported as `PASS`.

## 4. Security gate

Completion is blocked if the change weakens any AWH security invariant, including:

- deny-by-default authorization;
- centralized permission/security gates;
- filesystem path validation and symlink protection;
- subprocess sandboxing and resource limits;
- MCP request/body/message limits;
- timeout enforcement;
- authentication requirements for remote access;
- fail-closed security behavior;
- secret handling and log redaction;
- validation of untrusted or user-controlled input.

For security-sensitive changes, the completion report must identify the security checks performed and their results.

## 5. Change-scope gate

Run:

```bash
git diff --check
git status --short
git diff --stat
```

Completion requires:

- only intentional task files are changed;
- no generated credentials, tokens, private keys, or unrelated artifacts are present;
- no unrelated refactor is bundled with the task;
- external development/evaluation tools are not added as AWH runtime dependencies.

## 6. Branch and PR gate

The agent must:

1. Work on a temporary branch such as `agent/<TASK-ID>`.
2. Commit the implementation with focused commits.
3. Push the temporary branch only.
4. Prepare a PR targeting `rust`.
5. Never directly push implementation changes to `rust` or `main`.

The PR description must include the verification evidence and any known limitations.

## 7. Evidence requirements

The final completion report must contain:

```text
Task ID: <TASK-ID>
Base revision: <COMMIT-SHA>
Agent branch: <BRANCH>

Outcome:
<one-paragraph summary>

Changed files:
- <path>

Required verification:
- cargo fmt --all -- --check: PASS/FAIL
- cargo check --all-targets: PASS/FAIL
- cargo test --all-targets: PASS/FAIL
- cargo clippy --all-targets --all-features -- -D warnings: PASS/FAIL
- cargo audit: PASS/FAIL/SKIPPED + reason

Additional verification:
- <command>: PASS/FAIL/N/A

Security:
- <checks and result>

Known limitations:
- <none or exact limitation>

PR target: rust
Completion state: READY FOR REVIEW / CONDITIONALLY READY / BLOCKED
```

## 8. Failure reporting

When verification fails, report the exact command, exit status when available, and relevant failure summary.

Do not:

- claim success after a failed command;
- delete or weaken tests to remove failures;
- suppress compiler or Clippy warnings solely to obtain a green build;
- alter benchmark grading criteria;
- hide model or external-agent failures;
- omit known regressions from the completion report.

If the failure is outside the task's scope but blocks verification, record it and leave the task `BLOCKED` unless the repository's established CI process explicitly permits a conditional state.

## 9. Review handoff

Before requesting review, the agent should provide reviewers with:

- a concise problem/solution summary;
- the files changed and why;
- tests added or modified;
- complete verification results;
- security impact, if any;
- known limitations or follow-up work;
- benchmark results when the task is an evaluation task.

The reviewer should be able to reproduce the important verification steps from the PR description and repository documentation.

## 10. Final completion checklist

- [ ] Task requirements satisfied.
- [ ] Scope respected.
- [ ] Regression coverage added where appropriate.
- [ ] `cargo fmt --all -- --check` passed.
- [ ] `cargo check --all-targets` passed.
- [ ] `cargo test --all-targets` passed.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passed.
- [ ] `cargo audit` passed when applicable.
- [ ] Security invariants verified.
- [ ] `git diff --check` passed.
- [ ] No secrets or unrelated artifacts present.
- [ ] Temporary branch used.
- [ ] PR targets `rust`.
- [ ] Completion report is accurate.

Only after all applicable boxes are satisfied may the agent declare `READY FOR REVIEW`.
