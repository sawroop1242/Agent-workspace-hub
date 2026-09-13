# AWE-019 — Issue-Resolving Master Prompt

## Subject

**Issue #40 — `docs: roadmap, implementation status, known issues and strategic analysis report`**

## Mission

Act as the repository's senior technical auditor, documentation engineer, security reviewer, and product-strategy analyst. Resolve Issue #40 on the `rust` branch by producing a **verification-backed, implementation-accurate roadmap, implementation-status, known-issues, and strategic-analysis report**.

This is a documentation/audit task. The report must describe the repository as it actually exists at the time of implementation. Do not treat PR descriptions, roadmap aspirations, stale documents, generated summaries, or assumptions as proof of implementation.

The resulting document must be useful to maintainers, contributors, security reviewers, users, and technical/product leadership deciding what AWH should build next.

**Hard constraint:** do not claim a feature is implemented unless the current `rust` branch and/or directly verifiable CI evidence proves it.

---

# 1. Repository and scope preflight

Before writing or changing the report:

1. Confirm the repository is `sawroop1242/Agent-workspace-hub`.
2. Confirm the target branch is `rust`.
3. Inspect the current repository status and recent history.
4. Read the current status/architecture documents relevant to roadmap and completeness claims, including where present:
   - `docs/PROJECT_STATUS.md`
   - `docs/completeness-audit.md`
   - `AWH_FORENSIC_REPORT.md`
   - `RUFLO_AWH_ARCHITECTURE_ANALYSIS.md`
   - current roadmap/security documentation
5. Inspect the actual source and tests for every subsystem whose completion or implementation status will be reported.
6. Inspect GitHub issues, PRs, merge state, and relevant commits for milestone claims.
7. Inspect current GitHub Actions workflows and actual results when reporting test counts or CI health.
8. Establish and record the exact branch/commit baseline used for the report.
9. Verify whether `docs/ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md` exists. If it exists, update it rather than creating a duplicate.

Do not treat previous reports, issue descriptions, PR descriptions, comments, or README claims as authoritative implementation evidence by themselves.

---

# 2. Evidence hierarchy

Use this authority order for factual implementation claims:

1. Current source code on `rust`.
2. Current automated tests and their actual results.
3. Current GitHub Actions workflow results and merge state.
4. Git history and merged PRs.
5. Current architecture/project documentation.
6. Open issues/PR descriptions.
7. Historical plans and previous reports.
8. External market research for market claims only.

When evidence conflicts, prefer the higher-authority source and document the discrepancy when it materially affects the report.

Never convert:

- a data model into an enforced capability;
- a CLI declaration into a verified command;
- a roadmap item into implementation;
- a test name into proof that production behavior works;
- a PR description into proof that a PR merged;
- a source-scan regression test into mutation testing;
- a design document into an implemented subsystem.

---

# 3. Required report

Create or update:

`docs/ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md`

The report must contain at least the following sections.

## 3.1 Executive summary

State:

- what AWH actually is today;
- current implementation maturity;
- major shipped capabilities;
- major shipped security foundations;
- largest remaining gaps;
- current roadmap phase;
- highest-value near-term priorities;
- strategic positioning recommendation.

Keep implementation facts separate from strategic interpretation.

## 3.2 Verification methodology and status model

Document how status was determined and use explicit states such as:

- **Implemented** — present on the current branch and supported by meaningful verification.
- **Implemented / partially validated** — implementation exists but acceptance or interoperability evidence is incomplete.
- **In progress** — active implementation exists but acceptance is incomplete.
- **Planned / not started** — roadmap intent exists without sufficient implementation evidence.
- **Blocked / unresolved** — a concrete dependency or defect prevents completion.
- **Intentionally deferred** — deliberately postponed for architectural or sequencing reasons.
- **Beyond current roadmap** — vision-level work not currently phase-scoped.

Do not use ambiguous percentages without explaining their measurement basis.

## 3.3 Current implementation status

Provide a subsystem table covering the major AWH architectural subsystems.

For each subsystem include, where useful:

- subsystem name;
- previous estimate when meaningful;
- current status/estimate;
- implementation location;
- verification evidence;
- relevant issue/PR;
- what changed;
- known limitation;
- next action.

