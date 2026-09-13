# Autonomous development setup

The AWH repository contains a sequential GitHub Actions control plane for three OpenHands agents:

`Agent 1 (plan) -> Agent 2 (build) -> PR -> Agent 3 (review) -> CI gate -> Agent 1 (merge) -> next feature`

All three agents use the **same model: `moonshotai/kimi-k3`**, hosted by NVIDIA's OpenAI-compatible NIM API at `https://integrate.api.nvidia.com/v1`.

## Minimum key requirement

The minimum required LLM configuration is intentionally small:

- **One role-specific NVIDIA key per agent**
- **One shared routine/fallback NVIDIA key**

That means the minimum is **4 NVIDIA secrets total**:

1. Agent 1 primary key
2. Agent 2 primary key
3. Agent 3 primary key
4. Shared routine fallback key

### Agent 1 — Orchestrator

**Required:**

- `AWH_AGENT1_NVIDIA_KEY_1`

**Optional additional rotation keys:**

- `AWH_AGENT1_NVIDIA_KEY_2`
- `AWH_AGENT1_NVIDIA_KEY_3`

### Agent 2 — Builder

**Required:**

- `AWH_AGENT2_NVIDIA_KEY_1`

**Optional additional rotation keys:**

- `AWH_AGENT2_NVIDIA_KEY_2`
- `AWH_AGENT2_NVIDIA_KEY_3`

### Agent 3 — Reviewer

**Required:**

- `AWH_AGENT3_NVIDIA_KEY_1`

**Optional additional rotation keys:**

- `AWH_AGENT3_NVIDIA_KEY_2`
- `AWH_AGENT3_NVIDIA_KEY_3`

### Shared routine fallback

**Required:**

- `AWH_ROUTINE_NVIDIA_KEY`

The routine key is used after all configured role-specific keys for the current agent hit a rate limit. It is shared across the three agents and is not tied to a specific role.

## Rotation behavior

Each agent follows the same order:

`KEY_1 -> KEY_2 (if configured) -> KEY_3 (if configured) -> ROUTINE_KEY`

Therefore, **KEY_1 and ROUTINE_KEY are mandatory**. KEY_2 and KEY_3 are optional and automatically skipped when they are not configured.

Rotation happens **only when the previous request appears to have hit HTTP 429/rate limiting**. Normal agent failures are not silently retried with another key; they fail the workflow so the underlying problem remains visible.

The workflows explicitly validate that each agent's required `KEY_1` and the shared routine key are present before invoking the model.

## Required GitHub secrets

Create these secrets under **Settings -> Secrets and variables -> Actions**:

### Mandatory

- `AWH_AGENT1_NVIDIA_KEY_1`
- `AWH_AGENT2_NVIDIA_KEY_1`
- `AWH_AGENT3_NVIDIA_KEY_1`
- `AWH_ROUTINE_NVIDIA_KEY`

### Optional

- `AWH_AGENT1_NVIDIA_KEY_2`
- `AWH_AGENT1_NVIDIA_KEY_3`
- `AWH_AGENT2_NVIDIA_KEY_2`
- `AWH_AGENT2_NVIDIA_KEY_3`
- `AWH_AGENT3_NVIDIA_KEY_2`
- `AWH_AGENT3_NVIDIA_KEY_3`

### GitHub automation

- `AWH_AUTOMATION_TOKEN` — fine-grained token for this repository with Contents read/write, Pull requests read/write, and Checks read.

So you can run the entire autonomous system with only **4 NVIDIA secrets + 1 GitHub automation token**. Adding the optional role-specific keys improves rate-limit resilience but is not required.

Do not put NVIDIA API keys in repository files or workflow YAML.

## Model configuration

The model and endpoint are fixed in the workflows:

- Model: `moonshotai/kimi-k3`
- Base URL: `https://integrate.api.nvidia.com/v1`

No per-agent model selection is used.

## Starting the loop

1. Make sure the `rust` branch is the repository default branch.
2. Add the four mandatory NVIDIA secrets and `AWH_AUTOMATION_TOKEN`.
3. Optionally add KEY_2 and KEY_3 for any agent that needs extra rotation capacity.
4. Open **Actions -> AWH Autonomous Development Loop**.
5. Choose **Run workflow**, keep `start=true`, and optionally enter a specific ready feature.
6. Agent 1 selects/plans the feature and dispatches Agent 2.
7. Agent 2 creates or updates `feature/<feature-id>`, implements the feature, runs deterministic Rust verification, and opens a PR.
8. Agent 3 independently reviews the PR and writes a machine-readable verdict.
9. If changes are requested, Agent 2 gets the latest review feedback and retries, up to three review rounds.
10. If approved, the merge gate waits for PR checks, verifies the PR is mergeable, and squash-merges the exact reviewed head.
11. The backlog/state are updated and the next ready feature is dispatched automatically.

## Bootstrap status

`AWH-AUTO-001` is already implemented directly as the bootstrap commit because the loop cannot safely bootstrap itself. Future ready features are expected to go through the full three-agent pipeline.

## Failure behavior

The loop is deliberately fail-closed. Missing required LLM credentials, failed Rust checks, non-approved review state, merge conflicts, or exhausted review rounds stop the current feature instead of silently merging or skipping work.

The existing Rust CI and release workflows remain separate. The autonomous loop is an orchestration layer, not a replacement for repository CI.
