# AWH External Benchmark Evaluation Prompt

## Mission

Run and report external software-engineering benchmark evaluations of Agent Workspace Hub (AWH) without turning benchmark tools, agent runtimes, datasets, or model-provider tooling into AWH runtime dependencies.

The benchmark system is evaluation infrastructure only. AWH remains an independent, agent-agnostic Rust product.

Target branch for repository changes: `rust`.

## 1. Hard Boundary

External evaluation tools MUST remain outside AWH runtime code and dependency graph.

Do not add any of the following to AWH `Cargo.toml`, `src/`, runtime images, or product configuration merely to execute benchmarks:

- SWE-bench tooling or datasets;
- SWE-smith tooling or generated benchmark infrastructure;
- SWE-ReX or other external sandbox runtimes;
- mini-SWE-agent / SWE-agent runtimes;
- CodeClash tooling;
- `sb-cli`;
- model-provider SDKs used only by benchmark agents;
- NVIDIA NIM or Gemini client libraries used only by GitHub Actions;
- benchmark-specific Python/Node/Rust packages that are not required by AWH itself.

These systems may be installed or invoked by dedicated GitHub Actions jobs, temporary CI environments, or external services.

Never modify AWH production behavior to make an external benchmark harness easier to operate.

## 2. Evaluation Modes

Use the benchmark appropriate to the question being evaluated.

### SWE-bench

Use for repository-level software-engineering tasks represented by benchmark instances.

Record:

- benchmark/version or dataset revision;
- selected task IDs;
- repository revision evaluated;
- agent/model configuration;
- sandbox configuration;
- patch produced;
- test/verification result;
- resolved/unresolved outcome;
- timeout or infrastructure failures.

Do not claim that a benchmark result represents general AWH quality.

### SWE-smith

Use for generated or synthetic software-engineering task evaluation where applicable.

Record:

- generator/version or revision;
- generation configuration;
- seed if available;
- task count;
- task identifiers;
- target AWH revision;
- agent/model configuration;
- execution environment;
- per-task outcomes;
- generation or execution failures.

Generated tasks must be clearly distinguished from curated benchmark tasks.

### CodeClash

Use for long-running, goal-oriented software-engineering evaluation when the selected scenario supports AWH.

Record:

- CodeClash version/revision;
- scenario/task identifier;
- starting AWH revision;
- allowed execution time;
- agent/model configuration;
- tool permissions;
- intermediate checkpoints where available;
- final outcome;
- infrastructure failures separately from task failures.

Do not treat duration or token usage alone as a correctness result.

### sb-cli

Use `sb-cli` only as external benchmark execution infrastructure.

Record:

- CLI version;
- remote/local execution mode;
- benchmark/task identifiers;
- submitted revision or artifact;
- execution status;
- result identifier;
- returned score/result data;
- infrastructure errors.

Never put `sb-cli` into AWH's runtime dependency graph.

## 3. Repository Preflight

Before an evaluation, inspect the exact revision being evaluated.

Run:

```bash
git status --short
git rev-parse HEAD
git branch --show-current
git log -5 --oneline
```

For a normal AWH engineering candidate, verify the baseline first:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Run `cargo audit` when the benchmark evaluates a dependency/security-sensitive change.

If the baseline does not pass, record that fact before starting the benchmark. Do not silently repair the candidate unless the evaluation protocol explicitly requires an agent to do so.

## 4. Isolated Evaluation Environment

Benchmark execution must be isolated from the normal AWH development environment.

Prefer:

- GitHub-hosted runners;
- temporary workspaces;
- benchmark-managed sandboxes;
- disposable containers or virtual environments;
- read-only benchmark inputs where practical.

Do not persist benchmark credentials, model API keys, generated patches, or private task data in the AWH repository unless explicitly required by the evaluation design.

Clean temporary state after evaluation when possible.

## 5. Model and Credential Handling

Model credentials are CI infrastructure secrets.

Expected secret pools may include:

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

- inject secrets through GitHub Actions secrets or the external benchmark service;
- never commit keys;
- never echo keys;
- never include keys in benchmark artifacts;
- never write keys into AWH source/configuration;
- redact authorization headers and sensitive environment values from logs;
- report only the model identifier/configuration needed for reproducibility.

If a model request fails, report it as an infrastructure/model failure rather than silently converting it into a benchmark failure.

## 6. Agent Execution

When an external coding agent modifies AWH:

1. Start from a clean, identified AWH revision.
2. Give the agent only the task and repository context required by the evaluation.
3. Keep the agent's tools outside the AWH dependency graph.
4. Execute in an isolated workspace.
5. Capture the generated patch or resulting commit.
6. Apply/evaluate the patch according to the benchmark protocol.
7. Run the appropriate AWH verification commands.
8. Preserve enough evidence to reproduce the result.

Do not modify benchmark tasks after the agent starts unless the protocol explicitly permits it.

## 7. Fairness and Reproducibility

Keep evaluation conditions stable when comparing runs.

Record, where applicable:

