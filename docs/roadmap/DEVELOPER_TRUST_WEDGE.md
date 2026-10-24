# Developer Trust Wedge: Product Focus and Execution Plan

> **Status:** Active product decision record for the `rust` branch.
>
> **Purpose:** Turn the repository's broad roadmap into a small, demonstrable workflow that individual coding-agent developers will actually adopt. This document guides prioritization; it does not claim planned features are implemented.

## 1. Product decision

AWH should first be the **local safety and continuity layer for existing coding agents**—not a new agent framework, model router, cloud platform, or general-purpose automation engine.

**Core promise:** Let a developer connect an existing coding agent to a project, understand what AWH is protecting, and recover from a bad agent change without reconstructing the project state manually.

The smallest credible product loop is:

1. Install AWH and initialize a workspace.
2. Run a read-only diagnostic that explains setup problems and next steps.
3. Connect one existing agent through the documented MCP transport.
4. Make a controlled file change through AWH's canonical service path.
5. Inspect what changed and why it was allowed or denied.
6. Restore the prior state using a tested recovery mechanism.
7. Start a fresh agent session and recover useful project context.

Do not advertise this entire loop as available until each step is verified against the current Rust source, tests, and a real client.

## 2. Developer pain and value proposition

| Developer pain | AWH value to prove | Evidence required |
|---|---|---|
| Agent setup is confusing and failures are hard to diagnose | A short install/init/doctor path with actionable errors | Fresh-install walkthrough on supported platforms; failure-case tests |
| Agents can overwrite or corrupt useful work | Controlled edits, conflict detection, snapshots and rollback | Integration test that edits a file, injects a failure, and proves recovery |
| A second agent starts without project decisions or progress | Durable workspace context, memory and task handoff | Two-session handoff test with no hidden prompt-only state |
| MCP tools have unclear permissions | One authoritative policy decision before side effects | Denial tests prove the protected operation did not execute |
| Debugging agent actions is difficult | Searchable, redacted audit/provenance records | Tests cover allowed, denied, failed and recovered operations without leaking secrets |

These are product hypotheses, not proof of market demand. Validate them with developers who already use Claude Code, Codex, OpenCode, or another MCP-capable coding agent. Ask them to complete the workflow and observe where they get stuck before expanding scope.

## 3. Feature triage

### P0 — Make the core loop trustworthy

- Reliable install, upgrade and platform-specific release artifacts.
- `awh init` plus `awh doctor` with accurate, read-only diagnostics.
- One documented MCP setup path for a supported external agent.
- Controlled file-edit transactions with validation, stale-state conflict detection and atomic application.
- Snapshot/rollback or an explicitly documented safe recovery path.
- A single policy boundary and meaningful audit events for consequential actions.
- Tests that prove no side effects occur after authorization or validation denial.

### P1 — Add daily-use continuity after P0 is proven

- Git worktree isolation where the source implementation and tests establish safe semantics.
- Context, developer memory and tasks integrated into a repeatable handoff.
- Agent/session identity only where it affects policy, workspace isolation, recovery or audit.
- Useful structured status/errors and a small, navigable CLI/TUI experience.

### Defer until real users request them

- Remote runtime, tunnels and hosted control-plane features.
- Broad enterprise RBAC, multi-tenant administration and complex quota systems.
- Large connector catalogues and integrations that duplicate capabilities already provided by the developer's agent.
- Advanced infrastructure adapters, storage optimization and generalized distributed collaboration.
- Cosmetic dashboards or additional command surfaces that do not improve the core loop.

These features are not declared permanently useless. They are deferred because they increase maintenance and security surface before the primary value is demonstrated.

### Remove or consolidate only with evidence

- Remove one-off test artefacts that are not product documentation, fixtures, or active CI inputs.
- Consolidate competing current-status documents into the canonical roadmap/status pages.
- Keep historical reports in `docs/archive/` when they provide provenance; do not treat them as current instructions.
- Do not delete a subsystem or prompt merely because it is large, unfinished, or optional. Check source references, CI, README links, roadmap dependencies and historical value first.

## 4. Scope boundaries

AWH owns workspace state, controlled editing, recovery, policy enforcement, audit, context and the interfaces that expose those services.

External agents own reasoning, planning, model choice and agent-specific orchestration. AWH must not become a competing model router or general workflow engine. OS/container/VM isolation remains a separate boundary; AWH policy and rollback are not substitutes for operating-system sandboxing.

