# Autonomous development setup

The AWH repository contains a sequential GitHub Actions control plane for three OpenHands agents:

`Agent 1 (plan) -> Agent 2 (build) -> PR -> Agent 3 (review) -> CI gate -> Agent 1 (merge) -> next feature`

All three agents use the **same model: `moonshotai/kimi-k3`**, hosted by NVIDIA's OpenAI-compatible NIM API at `https://integrate.api.nvidia.com/v1`. NVIDIA currently lists Kimi-K3 as a free hosted endpoint with a 1M-token context window and native tool-calling/reasoning support.

## Key pools

Each agent has its own four-key NVIDIA pool so one agent cannot consume another agent's quota:

### Agent 1 — Orchestrator

- `AWH_AGENT1_NVIDIA_KEY_1`
- `AWH_AGENT1_NVIDIA_KEY_2`
- `AWH_AGENT1_NVIDIA_KEY_3`
- `AWH_AGENT1_NVIDIA_KEY_4`

### Agent 2 — Builder

- `AWH_AGENT2_NVIDIA_KEY_1`
- `AWH_AGENT2_NVIDIA_KEY_2`
- `AWH_AGENT2_NVIDIA_KEY_3`
- `AWH_AGENT2_NVIDIA_KEY_4`

### Agent 3 — Reviewer

- `AWH_AGENT3_NVIDIA_KEY_1`
- `AWH_AGENT3_NVIDIA_KEY_2`
- `AWH_AGENT3_NVIDIA_KEY_3`
- `AWH_AGENT3_NVIDIA_KEY_4`

### Shared routine fallback

- `AWH_ROUTINE_NVIDIA_KEY`

The routine key is attempted only after the four role-specific keys report a rate-limit condition.

## Rotation behavior

The workflows use this order:

`KEY_1 -> KEY_2 -> KEY_3 -> KEY_4 -> ROUTINE_KEY`

Rotation happens **only when the previous request appears to have hit HTTP 429/rate limiting**. Normal agent failures are not silently retried with another key; they fail the workflow so the underlying problem remains visible.

The model and endpoint are fixed in the workflows:

- Model: `moonshotai/kimi-k3`
- Base URL: `https://integrate.api.nvidia.com/v1`

No per-agent model selection is used.

## Other required secret

Create:

- `AWH_AUTOMATION_TOKEN` — fine-grained token for this repository with Contents read/write, Pull requests read/write, and Checks read.

Do not put NVIDIA API keys in repository files or workflow YAML.

## Starting the loop

1. Make sure the `rust` branch is the repository default branch.
2. Add the automation token and all NVIDIA key secrets under **Settings -> Secrets and variables -> Actions**.
3. Open **Actions -> AWH Autonomous Development Loop**.
4. Choose **Run workflow**, keep `start=true`, and optionally enter a specific ready feature.
5. Agent 1 selects/plans the feature and dispatches Agent 2.
6. Agent 2 creates or updates `feature/<feature-id>`, implements the feature, runs deterministic Rust verification, and opens a PR.
7. Agent 3 independently reviews the PR and writes a machine-readable verdict.
8. If changes are requested, Agent 2 gets the latest review feedback and retries, up to three review rounds.
9. If approved, the merge gate waits for PR checks, verifies the PR is mergeable, and squash-merges the exact reviewed head.
10. The backlog/state are updated and the next ready feature is dispatched automatically.

## Bootstrap status

`AWH-AUTO-001` is already implemented directly as the bootstrap commit because the loop cannot safely bootstrap itself. Future ready features are expected to go through the full three-agent pipeline.

## Failure behavior

The loop is deliberately fail-closed. Missing LLM credentials, failed Rust checks, non-approved review state, merge conflicts, or exhausted review rounds stop the current feature instead of silently merging or skipping work.

The existing Rust CI and release workflows remain separate. The autonomous loop is an orchestration layer, not a replacement for repository CI.
