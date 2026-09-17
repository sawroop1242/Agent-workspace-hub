# AWH External Agent Rules

## 1. Repository boundary

- Product repository: `sawroop1242/Agent-workspace-hub`.
- Active Rust development branch: `rust`.
- AWH source code and tests remain the product source of truth.
- External agent infrastructure in `.github/` exists only to build, repair, test, benchmark, and evaluate AWH.
- Do not add SWE-agent, mini-SWE-agent, SWE-ReX, SWE-bench, SWE-smith, CodeClash, sb-cli, or model-provider SDKs to AWH runtime dependencies unless a separate product requirement explicitly calls for them.

## 2. Branch and PR rules

Agents must:

1. Start from the requested base revision.
2. Work on a temporary branch such as `agent/<task-id>`.
3. Make focused commits.
4. Run required verification before requesting review.
5. Push only the temporary branch.
6. Open a pull request targeting `rust`.

Agents must not directly push implementation changes to `rust` or `main`.

## 3. Repository inspection before editing

Before making changes, the agent must inspect:

- relevant `docs/` files;
- current source implementation;
- existing tests;
- current Git diff/status;
- relevant CI configuration;
- related issue or task prompt;
- recent implementation history when behavior is ambiguous.

Do not implement a feature merely because it is described in an old roadmap or prompt. Verify that the requested behavior still matches the current architecture.

## 4. Evidence precedence

When sources disagree, use this order for implementation behavior:

1. Executable tests and observed current behavior.
2. Current Rust source and public interfaces.
3. Current security and architecture documentation.
4. Current CI/workflow contracts.
5. Historical status documents.
6. Agent prompts and generated task descriptions.

A discrepancy must be documented rather than silently ignored when it can affect correctness or security.

## 5. Change-scope rules

- Implement only the assigned task.
- Prefer the smallest coherent change.
- Do not perform unrelated refactors.
- Do not rename public APIs without an explicit requirement.
- Do not replace an existing security primitive with a duplicate implementation.
- Add or update focused regression tests for behavior changes.
- Update documentation when the externally observable contract changes.

## 6. Security invariants

The existing AWH security model must remain intact:

- deny by default;
- centralized execution/permission gates;
- fail closed when security controls cannot be applied;
- sandbox subprocess execution where required;
- validate filesystem paths against allowed bases;
- reject traversal and symlink escapes;
- never expose secrets in logs, errors, artifacts, prompts, or patches;
- preserve authentication and authorization boundaries;
- preserve MCP request/body/message limits;
- preserve timeouts and resource limits.

A change that weakens a security invariant is not acceptable merely because tests pass.

## 7. Error handling

- Do not hide errors to make an agent task appear successful.
- Do not replace structured errors with generic success responses.
- Avoid `unwrap()`, `expect()`, or `panic!()` on external, network, MCP, configuration, filesystem, or user-controlled input.
- Preserve stable JSON-RPC error behavior.
- Include useful context at application boundaries without leaking sensitive values.

## 8. Testing rules

For normal Rust changes, run at minimum:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

For dependency/security changes, also run:

```bash
cargo audit
```

Additional tests required by the task must not be skipped.

## 9. Test integrity

Agents must never:

- delete a failing test merely to obtain a green build;
- weaken an assertion without documented justification;
- skip a test without reporting the reason;
- modify benchmark criteria to improve a score;
- claim verification that was not actually executed.

If an environment prevents a required check, report it explicitly in the PR.

## 10. External benchmark boundaries

External evaluation tools are measurement infrastructure, not product dependencies.

- SWE-bench measures software-engineering task performance.
- SWE-smith generates engineering tasks.
- CodeClash measures long-running engineering behavior.
- sb-cli can execute remote benchmark workloads.
- SWE-ReX provides isolated execution for external agent workloads.

Their results may identify problems or regressions, but they do not override AWH's product requirements or security model.

## 11. Model credentials

GitHub Actions may provide a model pool through secrets:

```text
NVIDIA_NIM_API_KEY_1
NVIDIA_NIM_API_KEY_2
NVIDIA_NIM_API_KEY_3
NVIDIA_NIM_API_KEY_4
GEMINI_API_KEY_1
GEMINI_API_KEY_2
GEMINI_API_KEY_3
GEMINI_API_KEY_4
```

Rules:

- Never print secret values.
- Never write secrets to repository files.
- Never commit secrets to branches or artifacts.
- Do not expose all provider keys to an individual agent when routing can be performed outside the agent.
- If credentials are unavailable, stop or use a credential-free verification path; do not fabricate successful model execution.

## 12. Agent stop conditions

Stop and report instead of making speculative changes when:

- the task is underspecified;
- required repository context is missing;
- the requested behavior conflicts with a security invariant;
- the required dependency or tool is unavailable;
- the working tree contains unexplained changes that could be overwritten;
- verification cannot be performed honestly;
- the task requires destructive access outside the permitted sandbox.

## 13. Completion contract

An agent task is complete only when:

1. the requested behavior is implemented;
2. relevant tests cover the change;
3. required verification passes or failures are explicitly reported;
4. no unrelated changes remain;
5. security invariants remain intact;
6. the temporary branch contains the commits;
7. the agent has prepared a PR targeting `rust`.

A green CI result alone is not sufficient evidence that the task satisfies its requirements.
