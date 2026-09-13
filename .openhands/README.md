# Autonomous AWH Development Loop

This directory defines the repository-native control plane for sequential AI development.

## Roles

1. **Agent 1 — Orchestrator**: selects the next ready feature, creates a precise task contract, validates review/CI results, merges only when every gate passes, then starts the next feature.
2. **Agent 2 — Builder**: works only on the assigned feature branch, edits the smallest safe surface, runs Rust verification, and leaves a clean commit for the workflow to publish as a PR.
3. **Agent 3 — Reviewer**: independently reviews the complete PR with the OpenHands PR-review plugin and must approve only when correctness, architecture, security, tests, and acceptance criteria are satisfactory.

## State machine

`IDLE -> PLANNING -> BUILDING -> PR_OPEN -> REVIEWING -> MERGING -> COMPLETED -> PLANNING`

A review that requests changes enters `FIXING -> REVIEWING`. Three unsuccessful review rounds stop the loop in `BLOCKED` rather than looping forever.

## Safety rules

- Only one autonomous feature may be active at a time.
- The base branch is `rust`.
- The loop never force-pushes a feature branch.
- Agent-generated code must pass the existing Rust CI gates.
- Agent 1 never merges a PR unless the latest OpenHands review is `APPROVED` and required CI checks are successful.
- Automation tokens are stored only in GitHub Actions secrets.
- The agents must not modify `.github/workflows/` unless the feature explicitly requires workflow changes.
- Never disable or weaken an existing test, security gate, audit, or release check to make a feature pass.

## Required secrets

- `AWH_AUTOMATION_TOKEN`: fine-grained GitHub token with Contents: Read/Write, Pull requests: Read/Write, Actions: Read/Write for this repository. It is used only for cross-workflow dispatches and automation pushes/PRs.
- `AWH_LLM_API_KEY`: API key for the OpenHands SDK agents.
- `AWH_LLM_BASE_URL`: optional OpenAI-compatible base URL. Leave unset when the selected provider uses the SDK default.
- `AWH_ORCHESTRATOR_MODEL`: optional model identifier; default is `gpt-5.5`.
- `AWH_BUILDER_MODEL`: optional model identifier; default is `gpt-5.5`.
- `AWH_REVIEWER_MODEL`: optional model identifier for the OpenHands review action.

The workflow is intentionally fail-closed when required secrets are missing.

## Start

Run **AWH Autonomous Development Loop** from GitHub Actions with `start=true`. The workflow then selects the first `ready` feature. Use `feature_id` only when manually starting a specific ready feature.
