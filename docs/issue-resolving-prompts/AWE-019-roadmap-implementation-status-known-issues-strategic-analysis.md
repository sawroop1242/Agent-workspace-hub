# AWE-019 — Issue-Resolving Master Prompt

## Subject

**Issue #40 — `docs: roadmap, implementation status, known issues and strategic analysis report`**

## Mission

Resolve Issue #40 as a production-quality, evidence-backed documentation deliverable on the `rust` branch.

The objective is to create and maintain `docs/ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md` as a trustworthy strategic/implementation-status report for Agent Workspace Hub (AWH), grounded in the repository's actual implementation, tests, CI evidence, roadmap artifacts, known issues, and explicitly dated strategic analysis.

This is a documentation task. Do **not** change runtime behavior or source-code architecture unless the existing repository evidence proves that a documentation claim cannot otherwise be made accurately. If implementation changes are genuinely required to establish a factual claim, STOP and report the blocker rather than silently expanding scope.

---

## 1. Mandatory repository preflight

Before editing anything:

1. Confirm the active branch is `rust`.
2. Inspect the repository status and recent commits.
3. Read the existing project-status and architecture documentation relevant to roadmap/completeness claims, including where present:
   - `PROJECT_STATUS.md`
   - `docs/completeness-audit.md`
   - `docs/AWH_FORENSIC_REPORT.md`
   - `docs/RUFLO_AWH_ARCHITECTURE_ANALYSIS.md`
   - existing roadmap or milestone documentation
   - existing issue-resolving prompts
4. Inspect the source and tests needed to validate every implementation-status claim.
5. Inspect GitHub issues/PRs and merge status when the report claims that a feature is merged, in progress, planned, blocked, or not started.
6. Inspect current CI evidence when test counts or verification status are reported.
7. Verify whether `docs/ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md` already exists on the target branch. If it exists, update it rather than creating a duplicate.

Do not treat previous reports, issue descriptions, PR descriptions, comments, or README claims as authoritative implementation evidence by themselves.

---

## 2. Evidence hierarchy

Use the following authority order for factual implementation claims:

1. Current source code on `rust`.
2. Current automated tests and their actual results.
3. Current CI/workflow results and merge status.
4. Git history and merged PRs.
5. Current architecture/project documentation.
6. Open issues/PR descriptions.
7. Historical plans, proposals, or previous reports.

When sources disagree, prefer the higher-authority source and explicitly document the discrepancy where it materially affects the report.

Never convert a planned feature into an implemented feature merely because a prompt, issue, roadmap item, or PR description says it exists.

---

## 3. Required report structure

Create or update:

`docs/ROADMAP_STATUS_AND_STRATEGIC_ANALYSIS.md`

The report should be organized into clear, maintainable sections. At minimum include:

### A. Executive summary

Explain:

- what AWH currently is;
- the current implementation maturity;
- the major security/foundation milestones completed;
- the most important remaining gaps;
- the strategic direction supported by the evidence.

Keep implementation facts separate from strategic interpretation.

### B. Implementation-status methodology

Document how status was determined and define explicit states such as:

- **Implemented** — present in the current branch and supported by meaningful verification.
- **Implemented / partially validated** — implementation exists but validation is incomplete or limited.
- **In progress** — active implementation exists but acceptance is incomplete.
- **Planned / not started** — roadmap intent exists without sufficient implementation evidence.
- **Blocked / unresolved** — a concrete dependency or defect prevents completion.
- **Not in current roadmap** — intentionally outside the committed roadmap.
- **Deprecated / superseded** — replaced by another design.

Do not use ambiguous percentages without explaining the measurement basis.

### C. 21-subsystem completion/status table

Provide the repository's current subsystem completion table, but derive every row from live evidence.

For each subsystem, include where useful:

- subsystem name;
- current status;
- implementation location(s);
- verification evidence;
- relevant issue/PR;
- known limitation;
- next action.

Avoid inflated completion claims. A subsystem is not "complete" merely because types, stubs, documentation, or interfaces exist.