- AWH commit SHA;
- benchmark version/dataset revision;
- task IDs;
- model/provider/model version;
- agent version;
- system prompt or benchmark prompt version;
- tool configuration;
- timeout;
- retry policy;
- temperature or other generation settings;
- runner OS/image;
- relevant environment/runtime versions;
- seed, when supported.

Do not change multiple evaluation variables and attribute an observed difference to one variable without evidence.

## 8. Result Classification

Separate outcome categories.

### Task result

The benchmark task itself passed, failed, or partially completed according to the benchmark's defined criteria.

### Verification result

AWH's own relevant tests/checks passed or failed after the evaluated change.

### Infrastructure result

The benchmark could not execute reliably because of runner, sandbox, network, provider, quota, timeout, or tool failure.

### Invalid result

The evaluation violated the benchmark protocol or lacked required evidence and therefore must not be used as a valid comparison.

Never convert infrastructure failures into task failures.

## 9. Evidence Collection

Collect machine-readable results whenever the benchmark provides them.

For each run, retain at minimum:

```text
benchmark name/version
AWH revision
agent version/configuration
model/provider configuration
benchmark/task identifiers
start/end timestamps
execution status
per-task outcome
verification outcome
infrastructure errors
artifact/result identifiers
```

When possible, store benchmark outputs as GitHub Actions artifacts rather than committing them to AWH source.

Do not upload secrets, private credentials, or sensitive task contents into public artifacts.

## 10. Required Reporting Format

Every completed evaluation must produce a concise report in this form:

```text
Benchmark: <name>
Benchmark version/revision: <version>
AWH revision: <commit SHA>
Task set: <IDs/count>
Agent: <name/version>
Model: <provider/model identifier>
Runner/environment: <environment>

Execution:
- Tasks attempted: <N>
- Tasks completed: <N>
- Task failures: <N>
- Infrastructure failures: <N>
- Invalid results: <N>

AWH verification:
- fmt: PASS/FAIL/SKIPPED
- check: PASS/FAIL/SKIPPED
- test: PASS/FAIL/SKIPPED
- clippy: PASS/FAIL/SKIPPED
- audit: PASS/FAIL/SKIPPED/NOT APPLICABLE

Benchmark result:
- Official metric(s): <exact metric names and values>
- Per-task result artifact: <artifact/reference>

Notes:
<important limitations, retries, timeouts, provider failures, or protocol deviations>

Dependency boundary:
- External benchmark tools added to AWH runtime: NO
- AWH Cargo.toml modified for benchmark execution: NO
- Benchmark/model credentials committed: NO

Status: VALID / INVALID / INCOMPLETE
```

Only report metrics actually produced by the benchmark. Do not invent scores, success rates, or statistical significance.

## 11. Comparison Rules

When comparing two or more evaluations:

- use the same benchmark definition where possible;
- identify any changed variables;
- report sample size;
- report raw benchmark metrics before interpretation;
- distinguish task failures from infrastructure failures;
- do not mix curated and generated task sets without labeling them;
- do not compare results from materially different benchmark versions as if they were identical.

If the sample is too small or conditions are materially different, state that the comparison is limited rather than manufacturing a conclusion.

## 12. GitHub Actions Boundary

External benchmarks belong in dedicated workflows such as:

```text
.github/workflows/04-swebench.yml
.github/workflows/05-swesmith.yml
.github/workflows/06-codeclash.yml
.github/workflows/07-evaluation.yml
.github/workflows/08-nightly.yml
```

These workflows may install external tools and invoke external services.

They must not:

- modify `Cargo.toml` merely to install benchmark tooling;
- commit benchmark dependencies into AWH;
- alter AWH security controls to accommodate the benchmark;
- expose model credentials in logs;
- bypass required AWH verification;
- directly push benchmark-generated implementation changes to `rust` or `main`.

Benchmark-generated code changes should use temporary branches and PRs when they are intended to become AWH changes.

## 13. Stop Conditions

Stop the evaluation and mark it `INCOMPLETE` or `INVALID` when:

- the benchmark environment cannot be reproduced;
- required benchmark inputs are missing;
- credentials are unavailable or exposed;
- the benchmark protocol is violated;
- the AWH revision under evaluation cannot be identified;
- result artifacts are corrupted or incomplete;
- infrastructure failures prevent reliable task evaluation;
- running the benchmark would require adding benchmark tooling to AWH runtime dependencies;
- a security boundary would need to be bypassed.

Do not fabricate missing results.

## 14. Completion Criteria

An external benchmark evaluation is complete only when:

1. The exact AWH revision is recorded.
2. The benchmark/tool versions are recorded.
3. The evaluation environment is identified.
4. Model/agent configuration is recorded without exposing credentials.
5. Task-level and infrastructure outcomes are separated.
6. AWH verification results are reported honestly.
7. Benchmark artifacts are retained safely.
8. The evaluation introduces no AWH runtime dependency on external benchmark tooling.
9. Any repository changes follow the normal temporary-branch and PR process.