If percentages are used, label approximate values with `~` and explain the basis. Do not present a simple average as equivalent to product readiness.

At minimum investigate the status of the major MCP, workspace/filesystem, Git, skills, context, security, capability, policy, tool-broker, CLI, Control API, memory, observability, agent-runtime, messaging, workflow, scheduler, checkpoint/recovery, artifact/provenance, model-router, and TUI areas.

A subsystem is not complete merely because interfaces, types, documentation, or stubs exist.

## 3.4 Security-foundation roadmap

Document the security-foundation phases and classify each as:

- merged and verified;
- implemented but awaiting verification;
- specified/in flight;
- not started;
- intentionally deferred.

For completed phases include:

- scope;
- actual PR/commit reference;
- exact CI/test evidence when available;
- security problem solved;
- remaining limitations.

For future phases include:

- purpose;
- dependencies;
- sequencing rationale;
- explicit non-goals.

Do not claim a future phase has started merely because a prompt exists.

## 3.5 Full feature ledger

Separate features into:

### Already built

Only verified capabilities.

### Added in the current cycle

Only changes demonstrably merged on the branch.

### In progress

Only work with concrete evidence of active implementation.

### Upcoming

Planned work not yet implemented.

### Beyond current roadmap

Vision-level capabilities not yet phase-scoped.

For every important feature, identify its evidence or implementation location when practical.

---

# 4. Known-issues audit

Identify the meaningful current issues/gaps affecting AWH.

For each issue provide:

1. identifier/title when one exists;
2. observable current behavior;
3. evidence/source location;
4. affected subsystem;
5. severity/impact;
6. whether it is blocking, high priority, non-blocking, cosmetic, or intentionally deferred;
7. root-cause hypothesis only when supported by evidence;
8. architectural reason for its current sequencing;
9. recommended resolution phase;
10. concrete future implementation prompt when useful;
11. dependencies/blockers;
12. validation/acceptance condition.

At minimum investigate:

- capability data model versus actual per-agent enforcement;
- custom/dynamic MCP authorization versus built-in authorization;
- skills executing without the desired sandbox boundary;
- deny-only/workspace-scoped policy limitations;
- distribution and packaging verification;
- duplicate agent/grant identifier behavior;
- test terminology where source scans are described too strongly.

Do not invent issues merely to reach a target count. Distinguish intentional sequencing from accidental omission.

When an existing `docs/issue-resolving-prompts/AWE-*.md` already covers a future issue, cross-reference it rather than duplicating its implementation plan.

---

# 5. Verification methodology

The report must explain how status claims were verified.

Use, where available:

- direct source inspection;
- exact GitHub merge status;
- real GitHub Actions workflow/job results;
- exact test counts from raw job output rather than PR summaries;
- integration tests;
- command/help validation for CLI claims;
- real MCP interoperability evidence for MCP claims;
- security regression evidence for security claims.

For every important numerical claim, identify where the number came from.

Clearly separate:

- verified facts;
- estimates;
- technical interpretation;
- strategic recommendations.

If evidence is unavailable, say so rather than inventing a number or status.

Historical CI results must be labelled historical and must not be presented as current-branch verification unless rechecked.

---

# 6. CI and implementation-status discipline

When reporting test or CI evidence:

1. inspect the actual workflow/job result;
2. identify the relevant commit/PR;
3. distinguish tests that exist from tests that actually passed;
4. distinguish historical verification from current verification;
5. state limitations when a check was not run or cannot be independently verified.

Never manufacture test counts.

Do not describe a PR as merged until the actual merge state is verified.

---

# 7. Market and competitive analysis

Perform a current, dated competitive analysis only where it materially informs product strategy.

Investigate relevant categories such as:

- MCP gateways/tool brokers;
- MCP security infrastructure;
- coding-agent memory/context systems;
- agent orchestration frameworks;
- agent runtimes;
- terminal-native developer-agent tooling;
- Rust-native/local-first developer infrastructure.

For every material external claim:

- identify the source;
- record the research date;
- distinguish fact from interpretation;
- provide a confidence level where uncertainty exists.

Do not present vendor marketing claims as independently verified facts.

Do not merely list competitors. Explain:

- which categories are crowded;
- where AWH overlaps;
- where AWH has meaningful differentiation;
- where competing head-on is strategically weak;
- where ecosystem integration is better than replacement.

