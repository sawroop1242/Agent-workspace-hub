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

PR #153 (the `awh doctor` change) is merged into `rust`. Verify the command is present in the current checkout and run the setup-diagnostic acceptance cases before starting another code change. Fix demonstrated defects, then choose the highest-impact verified gap in controlled editing/recovery—not another broad infrastructure feature.


## 9. Repository-level feature disposition: simplify without breaking users

This section is the decision record for reducing active product scope. It is **not permission to delete modules by name**. The current repository has a broad target contract in `docs/FEATURES.md` and a multi-phase roadmap. Treat those as candidate capabilities, not a requirement to ship every command in the first useful release.

### 9.1 Keep and improve in the default developer experience

| Area | Decision | Why it stays | Required simplification |
|---|---|---|---|
| Install, upgrade, `init`, `status`, `doctor`, version/config | **Keep — P0** | Without predictable setup, no other capability is usable. | One short happy path; actionable diagnostics; no surprise writes during `doctor`. |
| MCP server and one verified client setup | **Keep — P0** | Connects AWH to agents developers already use. | Document one recommended path first; keep advanced transports/configuration out of the quick start. |
| Workspace identity and safe path handling | **Keep — P0** | Establishes the boundary for all workspace operations. | One canonical workspace model and path-validation implementation. |
| Filesystem read/search and controlled edit operations | **Keep — P0** | Directly supports coding-agent work. | One canonical edit service; explicit validation, conflict detection, atomicity and post-edit verification. |
| Recovery, snapshots/rollback and provenance | **Keep — P0, but narrow** | Recovery is the trust differentiator. | Tie snapshots to actual consequential edits; avoid a separate general-purpose backup platform. Do not advertise rollback until tested end to end. |
| Policy/capability checks and redacted audit | **Keep — P0** | Protects file operations and explains agent actions. | One authoritative enforcement path; safe defaults; clear denial reasons; never log secrets. |
| Git read/status/diff and essential mutations | **Keep — P1 after safe editing** | Developers need to understand and validate changes. | Prioritize status/diff/validate. Gate destructive or remote mutations; don't expose every Git subcommand in the beginner path. |
| Context and minimal task/session handoff | **Keep — P1** | Helps developers continue work across sessions. | Start with a small explicit handoff record; do not build an autonomous planning or orchestration engine. |
| Tests, release artifacts, clear docs and supported-platform checks | **Keep — required foundation** | Reliability and installability are product features. | Test the promised core path first; explicitly label unsupported platforms and unverified features. |

### 9.2 Disable or hide from the default experience; do not necessarily delete the implementation

These items should not appear in the first-run guide, default quick-start, or core release promise until they have a verified user need and passing acceptance tests. “Disable” means remove from the default path or guard behind an explicit opt-in/configuration; it does **not** mean silently breaking existing users or removing a security control.

| Area / roadmap family | Decision | Repository action to plan | Conditions to re-enable/promote |
|---|---|---|---|
| Remote AWH, hosted control plane, tunnels and remote execution | **Defer / opt-in only** | Keep out of the local-first quick start. Review deployment workflows and remote API examples for accidental “core requirement” language. Preserve authentication/TLS code used by supported local/server flows. | A concrete user use case, threat review, deployment test and documented support model. |
| Advanced infrastructure adapters (Docker/WASM sandboxes, resource management, secret backends) | **Defer** | Do not add new adapters or make them mandatory dependencies. Keep existing isolation/security primitives that are already required by current code. | A named supported scenario and tests for failure/isolation boundaries. |
| Broad enterprise RBAC, multi-tenant administration, quotas and approval workflows | **Defer** | Remove from near-term milestones and default examples; keep policy enforcement itself. Do not implement task/agent scopes before their identities and semantics exist. | A real multi-user requirement, defined identity model and authorization tests. |
| Large connector catalogue and duplicate provider-specific integrations | **Defer / optional** | Keep only connectors proven to work and used by the supported path. Mark others experimental; remove dead registrations/config/docs only after source and CI reference audit. | A maintained integration test, owner, clear user value and documented credentials/scope. |
| Multi-agent swarms, distributed collaboration, autonomous schedulers and generic DAG/workflow engine | **Out of core scope** | Stop adding new core abstractions and CLI promises for swarms/schedulers. Keep simple task/handoff primitives that serve the single-developer loop. | Strong demand that cannot be handled by existing agents or focused external tools. |
| Full agent runtime, model selection/router, prompt orchestration | **Out of core scope** | Do not implement as AWH-owned runtime responsibilities. Remove duplicate roadmap commitments; retain thin compatibility/adaptor interfaces only where needed. | Reconsider only if the product boundary changes through an explicit decision record. |
| Large dashboard, second control plane, duplicate TUI/API commands | **Defer / consolidate** | Prefer CLI + MCP as the first-class path. Keep TUI/API only where they call the same application services and add demonstrated value; don't maintain duplicate business logic. | Repeated user demand and tests proving the interface is a thin client over canonical services. |
| Generic plugin/skill marketplace or unreviewed executable packages | **Defer** | No marketplace or automatic execution by default. Keep existing skill metadata/policy controls where referenced by source. | Trust, provenance, capability review, lifecycle and uninstall semantics are implemented and tested. |
| Excessive shell completions, cosmetic screens, duplicate status/roadmap docs | **Defer / consolidate** | Prioritize correctness and docs accuracy over polish. Keep one canonical live status and preserve history in `docs/archive/`. | Core loop is stable and the enhancement reduces measured user friction. |

