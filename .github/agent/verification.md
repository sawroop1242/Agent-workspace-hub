# AWH Pre-PR Verification Contract

## Purpose

This file defines the mandatory verification gate for external agents working on Agent Workspace Hub (AWH).

The verification infrastructure is external to AWH. It exists to build, test, audit, benchmark, and evaluate the repository; it must not become an AWH runtime dependency.

## 1. Preflight

Before running verification, the agent must confirm:

```bash
git status --short
git branch --show-current
git diff --check
rustc --version
cargo --version
```

Pass conditions:

- The agent is on its temporary task branch, not `rust` or `main`.
- The working tree contains only intentional task changes.
- `git diff --check` reports no whitespace errors.
- Rust and Cargo are available.

Fail conditions:

- Unexplained pre-existing changes could be overwritten.
- The agent is about to push implementation changes directly to `rust` or `main`.
- Required tooling is unavailable.

Do not clean, reset, stash, or overwrite unexplained changes without explicit authorization.

## 2. Mandatory Rust verification

For every implementation change, run all of the following from the repository root:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Pass conditions:

- Every command exits with status `0`.
- No test is skipped, deleted, weakened, or modified merely to obtain a passing result.
- Clippy produces no warnings because warnings are treated as errors.

Fail conditions:

- Any command exits non-zero.
- A required test cannot be executed and the omission is not explicitly reported.
- The agent changes verification commands or test assertions solely to hide a failure.

A failed mandatory check blocks the PR from being considered verified.

## 3. Dependency and security verification

When dependencies, Cargo manifests, build configuration, authentication, filesystem access, subprocess execution, MCP handling, networking, or other security-sensitive behavior changes, also run:

```bash
cargo audit
```

Pass condition:

- `cargo audit` exits successfully with no unresolved vulnerability that violates the repository's security requirements.

Fail conditions:

- `cargo audit` exits non-zero.
- A known vulnerability is ignored without a documented, reviewed reason.
- Security behavior is weakened to make tests pass.

If a change is not security- or dependency-related, `cargo audit` remains recommended but is not a substitute for the mandatory Rust verification above.

## 4. Test coverage and regression verification

For behavior changes, the agent must inspect and run the relevant existing unit and integration tests and add focused regression coverage where appropriate.

Required rules:

- A bug fix must include a regression test when the behavior can be tested deterministically.
- A new public behavior must have corresponding test coverage unless the behavior is explicitly non-testable.
- Existing security, MCP, filesystem, subprocess, and error-handling tests must continue to pass.
- Tests must exercise the actual implementation path rather than a mock that bypasses the behavior under test.

Pass condition:

- Relevant tests pass and the change is covered to a level appropriate to the risk and scope.

Fail condition:

- The implementation changes behavior without adequate verification.

## 5. Security-invariant verification

For security-sensitive changes, verify that these AWH invariants remain true:

- deny-by-default behavior remains intact;
- authorization and authentication checks remain enforced;
- filesystem paths cannot escape permitted bases through traversal or symlinks;
- subprocess execution remains bounded and sandboxed where required;
- MCP request/body/message limits remain enforced;
- timeouts and resource limits remain enforced;
- secrets never appear in logs, errors, test output, artifacts, prompts, or patches;
- remote access requirements remain fail-closed according to the current security contract;
- structured protocol errors remain stable and do not expose sensitive data.

Pass condition:

- Relevant tests and manual inspection demonstrate that the security boundary is preserved.

Fail condition:

- Any change weakens or bypasses a security control, even if the general test suite is green.

## 6. Cross-platform verification

The repository CI matrix is expected to validate Rust build/test behavior on:

- Ubuntu
- macOS
- Windows

For platform-specific code, the agent must run the applicable local checks when possible and rely on the corresponding GitHub Actions matrix job for unavailable platforms.

Pass condition:

- Applicable platform CI jobs pass.

Fail condition:

- A platform-specific regression is introduced or a required CI job fails.

An unavailable local platform is not itself a failure if the corresponding CI job is allowed to execute and is reported accurately.

## 7. MCP verification

If the task changes MCP behavior, run the normal Rust verification and the relevant MCP tests. At minimum, verify:

- valid requests still receive valid protocol responses;
- malformed requests fail deterministically;
- authentication/authorization rules remain enforced;
- configured request/body/message limits remain enforced;
- timeout behavior remains bounded;
- sensitive values are not returned in errors or logs.

A task must not be marked verified solely because the binary compiles if MCP behavior changed.

## 8. Filesystem and subprocess verification

If the task changes filesystem or process execution behavior, verify the applicable cases for:

- allowed paths;
- traversal attempts;
- symlink escapes;
- missing paths;
- permission failures;
- process timeout;
- process exit status;
- stdout/stderr capture and limits;
- cleanup after failure.

Generated or untrusted code must execute only through the repository's permitted sandbox/evaluation path. External sandbox infrastructure such as SWE-ReX is CI/evaluation infrastructure and is not an AWH dependency.

## 9. External agent and model verification

External coding agents and model providers may be used by GitHub Actions, but their credentials and runtimes are outside AWH.

When model-backed execution is part of a workflow:

- use GitHub Actions secrets for credentials;
- never echo or print secret values;
- do not write credentials into repository files or artifacts;
- verify that failed model calls are reported as failures rather than converted into false success;
- record model/provider identity without recording secret material;
- prefer routing credentials outside the agent process when possible.

A model-generated patch is not considered correct until it passes the same repository verification required for a human-authored patch.

## 10. Benchmark verification

External benchmarks are evaluation infrastructure only.

### SWE-bench

For SWE-bench tasks:

- apply the generated patch to an isolated checkout;
- run the benchmark's task-specific tests;
- preserve the original repository test suite as an additional gate;
- report resolved/unresolved tasks accurately;
- never alter benchmark tests or grading criteria.

### SWE-smith

For SWE-smith-generated tasks:

- keep each generated task isolated;
- record the task identifier and base revision;
- verify that the task is reproducible before evaluating an agent;
- reject malformed or non-reproducible tasks rather than counting them as successful repairs.

### CodeClash

For long-running goal-oriented evaluation:

- use an isolated workspace;
- enforce the configured time/resource limits;
- preserve complete task and verification logs;
- evaluate the resulting repository state with deterministic checks where available;
- do not modify the evaluation criteria to improve results.

### sb-cli / remote benchmark execution

For remote benchmark execution:

- submit only the intended repository revision/task;
- do not send AWH secrets or unrelated repository data;
- capture the remote job identifier and final status;
- treat remote benchmark success as evaluation evidence, not as a replacement for AWH CI.

## 11. Artifact and evidence requirements

Before opening a PR, the agent must be able to provide:

```text
Base revision: <commit>
Task branch: <branch>
Changed scope: <short description>

Verification:
- cargo fmt --all -- --check: PASS/FAIL
- cargo check --all-targets: PASS/FAIL
- cargo test --all-targets: PASS/FAIL
- cargo clippy --all-targets --all-features -- -D warnings: PASS/FAIL
- cargo audit: PASS/FAIL/SKIPPED (with reason)

Additional tests: <commands/results>
Security checks: <results>
Benchmark checks: <results or N/A>
Known failures: <none or exact failures>
```

Do not report `PASS` for a command that was not actually executed.

## 12. PR gate

A PR is **VERIFIED** only when all applicable mandatory checks pass and all exceptions are explicitly documented.

A PR is **BLOCKED** when any mandatory check fails, required regression coverage is missing, a security invariant is weakened, or verification results cannot be established honestly.

A PR may be **CONDITIONALLY VERIFIED** only when a platform-dependent check cannot run in the current environment but the corresponding GitHub Actions check is configured and required; the missing local check must be explicitly reported.

The agent must never claim that a task is complete solely because a patch was generated, a model returned success, or an external benchmark produced a score.

## 13. Final command before PR

After all applicable checks pass, run:

```bash
git diff --check
git status --short
git log -1 --oneline
```

Then confirm:

1. Only intended files changed.
2. No secrets or generated credentials are present.
3. Required verification results are recorded.
4. The temporary branch is pushed.
5. The PR targets `rust`.
6. The PR description states any skipped or environment-limited checks.

Only then should the agent request review.