The market section is decision support, not marketing copy.

---

# 8. Strategic positioning recommendation

Use the implementation audit and market research to answer:

1. What should AWH **not** try to become?
2. What should remain core infrastructure?
3. What should be prioritized next?
4. Which capabilities are table stakes versus differentiation?
5. Which capabilities should integrate with existing ecosystems rather than be rebuilt?
6. What is the strongest defensible product bundle supported by the current architecture?

Evaluate the strategic thesis around a developer-focused, Rust-native, local-first workspace combining capabilities such as:

- MCP infrastructure;
- workspace/file operations;
- context;
- memory;
- skills;
- Git;
- security;
- TUI/observability.

Do not declare this thesis correct merely because it is coherent. Tie it to verified implementation strengths and competitive evidence.

Explicitly assess whether AWH should compete directly with:

- generic MCP gateways;
- generic memory servers;
- workflow/DAG frameworks;
- model routers;
- large agent orchestration frameworks.

Where interoperability is strategically superior to replacement, say so.

---

# 9. Roadmap prioritization

Convert the audit into an actionable priority order using at least:

- security impact;
- user value;
- architectural dependency;
- implementation feasibility;
- differentiation;
- adoption/distribution impact;
- maintenance burden.

Classify recommendations as:

- **P0 — table stakes/blocking**;
- **P1 — strategic near-term**;
- **P2 — later/optional**;
- **Avoid/delegate — work better supplied by the ecosystem**.

For every priority provide a reason and a verification/acceptance criterion.

Do not let a large vision roadmap hide foundational gaps.

If distribution/packaging is a meaningful adoption blocker, evaluate it independently rather than assuming it belongs after every architectural phase.

---

# 10. Future implementation prompts

Where a known issue is sufficiently mature, include a concrete implementation prompt for future work.

Each prompt should specify:

- exact files/subsystems to inspect first;
- current behavior that must be preserved;
- desired behavior;
- security invariants;
- compatibility constraints;
- tests required;
- acceptance criteria;
- explicit non-goals.

Prompts must be implementation-specific rather than vague requests such as “improve security.”

Do not implement those future issues as part of Issue #40 unless Issue #40 itself explicitly requires implementation.

---

# 11. Roadmap and documentation consistency

Cross-check the report against:

- existing roadmap documents;
- project status;
- completeness audits;
- architecture/forensic reports;
- open issues;
- merged PRs;
- current source tree.

Resolve contradictions explicitly.

Do not silently rewrite historical documents to make them agree with the new report.

Preserve the distinction among:

- architecture documents;
- implementation-status documents;
- forensic/audit reports;
- roadmap documents;
- strategic analysis.

The Issue #40 report should be a synthesis/status document, not an unauthorized replacement for foundational architecture documents.

---

# 12. Architecture guardrails

Preserve these AWH boundaries:

- AWH is not a generic autonomous-agent framework.
- AWH is not a generic workflow/DAG platform.
- AWH is not a generic model router.
- MCP-first does not mean MCP-only.
- CLI, MCP, TUI, and Control API should remain interfaces over shared services rather than independent implementations of the same semantics.
- Security-sensitive mutations must remain policy/capability/session aware.
- Snapshots are not a replacement for sandboxing.
- Composable adapters are preferred over hard-coded integrations.
- Complete workflow validation is more important than feature-count inflation.
- Distribution and operational usability are product features, but must not be confused with core correctness.

Do not recommend architectural changes that violate these constraints without explicitly identifying the trade-off and requiring a separate implementation decision.

---

# 13. Security and sensitive-data requirements

Never expose:

- API keys;
- bearer tokens;
- credentials;
- private user data;
- CI secrets;
- sensitive filesystem contents.

Do not paste raw secret-bearing CI logs.

When describing security controls, distinguish among:

- authentication;
- trust;
- capability authorization;
- policy authorization;
- sandboxing;
- audit/observability.

Do not imply that one layer substitutes for another.

For example, a capability data model without an enforcement call site is not per-agent authorization.

---

# 14. Documentation quality

The report must be:

- concise enough to remain maintainable;
- detailed enough for engineering decisions;
- internally consistent;
- traceable to repository paths/issues/PRs where useful;
- explicit about evidence and uncertainty;
- free of stale copied claims that are no longer true.