CLI, MCP, TUI and Control API must call the same canonical application services. Do not create a second edit engine, authorization path, state store or audit model for an individual interface.

## 5. Execution sequence

### Step 0 — Establish the truth

- Read `AGENTS.md`, `Cargo.toml`, README, `docs/PROJECT_CONTEXT.md`, `docs/FEATURES.md`, architecture, security, threat model, canonical roadmap and current status.
- Inspect the actual Rust source and tests; documentation and command help are not implementation proof.
- Check open PRs and current branch head before duplicating work.
- Record each priority item as **implemented and tested**, **partially implemented**, **planned only**, or **blocked**.

### Step 1 — Complete setup diagnostics

- Verify `awh doctor` exists on the target branch and behaves as documented.
- Test uninitialized workspace, valid workspace, invalid manifest/policy, Git absent, `.agent` symlink, tracked state and missing ignore rules.
- Keep diagnostics read-only: no workspace initialization, policy mutation or audit writes while checking.
- Run formatting, lint, build and focused tests on an environment with the Rust toolchain.

### Step 2 — Prove controlled editing and recovery

- Inspect and reuse the existing editing services and implementation prompts 04–12.
- Close gaps in validation, conflict handling, atomic writes, post-edit verification, rollback and provenance.
- Test failures and concurrent/stale changes, not only the happy path.
- Demonstrate that a failed or denied edit does not leave partial changes.

### Step 3 — Prove one real-agent integration

- Use a supported MCP client and a clean sample workspace.
- Document exact configuration, expected tools, transport requirements and troubleshooting.
- Verify that denied actions have no side effects and that secrets are not exposed in logs.
- Avoid adding provider-specific runtime logic to the AWH core.

### Step 4 — Prove handoff and repeat use

- Run two separate agent sessions against the same workspace.
- Confirm that context/tasks/memory expose the decisions and progress needed by the next session.
- Verify scope boundaries and audit history.
- Measure setup time, successful task completion, recovery success and user friction.

### Step 5 — Expand only from observed demand

Use developer interviews, issue reports and repeat usage to choose the next P1 feature. Do not use the number of roadmap items or percentage completion as the product success metric.

## 6. Acceptance gates

The trust wedge is not ready to promote as a reliable developer product until all applicable gates pass:

- [ ] A fresh user can install and initialize using documented steps.
- [ ] Diagnostics give actionable output and do not mutate workspace state.
- [ ] At least one real MCP client can connect using the documented configuration.
- [ ] A controlled edit succeeds and its result is verified from the filesystem.
- [ ] A stale or unauthorized edit is rejected without unintended side effects.
- [ ] A recovery test restores the original file content after a simulated failed/bad edit.
- [ ] Audit/provenance is useful and contains no secret values.
- [ ] Relevant Rust formatting, lint, build and test gates pass in a supported environment.
- [ ] Documentation labels implemented, partial, planned and blocked functionality accurately.
- [ ] At least a small set of target developers can complete the workflow and identify a reason to keep using it.

If a gate cannot be run, report it as blocked; do not treat an unexecuted test as passing.

## 7. How to use the existing implementation prompts

Use `docs/implementation-prompts/README.md` as the canonical prompt index. Start with repository forensics, then select only the prompt that closes a verified gap. In particular:

- `01-init-runtime-contracts.md`: workspace/runtime foundation.
- `03-mcp-routing-and-security.md`: MCP routing and policy boundary.
- `04-edit-transaction-model.md` through `07-edit-authorization.md`: controlled editing and authorization.
- `08-snapshots-provenance.md` and `09-rollback-recovery.md`: recovery and evidence.
- `10-persistent-audit.md`: durable audit.
- `16-testing-and-acceptance.md`: regression and acceptance gates.
- `17-contract-status-and-roadmap.md`: reconcile documented contracts with actual source.

Prompts are implementation specifications, not evidence that their features exist. Before implementing, inspect the current Rust modules, tests, open PRs and current branch. Reuse working code and update status only after verification.

## 8. Immediate next action

PR #153 (the `awh doctor` change) is merged into `rust`. Verify the command is present in the current checkout and run the setup-diagnostic acceptance cases before starting another code change. Fix demonstrated defects, then choose the highest-impact verified gap in controlled editing/recovery—not another broad infrastructure feature. After that, choose the highest-impact verified gap in controlled editing/recovery—not another broad infrastructure feature.
