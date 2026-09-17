# AWH External Agent System Contract

## Purpose

This directory defines the autonomous development infrastructure used to build, repair, verify, benchmark, and evaluate Agent Workspace Hub (AWH).

The infrastructure is **external to AWH**. It must never become a runtime dependency of the Rust application.

## Repository Target

- Product: `agent-workspace-hub`
- Primary development branch: `rust`
- Agents work from temporary branches and propose changes through pull requests.
- Agents must not push implementation changes directly to `rust` or `main`.

## External Tool Roles

| Tool | Role | Boundary |
|---|---|---|
| mini-SWE-agent / SWE-agent | Implement and repair repository tasks | CI/agent runtime only |
| SWE-ReX | Isolated shell and code execution | CI sandbox only |
| SWE-bench | Software-engineering benchmark evaluation | Evaluation only |
| SWE-smith | Generate software-engineering tasks | Task-generation only |
| CodeClash | Long-running goal-oriented engineering evaluation | Evaluation only |
| sb-cli | Remote benchmark execution | Evaluation infrastructure only |

None of these tools may be added to `Cargo.toml`, AWH source code, or the AWH runtime image merely because CI uses them.

## Agent Operating Loop

1. Read this file and `.github/agent/rules.md`.
2. Read the repository documentation relevant to the assigned task.
3. Inspect source code and existing tests before editing.
4. Make the smallest coherent change that addresses the task.
5. Run the required verification commands from `.github/agent/verification.md`.
6. Record failures accurately; never hide or weaken tests to make a task pass.
7. Commit changes on the temporary agent branch.
8. Open a pull request against `rust`.
9. Do not merge the pull request unless the repository's normal review/merge process explicitly allows it.

## Task Isolation

Every agent task must have:

- a unique task identifier;
- a clean checkout of the requested base revision;
- an isolated working directory or sandbox when executing untrusted/generated code;
- bounded execution time;
- captured stdout/stderr and exit status;
- no access to production secrets unless the specific evaluation requires them.

## Secret Handling

Model credentials are provided to CI through GitHub Actions secrets. Agents must never print, commit, persist, or include credentials in logs, artifacts, prompts, patches, or generated documentation.

Expected secret pool:

- `NVIDIA_NIM_API_KEY_1` through `NVIDIA_NIM_API_KEY_4`
- `GEMINI_API_KEY_1` through `GEMINI_API_KEY_4`

Agents should receive a model endpoint abstraction rather than being given all raw credentials when possible.

## Source-of-Truth Rules

1. Rust source and tests are authoritative for implementation behavior.
2. Repository documentation defines intended behavior when it does not contradict executable tests.
3. CI configuration defines automation behavior.
4. Agent prompts are instructions for the agent, not product specifications.
5. Benchmarks measure behavior; they do not redefine product requirements.

When documentation and executable behavior disagree, investigate the discrepancy and verify against tests and project intent before changing either one.

## Failure Policy

An agent must stop and report when:

- the requested behavior conflicts with repository invariants;
- required credentials are unavailable;
- the task requires destructive operations outside the sandbox;
- verification cannot be performed honestly;
- the repository state is inconsistent or the task is underspecified.

Do not claim success when a required check was skipped or failed.