Use tables when they improve scanability, but avoid giant unreadable tables.

Prefer exact paths, issue numbers, PR numbers, commit identifiers, and test names when they materially improve traceability.

---

# 15. Change-scope rules

Issue #40 is documentation/audit/strategy work.

Unless the issue's evidence proves that a documentation-only correction is impossible, modify only:

`docs/ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md`

Do **not** modify:

- Rust source files;
- tests;
- Cargo manifests;
- workflows;
- configuration;
- existing issue-resolving prompt files;
- README files;
- generated artifacts;
- unrelated documentation.

If the report cannot be completed accurately without another file change, STOP and report the exact blocker.

---

# 16. Validation of the finished report

After creating/updating the report:

1. Re-read the complete document.
2. Check every implementation claim against current repository evidence.
3. Check every issue/PR reference for correctness.
4. Check every roadmap status for consistency.
5. Check historical versus current CI claims.
6. Check that strategic speculation is explicitly labelled.
7. Check that no unsupported completion percentage or feature claim remains.
8. Check Markdown formatting and repository-relative links.
9. Confirm only the intended documentation file changed.
10. Run appropriate documentation/static checks when available without changing unrelated files.
11. Do not claim full Rust CI was run unless it actually was run and passed.

When practical, the standard repository validation remains:

```text
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Do not weaken CI or modify CI configuration to make the documentation task pass.

---

# 17. Acceptance criteria

AWE-019 is complete only when:

- [ ] `docs/ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md` exists on `rust`.
- [ ] The report directly addresses Issue #40.
- [ ] The 21-subsystem status is evidence-backed.
- [ ] Security-foundation roadmap status is verified.
- [ ] The feature ledger distinguishes implementation from plans.
- [ ] Known issues have concrete, evidence-backed resolution paths.
- [ ] Strategic/market analysis is clearly separated from repository facts.
- [ ] External strategic claims are dated and confidence-labelled where necessary.
- [ ] Roadmap recommendations are actionable and prioritized.
- [ ] Existing AWH architecture guardrails are preserved.
- [ ] No source-code behavior is changed by this issue.
- [ ] No unsupported implementation claim remains.
- [ ] The document is internally consistent and maintainable.
- [ ] The final diff contains only the intended documentation file.

---

# 18. Definition of Done

The issue is DONE only if the final report is a reliable engineering/strategy reference that a new contributor can use to understand:

1. what AWH actually implements today;
2. what is partially implemented;
3. what is actively being built;
4. what is merely planned;
5. what is blocked or known-broken;
6. what the highest-priority technical work is;
7. what strategic direction is supported by the available evidence.

The report must be more trustworthy than a copy of issue/PR descriptions because its status claims are independently checked against the repository and CI evidence.

---

# 19. Explicit non-goals

Do not use AWE-019 to:

- implement unrelated source-code features;
- redesign AWH architecture;
- introduce a new agent framework;
- introduce a generic workflow engine;
- introduce model routing;
- replace existing security mechanisms;
- replace the editing architecture;
- create a second audit/provenance/snapshot system;
- silently close or rewrite unrelated issues;
- inflate project maturity for marketing purposes;
- treat speculative market analysis as factual evidence.

---

# 20. Final implementation report

When Issue #40 is complete, report:

1. exact target file created/updated;
2. branch and baseline commit used;
3. evidence sources inspected;
4. major report sections added/updated;
5. implementation-status conclusions;
6. roadmap conclusions;
7. known issues identified;
8. strategic recommendation;
9. tests/validation executed and results;
10. evidence limitations or unresolved uncertainty;
11. exact final diff scope;
12. explicit confirmation that no unrelated repository file was modified.

Do not claim Issue #40 is fully verified if any major status claim remains unsupported. State the limitation explicitly.

---

# HARD STOP

After completing AWE-019:

1. **Do not begin AWE-020 or any later issue.**
2. **Do not modify unrelated repository files.**
3. **Do not convert strategic recommendations into implementation work.**
4. **Do not convert planned roadmap items into claimed implementation.**
5. **Do not invent verification evidence.**
6. **Do not weaken existing security controls to simplify the report.**
7. **Stop after the Issue #40 deliverable and final verification report.**