### D. Security-foundation roadmap phase

Document the security-foundation roadmap accurately, including:

- completed milestones;
- currently active/in-flight work;
- not-started milestones;
- dependencies;
- evidence for merged milestones;
- remaining security risks.

If the report states that specific PRs are merged, verify their actual merge state instead of relying on issue text.

### E. Feature ledger

Maintain a complete feature ledger divided into categories such as:

- built;
- added recently;
- in progress;
- upcoming;
- beyond the current roadmap.

Every feature must have a defensible status and should identify its evidence or implementation location when practical.

### F. Known issues and resolution paths

Identify the major known issues currently affecting AWH.

For each issue provide:

- problem statement;
- affected subsystem;
- observable evidence;
- severity/impact;
- current state;
- likely root cause when supported by evidence;
- recommended resolution;
- validation required;
- dependencies/blockers.

Do not fabricate issues merely to fill a quota. If fewer than seven materially supported issues exist, report fewer and explain why.

Where an issue already has an issue-resolving master prompt under `docs/issue-resolving-prompts/`, cross-reference it rather than duplicating implementation instructions.

### G. Strategic/market analysis

Provide a clearly separated strategic analysis of AWH's positioning.

Distinguish:

- verified repository facts;
- technical interpretation;
- market/competitive observations;
- hypotheses;
- recommendations.

Any market or competitive claim that depends on external information must be dated and assigned an appropriate confidence level.

Do not present speculative competitive claims as settled facts.

### H. Recommended strategic direction

Give a concise, evidence-backed recommendation covering:

- what AWH should prioritize;
- what should be deferred;
- what should explicitly remain outside scope;
- which capabilities create the strongest product differentiation;
- which technical foundations must be completed before higher-level features.

Recommendations must respect AWH's existing architectural guardrails and must not redefine the project into a generic agent framework, workflow/DAG platform, or model router.

### I. Roadmap priorities

Turn the evidence into a practical priority order:

1. immediate blockers/security foundations;
2. core correctness/reliability;
3. integration/acceptance gaps;
4. developer/operator experience;
5. strategic product capabilities;
6. longer-term experiments.

Each priority should have a concrete reason and verification criterion.

### J. Conclusion

Summarize the current maturity, the most important gaps, and the recommended path forward without overstating certainty.

---

## 4. Forensic verification requirements

Before writing factual status claims, inspect the relevant implementation directly.

At minimum verify, where applicable:

- core service modules;
- CLI commands;
- MCP server/tool registry;
- Control API;
- TUI;
- context/session/agent/workspace services;
- policy and capability enforcement;
- filesystem/security helpers;
- editing services and tests;
- snapshot/provenance/audit facilities;
- automation workflows;
- integration tests;
- documentation structure.

For every major claim ask:

> "What concrete repository evidence proves this is implemented today?"

If there is no sufficient evidence, downgrade the status rather than guessing.

---

## 5. CI and verification evidence

When reporting test counts or CI status:

1. inspect the actual workflow/job result;
2. identify the relevant commit/PR;
3. distinguish passing tests from merely existing tests;
4. distinguish current-branch verification from historical verification;
5. state limitations when a check was not run or is unavailable.

Never manufacture test counts.

If the report uses historical CI numbers, label them as historical and do not imply that they represent the current branch unless verified.

---

## 6. Roadmap consistency

Cross-check the report against:

- existing roadmap documents;
- project status;
- completeness audits;
- architecture reports;
- open issues;
- merged PRs;
- current source tree.

Resolve contradictions explicitly.

Do not silently rewrite roadmap history. If priorities changed, describe the transition and why the current recommendation differs.

---

## 7. Strategic-analysis discipline

The strategic section must not contaminate implementation-status claims.

Use explicit labels such as:

- **Repository fact**
- **Verified implementation status**
- **Technical assessment**
- **Market observation**
- **Strategic hypothesis**
- **Recommendation**

For external market/competitive observations:

- include the date of the observation;
- identify the confidence level;
- avoid unsupported numerical market claims;
- avoid claiming proprietary knowledge;
- do not confuse popularity with technical superiority.

The purpose is strategic decision support, not marketing copy.

---

## 8. Known-issues resolution quality

Every documented known issue must be actionable.

A strong resolution path should contain:

`Observed symptom → evidence → root cause hypothesis → affected boundary → implementation target → tests → acceptance condition`

Prefer existing AWH issue-resolving prompts when available.

Do not create contradictory recommendations that bypass existing security, policy, capability, editing, snapshot, provenance, audit, MCP, or CLI architecture.

---

## 9. Architecture guardrails

The report must preserve these project boundaries:

- AWH is not a generic autonomous-agent framework.
- AWH is not a generic workflow/DAG platform.
- AWH is not a generic model router.
- MCP-first does not mean MCP-only.
- CLI, MCP, TUI, and Control API should remain interfaces over shared services rather than independent implementations of the same semantics.
- Security-sensitive operations must remain policy/capability/session aware.
- Snapshots are not a replacement for sandboxing.
- Composable adapters are preferred over hard-coded integrations.
- Complete workflow validation is more important than feature-count inflation.
- Distribution and operational usability are product features, but must not be confused with core correctness.

Do not recommend architectural changes that violate these constraints without explicitly identifying the architectural trade-off and obtaining a separate implementation decision.

---

## 10. Documentation quality requirements

The report must be:

- concise enough to remain maintainable;
- detailed enough to support engineering decisions;
- internally consistent;
- linkable through repository-relative paths;
- explicit about evidence;
- explicit about uncertainty;
- free of stale copied claims that are no longer true.

Use tables where they improve scanability, but do not create giant unreadable tables merely to maximize detail.

Prefer exact paths, issue numbers, PR numbers, commit identifiers, and test names when they materially improve traceability.

---

## 11. Change-scope rules

This issue is documentation-only.

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

If the target report cannot be completed accurately without another file change, STOP and report the exact blocker.

---

## 12. Validation of the finished report

After creating/updating the report:

1. Re-read the complete document.
2. Check every implementation claim against repository evidence.
3. Check every issue/PR reference for correctness.
4. Check every roadmap status for consistency.
5. Check historical versus current CI claims.
6. Check that strategic speculation is explicitly labelled.
7. Check that no unsupported completion percentage or feature claim remains.
8. Check Markdown formatting and links.
9. Confirm only the intended documentation file changed.
10. Run appropriate documentation/static checks if available without changing unrelated files.
11. Do not claim full Rust CI was run unless it actually was run and passed.

---

## 13. Acceptance criteria

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
- [ ] No source-code behavior was changed.
- [ ] No unsupported implementation claim remains.
- [ ] The document is internally consistent and maintainable.
- [ ] The final diff contains only the intended documentation file.

---

## 14. Definition of Done

The issue is DONE only if the final report is a reliable engineering/strategy reference that a new contributor can use to understand:

1. what AWH actually implements today;
2. what is partially implemented;
3. what is actively being built;
4. what is merely planned;
5. what is blocked or known-broken;
6. what the highest-priority technical work is;
7. what strategic direction is supported by the available evidence.

The document must be more trustworthy than a simple copy of issue/PR descriptions because its status claims are independently checked against the repository and CI evidence.

---

## 15. Explicit non-goals

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

## 16. Final implementation report

When the issue is complete, report:

- target file changed;
- Issue #40 subject addressed;
- evidence sources inspected;
- major report sections added/updated;
- validation performed;
- any unresolved evidence gaps;
- exact final diff scope.

State explicitly whether any non-target file changed.

---

## HARD STOP

After completing AWE-019:

1. Do not begin AWE-020 or any later issue.
2. Do not modify unrelated repository files.
3. Do not convert strategic recommendations into implementation work.
4. Do not claim features are implemented without current evidence.
5. Do not silently change the AWH architecture.
6. Stop after the AWE-019 deliverable and report the final verification state.