### 9.3 Remove from the repository only after a dependency audit

Candidate removal categories—not an assertion that every item currently exists—are:

- Dead feature modules that are unreachable from the CLI/MCP/API and have no supported public API or required tests.
- Duplicate services, models, policy checks, stores, or edit paths that compete with the canonical implementation.
- Abandoned experimental connectors, adapters, commands, and feature flags that have no owner, tests, or documented supported use.
- Stale active-roadmap promises and duplicated “current status” documents that conflict with the canonical docs.
- Temporary local experiment notes or generated artefacts that are not fixtures, release inputs, provenance, or useful historical records.

Do **not** remove these just to reduce line count: PolicyEngine checks, path validation, authentication, redaction, audit, rollback/recovery, conflict detection, release signing/checksums, CI tests, migration code, or compatibility shims with known users. A feature that is difficult to maintain may still be a critical security boundary.

### 9.4 Mandatory audit before disabling or deleting any code

For each candidate, record the following in the PR description or a short removal checklist:

1. **Source inventory:** locate its Rust module, public types, CLI subcommands, MCP tools/resources, API routes, config keys, dependencies and feature flags. Search the whole repository, not only `docs/`.
2. **Reachability:** identify all call sites and entry points; determine whether the code is compiled by default, gated by Cargo features, or exposed at runtime.
3. **Contract inventory:** check `README.md`, `docs/FEATURES.md`, `docs/CLI.md`, `docs/mcp.md`, configuration docs, examples, tests, CI/release workflows and implementation prompts.
4. **Safety classification:** label the code as product capability, security control, compatibility layer, test fixture, build/release dependency or dead code. Security controls are never removed as “feature cleanup.”
5. **Usage and compatibility:** inspect open PRs/issues and recent history; identify existing users, config files, data formats and scripts that may depend on the command.
6. **Decision:** choose one explicit outcome: `keep`, `simplify`, `disable-by-default`, `deprecated`, or `remove`. State the user impact and replacement path.
7. **Small reversible change:** prefer a feature flag or an explicit experimental label before deletion when reachability or usage is uncertain. Do not introduce a flag that leaves two competing implementations active.
8. **Verification:** run formatting, lint, build, relevant unit/integration tests, CLI help/snapshot tests, MCP schema/client tests, and release workflow checks as applicable. Add a regression test for the behavior that remains.
9. **Docs and release notes:** update every affected contract and example; document migration/removal when users may rely on the feature.
10. **Evidence:** report exact commands run and results. A check that could not run is blocked, not passed.

### 9.5 Core simplification order

Work in this order; do not start by deleting broad subsystems:

1. **Inventory reality:** compare `docs/FEATURES.md`, `docs/CLI.md`, MCP/API surfaces and Rust source. Mark each feature implemented-and-tested, partial, planned, unreachable, or unknown.
2. **Reduce promises:** identify the supported “first successful session” path in the README and CLI help. Move deferred features out of the quick start and near-term roadmap without misrepresenting their implementation status.
3. **Unify paths:** make CLI, MCP and API call the same canonical workspace, policy and edit services. Remove duplicate logic only after tests show the replacement preserves behavior.
4. **Finish safe edits:** test denied edits, stale edits, atomic failure, verification failure, snapshots, rollback and redacted audit.
5. **Prove one client:** validate the documented MCP setup against a real supported coding-agent client from a clean workspace.
6. **Simplify output/configuration:** make common commands short and errors actionable; retain advanced knobs for explicit configuration rather than exposing them to every new user.
7. **Defer advanced scope:** remote runtime, enterprise administration, large connector catalogues and distributed collaboration remain non-blocking until usage evidence changes the priority.
8. **Review with developers:** measure time to first successful agent operation, setup failure rate, successful recovery rate and whether users return to AWH.

### 9.6 Definition of “simpler and useful”

A release is simpler only if it reduces the number of concepts a new user must learn **without weakening safety or breaking supported workflows**. Acceptance criteria:

- [ ] A newcomer can follow one quick start without needing remote hosting, a connector catalogue, a dashboard or multiple agent profiles.
- [ ] Every command in the quick start exists and is tested; advanced commands are clearly marked optional or experimental.
- [ ] There is one canonical policy decision path and one canonical edit path.
- [ ] A denied or stale edit produces no unintended filesystem side effects.
- [ ] A failed edit can be recovered, and the recovery result is verified from the filesystem.
- [ ] Audit output is understandable and redacts secrets.
- [ ] Deferred features do not become required build/runtime dependencies for the core local workflow.
- [ ] Removing or disabling a feature has a documented dependency audit, tests and migration note where needed.
- [ ] The README, CLI docs, MCP docs, feature contract and roadmap agree on what is supported.
- [ ] No roadmap percentage or documentation statement is used as a substitute for source/test evidence.

## 10. Immediate implementation ticket to create from this plan

**Do not start with a mass deletion PR.** The first code task should be a source-backed capability inventory and core-path test report:

- List each public CLI command, MCP tool/resource, API route and Cargo feature.
- Map each to its implementation module, tests, dependencies and security boundary.
- Mark `keep / improve`, `hide from default path`, `defer`, or `candidate for removal`.
- Identify the smallest set of fixes needed to complete setup → MCP client → controlled edit → inspect → rollback.
- Submit a focused PR that changes only verified gaps and includes regression tests.

Only after that inventory is reviewed should a separate PR disable or remove an identified non-core capability. Keep each removal small enough to review and revert independently.
