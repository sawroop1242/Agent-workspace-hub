# Autonomous development setup

The AWH repository now contains a sequential GitHub Actions control plane for three OpenHands agents:

`Agent 1 (plan) -> Agent 2 (build) -> PR -> Agent 3 (review) -> CI gate -> Agent 1 (merge) -> next feature`

GitHub `repository_dispatch` is used as the event bus. This is intentional: GitHub documents that `repository_dispatch` and `workflow_dispatch` create workflow runs, while ordinary `GITHUB_TOKEN` activity does not generally create recursive workflow runs. The automation therefore uses a dedicated `AWH_AUTOMATION_TOKEN` for cross-workflow dispatches and branch/PR operations.

## Required repository secrets

Create these under **Settings -> Secrets and variables -> Actions**:

- `AWH_AUTOMATION_TOKEN` — fine-grained token for this repository with Contents read/write, Pull requests read/write, and Actions read/write.
- `AWH_LLM_API_KEY` — OpenHands SDK-compatible LLM API key.
- `AWH_LLM_BASE_URL` — optional OpenAI-compatible endpoint.
- `AWH_ORCHESTRATOR_MODEL` — optional; defaults to `gpt-5.5`.
- `AWH_BUILDER_MODEL` — optional; defaults to `gpt-5.5`.
- `AWH_REVIEWER_MODEL` — optional; defaults to `gpt-5.5`.

Do not put API keys in repository files or workflow YAML.

## Starting the loop

1. Make sure the `rust` branch is the repository default branch (it currently is).
2. Add the required secrets.
3. Open **Actions -> AWH Autonomous Development Loop**.
4. Choose **Run workflow**, keep `start=true`, and optionally enter a specific ready feature such as `AWH-AUTO-001`.
5. Agent 1 selects/plans the feature and dispatches Agent 2.
6. Agent 2 creates or updates `feature/<feature-id>`, runs deterministic Rust verification, and opens a PR.
7. Agent 3 reviews the PR using the official OpenHands PR-review action.
8. If changes are requested, Agent 2 gets the latest review feedback and retries, up to three review rounds.
9. If approved, the merge gate waits for PR checks, verifies the PR is mergeable, and squash-merges the exact reviewed head.
10. The backlog/state are updated and the next ready feature is dispatched automatically.

## Failure behavior

The loop is deliberately fail-closed. Missing LLM credentials, failed Rust checks, non-approved review state, merge conflicts, or exhausted review rounds stop the current feature instead of silently merging or skipping work.

The existing Rust CI and release workflows remain separate. The autonomous loop is an orchestration layer, not a replacement for repository CI.
