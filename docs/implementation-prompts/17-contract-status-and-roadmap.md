# Prompt 17 — Agent-Grade Editing Contract + Implementation Status & Roadmap (AWE-018 / AWE-019)

## 1. Mission

Produce one authoritative, implementation-backed documentation artifact for the AWH agent-grade editing contract, current implementation status, known gaps, and forward roadmap.

This is a documentation/status task, not a production-feature implementation task.

The executor must reconstruct current truth from the current rust branch by inspecting production source, tests, CI/workflows, executable interfaces, persisted-state behavior where relevant, and repository documentation. Historical reports, issue prompts, PR descriptions, and roadmap aspirations are evidence only; they must never be promoted to current capability without current verification.

The final document must answer:
1. What is the canonical AWH editing contract?
2. Which parts are implemented on current rust?
3. Which parts are implemented but insufficiently verified?
4. Which parts are partial, scaffolded, disconnected, planned, or absent?
5. What concrete evidence supports every status claim?
6. What security and architecture gaps remain?
7. Which roadmap statements are target intent versus delivered behavior?
8. What is the dependency-ordered next work?
9. Which claims remain unverified?

## 2. Product boundary

AWH is an agent-agnostic, local-first workspace runtime for coding agents.

AWH owns, where implemented:
- workspace and filesystem state;
- controlled agent-grade file editing;
- Git and worktree state;
- capabilities and policy enforcement;
- snapshots, provenance, rollback, and recovery;
- context, memory, skills, agent/session/task runtime state;
- audit and observability;
- MCP, CLI, TUI, and Control API interfaces.

External agents own:
- reasoning;
- planning;
- model selection;
- agent intelligence;
- agent-specific orchestration.

Do not describe AWH as an agent reasoning engine, model router, swarm scheduler, or general-purpose workflow engine merely because the roadmap mentions integrations around those systems.

## 3. Scope

AWE-018 owns documentation of:
- canonical editing lifecycle;
- edit vocabulary and semantics;
- identity and authorization boundary;
- expected-state/conflict behavior;
- filesystem/path/TOCTOU boundary;
- snapshots/provenance;
- rollback/recovery;
- audit;
- MCP/CLI/API/TUI convergence;
- externally observable editing behavior.

AWE-019 owns:
- verified current status;
- evidence hierarchy;
- known security and architecture gaps;
- historical-status reconciliation;
- roadmap authority;
- dependency ordering;
- phase/acceptance readiness;
- explicit unverified claims.

The final artifact remains:
docs/implementation-prompts/17-contract-status-and-roadmap.md

## 4. Strict non-goals

Do not use this prompt to:
- implement or refactor editing code;
- implement a second edit engine;
- implement a second filesystem service;
- implement authorization, policy, capabilities, snapshots, rollback, provenance, or audit;
- redesign MCP routing, CLI, TUI, or Control API;
- implement worktree lifecycle or filesystem coordination;
- build a model router, swarm scheduler, workflow engine, database, ORM, or event-sourcing layer;
- modify historical prompt collections;
- modify another implementation prompt;
- create an issue chain merely to make the roadmap look complete;
- label planned commands as implemented because they appear in documentation or help text.

If source changes are genuinely required to establish a documented fact, report the implementation/documentation inconsistency instead of expanding this task into production work.

## 5. Source-of-truth hierarchy

When sources disagree, use this order:

1. current executable/source behavior on rust;
2. current automated tests that exercise that behavior;
3. current CI/workflow evidence and recorded verification;
4. current interface documentation that matches executable behavior;
5. current roadmap/feature documents as target intent;
6. historical status reports;
7. issue/PR descriptions and external-agent claims;
8. comments or aspirational examples without executable evidence.

A roadmap item is not completion evidence.

A command listed in help is not completion evidence.

A data type or enum is not evidence that its complete runtime path exists.

A merged PR is not, by itself, evidence that its claimed behavior remains on current rust.

Historical percentages must never override current source evidence.

## 6. Required repository forensics

Read at minimum:

Roadmap/product:
- docs/roadmap/GROWTH_STRATEGY.md
- docs/roadmap/PROJECT_ROADMAP.md
- docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
- docs/FEATURES.md
- docs/PROJECT_CONTEXT.md
- docs/architecture.md
- docs/security.md
- docs/threat-model.md
- docs/CLI.md when present
- docs/mcp.md when present
- docs/testing.md when present

Implementation ownership:
- docs/implementation-prompts/01 through 16;
- Prompt 17 before replacing it;
- any later prompt that exists on current rust.

Historical evidence:
- docs/trust-wedge/
- docs/issue-resolving-prompts/
- docs/archive/
- AWH-FORENSIC-REPOSITORY-REPORT.md
- AWH_FORENSIC_REPORT.md
- relevant architecture/completion reports.

Search for:
AWE-001 through AWE-019, TW-001 through TW-008, editing, EditService, EditTransaction, ExpectedState, rollback, snapshot, provenance, audit, authorization, capability, policy, MCP, CLI, TOCTOU, service/store convergence, status, roadmap, known issues.

Historical material is requirements/evidence input only. Current source wins.

## 7. Required current-source inspection

Inspect, where present:

- src/lib.rs
- src/main.rs
- src/services/mod.rs
- src/services/files.rs
- src/services/edit.rs
- src/services/authorization.rs
- src/services/snapshot.rs
- src/services/provenance.rs
- src/services/audit.rs
- src/services/git.rs
- src/core/workspace.rs
- src/core/agents.rs
- src/core/capability_grants.rs
- src/core/policy.rs
- src/core/project.rs
- src/core/context.rs
- src/core/memory.rs
- src/core/tasks.rs
- src/mcp/mod.rs
- src/mcp/dispatcher.rs
- src/mcp/workspace.rs
- src/mcp/store_lock.rs
- src/api/control.rs
- src/tui/*
- actual CLI command/handler modules.

Inspect:
- tests/*
- source unit tests
- examples/*
- .github/workflows/*
- Cargo.toml
- Cargo.lock

Search definitions and call sites for:
EditService, EditTransaction, EditOperation, EditId, EditIdentity, EditRefs, ExpectedState, FileState, EditStatus, EditError, PatchResult, PatchFailure, SnapshotStore, SnapshotId, SnapshotEntryId, ProvenanceRecord, AuditLog, StoreLock, Authorization, Capability, Policy, workspace root, worktree, write_atomic, sha256, canonicalize, symlink, rename, remove_file, std::fs::write, tokio::fs, McpDispatcher, tools/list, tools/call, awh fs, replace, insert, delete-range, patch, apply-diff, verify, history, rollback.

For every important contract identify both its definition and its production call path.

## 8. Before-state evidence map

Build an internal table:

| Contract | Owner | Production path | Tests | Interfaces | Persistence | Status | Evidence |
|---|---|---|---|---|---|---|---|
| Workspace identity | ... | ... | ... | ... | ... | ... | ... |
| Caller/agent identity | ... | ... | ... | ... | ... | ... | ... |
| Authorization | ... | ... | ... | ... | ... | ... | ... |
| Capability/policy | ... | ... | ... | ... | ... | ... | ... |
| Edit transaction | ... | ... | ... | ... | ... | ... | ... |
| Edit execution | ... | ... | ... | ... | ... | ... | ... |
| ExpectedState/conflict | ... | ... | ... | ... | ... | ... | ... |
| Filesystem coordination | ... | ... | ... | ... | ... | ... | ... |
| Snapshot | ... | ... | ... | ... | ... | ... | ... |
| Provenance | ... | ... | ... | ... | ... | ... | ... |
| Rollback | ... | ... | ... | ... | ... | ... | ... |
| Audit | ... | ... | ... | ... | ... | ... | ... |
| MCP editing | ... | ... | ... | ... | ... | ... | ... |
| CLI editing | ... | ... | ... | ... | ... | ... | ... |
| API/TUI editing | ... | ... | ... | ... | ... | ... | ... |
| Service/store convergence | ... | ... | ... | ... | ... | ... | ... |

Do not publish invented percentages.

## 9. Status taxonomy

Use these categories exactly where applicable:

IMPLEMENTED_AND_VERIFIED:
Production behavior exists, is connected, and meaningful current evidence verifies it.

IMPLEMENTED_BUT_UNVERIFIED:
Production implementation exists and is connected, but required real-interface, restart, failure, security, concurrency, or other evidence is missing.

PARTIAL:
A material portion exists but required contract remains absent or disconnected.

SCAFFOLDED:
Types, models, placeholders, configuration, or partial service boundaries exist without the complete runtime behavior.

DECLARED_BUT_DISCONNECTED:
Documentation/help/registry advertises behavior but the production service path is absent or disconnected.

PLANNED:
Target-roadmap behavior without current implementation evidence.

UNVERIFIED:
Evidence is insufficient to determine implementation truth.

NOT_IMPLEMENTED:
Current forensics clearly establishes absence.

ENVIRONMENT_LIMITATION:
Verification could not run because of a concrete environment constraint.

TEST_HARNESS_DEFECT:
The claimed verification is invalid because the test does not exercise production behavior or has a test-side defect.

Do not substitute “complete”, “production-ready”, “100%”, or a numeric maturity score for this taxonomy.

## 10. Evidence requirements

Every significant status claim must have concrete evidence:
- source path and symbol;
- test path and test name;
- CI workflow/job evidence;
- real executable/interop evidence;
- persisted-state inspection;
- explicit environment limitation;
- historical evidence clearly labeled historical.

Insufficient evidence includes:
- roadmap text;
- help output alone;
- a struct alone;
- old PR description;
- isolated helper test;
- historical percentage.

For each major contract ask:
- Does the real production caller reach the implementation?
- Does authorization run on the real path?
- Does state actually persist when persistence is claimed?
- Does the test exercise the same production path?
- Does restart preserve the claimed state?
- Can unauthorized input cause a side effect?

## 11. Canonical agent-grade editing contract

Document the current architecture using this conceptual lifecycle, but classify each stage according to evidence:

External agent
→ MCP / CLI / TUI / Control API
→ caller + workspace/session context
→ canonical authorization/policy boundary
→ canonical path/workspace/worktree validation
→ filesystem coordination
→ EditTransaction / canonical edit service
→ ExpectedState/live-state validation
→ preparation
→ required recovery capture
→ atomic mutation
→ post-commit verification
→ provenance/audit
→ stable interface result

Do not present the conceptual lifecycle as proof that every stage exists.

Separate:
- domain semantics;
- transport/session identity;
- authorization;
- filesystem safety;
- mutation;
- recovery;
- audit/observability;
- presentation.

## 12. Canonical edit vocabulary

Where present, document the real role of:
- EditId;
- EditOperation;
- EditTransaction;
- EditIdentity;
- EditRefs;
- ExpectedState;
- FileState;
- EditStatus;
- EditError;
- patch/diff representations;
- snapshot IDs;
- provenance IDs;
- rollback IDs.

If a type is only scaffolded, classify it.

If competing definitions exist, record the architecture discrepancy rather than silently selecting one.

## 13. Editing operation contract

Document actual semantics for:
- Replace;
- Insert;
- DeleteRange;
- Patch;
- ApplyDiff;
- single-file and multi-file operations;
- creation/deletion;
- empty and zero-byte files;
- missing files;
- no-final-newline;
- LF/CRLF/mixed newline;
- Unicode, Devanagari, emoji;
- binary-like bytes where supported.

For every operation record:
1. owner;
2. input contract;
3. ExpectedState behavior;
4. authorization;
5. mutation behavior;
6. verification;
7. recovery;
8. interfaces;
9. tests;
10. limitations.

Do not infer support from enum variants alone.

## 14. ExpectedState and conflict

Document actual behavior for:
- hash/SHA-256;
- byte size;
- line count;
- context matching;
- missing-vs-existing state;
- malformed state;
- stale state;
- concurrent external modification;
- multi-operation validation.

Explicitly answer:
- Can stale state overwrite newer content?
- Where is the check performed?
- Is it revalidated immediately before mutation?
- Which interfaces use the same check?
- What evidence proves the behavior?

Unknown answers are UNVERIFIED.

## 15. Authorization and zero-side-effect denial

Trace:
- caller identity;
- agent identity;
- session identity;
- workspace binding;
- capability grants;
- policy rules;
- MCP authentication/trust;
- CLI authorization context;
- API authorization;
- inactive/unknown agents;
- malformed authorization state.

For every mutation determine whether authorization precedes consequential filesystem mutation.

For denial, seek evidence for:
denied request → no edit → no unauthorized recovery mutation → no cross-workspace side effect.

Do not infer zero-side-effect denial from an error response alone.

## 16. Filesystem and TOCTOU contract

Document actual:
- absolute/traversal rejection;
- repeated/dot path handling;
- symlink parent/final-component handling;
- workspace/worktree containment;
- canonicalization limitations;
- per-resource coordination;
- multi-file lock ordering;
- final-state revalidation;
- atomic writes;
- create/replace/delete races;
- external modification behavior.

Never say “TOCTOU-free”.

Use precise evidence-backed wording such as “race mitigated by X”, “final state revalidated by Y”, or “unverified under Z”.

Prompt 14 owns the implementation boundary; Prompt 17 only documents its current state.

## 17. Snapshot and provenance contract

Document only current implementation.

Distinguish:
- file-edit recovery snapshots;
- context-engine snapshots;
- provenance;
- generic backups;
- Git history.

Where file-edit snapshots are claimed, verify:
- exact pre-edit bytes;
- missing versus zero-byte;
- length/hash integrity;
- durable persistence;
- corruption behavior;
- edit correlation;
- agent/session/workspace correlation;
- recovery reads.

Provenance explains relationships; it does not grant authorization.

## 18. Rollback contract

Document actual rollback/recovery behavior.

Where implemented, inspect:
EditId → authorization → canonical snapshot/provenance → produced-state verification → exact restoration → post-rollback verification → audit/provenance.

Explicitly classify:
- success;
- unknown ID;
- missing/corrupt snapshot;
- unauthorized rollback;
- wrong workspace;
- produced-state mismatch;
- concurrent modification;
- repeated rollback;
- restart behavior.

Do not equate Git reset/revert with edit rollback unless current product semantics explicitly do so.

## 19. Audit contract

Determine:
- event model;
- writer/store;
- durability;
- ordering;
- restart persistence;
- corruption behavior;
- redaction;
- workspace isolation;
- edit/rollback/authorization correlation;
- MCP/CLI/API/TUI integration.

Do not infer durable history from an in-memory ring buffer.

Do not infer provenance from audit records.

Do not infer completeness from logging statements.

## 20. MCP contract

Verify:
- initialization;
- remote authentication;
- session state;
- tools/list;
- argument validation;
- edit tool exposure;
- authorization;
- filesystem mutation path;
- ExpectedState;
- errors;
- audit;
- real-client evidence;
- cross-session isolation.

The route namespace is not authorization.

If MCP is a thin adapter, document that. If it contains duplicate business logic, record it as an architecture gap.

## 21. CLI contract

Treat docs/CLI.md as target contract, not proof.

Classify separately:
- awh fs replace;
- awh fs insert;
- awh fs delete-range;
- awh fs patch;
- awh fs apply-diff;
- awh fs verify;
- awh fs history;
- awh fs rollback.

For each determine:
- parser;
- handler;
- canonical service call;
- authorization;
- real mutation/recovery;
- tests;
- real-binary evidence;
- persistence.

Clearly separate documented target commands from executable verified commands.

## 22. Control API and TUI

Inspect current paths for:
- canonical service use;
- canonical store use;
- edit/recovery use;
- interface-local state;
- authorization bypass;
- duplicate filesystem mutation;
- history/rollback source.

Presentation differences are acceptable. Duplicate business/security semantics are an architecture gap unless intentional and documented.

## 23. Service/store convergence

Prompt 15 owns convergence implementation. Prompt 17 documents current convergence.

For each domain identify:
- canonical owner;
- persistence owner;
- writers;
- readers;
- interface adapters;
- compatibility/migration paths;
- unresolved duplicates.

Cover at least:
FilesService, EditService, MemoryStore, TaskStore, project/workspace state, AgentStore, CapabilityGrantStore, PolicyStore, SnapshotStore, provenance, audit, and MCP protocol/session state.

Classify remaining implementations as canonical, adapter, protocol state, compatibility, test-only, legacy/dead, or unresolved duplicate.

## 24. Interface parity

Include an evidence-backed matrix:

| Operation | MCP | CLI | TUI | Control API | Canonical owner | Parity |
|---|---|---|---|---|---|---|
| read | ... | ... | ... | ... | ... | ... |
| replace | ... | ... | ... | ... | ... | ... |
| insert | ... | ... | ... | ... | ... | ... |
| delete | ... | ... | ... | ... | ... | ... |
| patch | ... | ... | ... | ... | ... | ... |
| apply-diff | ... | ... | ... | ... | ... | ... |
| verify | ... | ... | ... | ... | ... | ... |
| history | ... | ... | ... | ... | ... | ... |
| rollback | ... | ... | ... | ... | ... | ... |

Use precise labels such as adapter, canonical service, duplicate path, not exposed, or unverified.

## 25. Security and architecture gap register

Produce:

| Gap | Evidence | Impact | Current status | Owner | Next action |
|---|---|---|---|---|---|
| ... | ... | ... | ... | ... | ... |

Investigate at least:
- capability model versus enforcement;
- custom/dynamic MCP divergence;
- policy scope/effect limitations;
- unsandboxed executable skills;
- editing path duplication;
- MCP-local filesystem mutation;
- CLI-local mutation;
- snapshot durability;
- rollback conflict protection;
- audit persistence;
- agent/session binding;
- worktree isolation;
- TOCTOU coordination;
- duplicate IDs/stores;
- distribution/release verification;
- test terminology versus actual technique.

Only retain gaps supported by current evidence. If a historical gap is closed, record it as closed with current evidence.

## 26. Roadmap authority

Explicitly distinguish:
- docs/roadmap/PROJECT_ROADMAP.md = forward-looking implementation plan;
- docs/roadmap/GROWTH_STRATEGY.md = adoption sequencing;
- docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md = agent-profile/routing addendum;
- implementation prompts = implementation contracts;
- historical reports = snapshots, not current authority.

Do not combine these into one completion percentage.

When phases conflict, state the exact contradiction and which document governs which question.

## 27. Dependency-ordered roadmap

For every proposed next step identify:
- prerequisite;
- owner;
- evidence prerequisite is present/absent;
- implementation boundary;
- acceptance evidence;
- blocking gap.

Do not create hidden dependencies on future prompt merges.

Use current source contracts rather than prompt numbering as dependencies.

Where supported by current evidence, consider the general order:
foundation → workspace/path safety → canonical editing → authorization/capability/policy → filesystem coordination → snapshot/provenance → rollback/recovery → persistent audit → interface convergence → real-interface acceptance → agent/session isolation → worktree isolation → higher-risk runtime → control-plane/ecosystem.

This is a dependency model, not a claim of current completion.

## 28. Runtime status versus evidence status

Maintain two dimensions:

Runtime status:
What current source actually does.

Evidence status:
How strongly that behavior is verified.

Example:
| Runtime | Evidence | Meaning |
|---|---|---|
| implemented | verified | strongest claim |
| implemented | unverified | source connected, required evidence missing |
| partial | verified | some contract proven |
| scaffolded | verified | only foundation proven |
| planned | none | target intent |

Never collapse these dimensions.

## 29. Current-state contract matrix

Include:

| Contract | Intended owner | Current implementation | Evidence | Runtime status | Evidence status | Limitation |
|---|---|---|---|---|---|---|
| workspace resolution | ... | ... | ... | ... | ... | ... |
| identity/session | ... | ... | ... | ... | ... | ... |
| authorization | ... | ... | ... | ... | ... | ... |
| edit model | ... | ... | ... | ... | ... | ... |
| edit execution | ... | ... | ... | ... | ... | ... |
| conflict detection | ... | ... | ... | ... | ... | ... |
| coordination | ... | ... | ... | ... | ... | ... |
| snapshot | ... | ... | ... | ... | ... | ... |
| provenance | ... | ... | ... | ... | ... | ... |
| rollback | ... | ... | ... | ... | ... | ... |
| audit | ... | ... | ... | ... | ... | ... |
| MCP exposure | ... | ... | ... | ... | ... | ... |
| CLI exposure | ... | ... | ... | ... | ... | ... |
| API/TUI exposure | ... | ... | ... | ... | ... | ... |

## 30. Known-issue discipline

Every issue must include:
1. precise statement;
2. evidence;
3. affected contract;
4. technical impact;
5. current status;
6. owning subsystem;
7. next verification/implementation step;
8. whether it blocks a claimed capability.

Avoid vague statements such as “security needs work”.

## 31. Security claim discipline

For every security claim answer:
- attacker/input;
- rejecting boundary;
- before/after mutation;
- test evidence;
- real-interface evidence;
- persistence/restart evidence if relevant;
- platform limitation;
- remaining uncertainty.

Never write “fully secure”, “TOCTOU-proof”, “sandboxed everywhere”, “all edits authorized”, or “all audit events durable” without exact evidence and scope.

## 32. Test/evidence inventory

Classify evidence as:
- unit;
- service integration;
- binary/CLI;
- MCP protocol/interop;
- Control API;
- TUI/backend;
- persistence/restart;
- concurrency/race;
- security/adversarial;
- failure injection.

Do not count tests mechanically.

Do not call source-text scans mutation tests.

Do not call an in-process mock real-client validation.

## 33. CI evidence

Inspect current workflows and record:
- format;
- build;
- unit/integration;
- clippy;
- audit/security;
- OS matrix;
- MCP interop;
- release/package verification.

For every claimed CI result, identify what the workflow actually executes.

If live CI status is unavailable, label workflow configuration as configuration evidence only.

## 34. Real-interface evidence

Prefer real evidence:

CLI:
compiled binary, isolated workspace, actual command, exit code, stdout/stderr, filesystem state.

MCP:
real JSON-RPC/transport, initialize, tools/list, tools/call, authorization/session behavior, and real-client interoperability where available.

Control API:
actual HTTP request through production router/middleware.

TUI:
actual backend/service path where full terminal automation is impractical.

If only unit evidence exists, say so.

## 35. Persistence/restart evidence

For every durable-state claim investigate:
write → process termination → new process → read/recover.

Cover where relevant:
workspace manifest, agent/capability/policy state, edit metadata, snapshot, provenance, rollback, audit, migrations.

If restart is not tested, do not call the durable behavior fully verified.

## 36. Failure semantics

Document current behavior for:
- invalid path;
- authorization denial;
- stale ExpectedState;
- preparation failure;
- snapshot failure;
- atomic write failure;
- verification failure;
- provenance failure;
- audit failure;
- rollback conflict;
- corrupted persisted state;
- restart after interruption.

Separate:
- filesystem truth;
- recovery truth;
- audit truth;
- interface-reported truth.

An observer failure must not be presented as if it changed filesystem reality.

## 37. Historical reconciliation

When historical and current evidence disagree:
1. identify the historical claim;
2. inspect current source;
3. inspect current tests;
4. inspect current CI;
5. state current result;
6. explain the change when evidence permits;
7. never preserve an obsolete percentage for continuity.

The final artifact must not become a contradictory collection of snapshots.

## 38. Percentages and scoring

Do not produce:
- overall completion percentages;
- security scores;
- maturity scores;
- best/worst rankings;
- unsupported numeric ratings.

Historical percentages may be mentioned only as attributed historical estimates and must remain separate from current status.

## 39. Strategic analysis boundary

AWE-019 strategic analysis must remain technical and evidence-based.

Allowed:
- dependency analysis;
- architecture trade-offs;
- implementation sequencing;
- scope reduction based on evidence;
- duplicate-system identification;
- unsupported-roadmap identification;
- release-readiness gaps;
- verification gaps.

Do not turn the document into marketing copy or speculative competitive analysis.

## 40. Required roadmap matrix

Include:

| Work item | Current evidence | Runtime status | Prerequisites | Owner | Acceptance evidence | Blocking gap |
|---|---|---|---|---|---|---|
| canonical editing | ... | ... | ... | ... | ... | ... |
| authorization | ... | ... | ... | ... | ... | ... |
| filesystem coordination | ... | ... | ... | ... | ... | ... |
| snapshot/provenance | ... | ... | ... | ... | ... | ... |
| rollback | ... | ... | ... | ... | ... | ... |
| persistent audit | ... | ... | ... | ... | ... | ... |
| service/store convergence | ... | ... | ... | ... | ... | ... |
| agent/session isolation | ... | ... | ... | ... | ... | ... |
| worktree isolation | ... | ... | ... | ... | ... | ... |
| real-interface acceptance | ... | ... | ... | ... | ... | ... |

Add work items only when current evidence justifies them.

## 41. Phase-exit interpretation

Use the roadmap phase-exit principle:
implementation → unit tests → integration tests → real terminal/interface validation → failure/recovery validation → documentation update.

MCP-facing capabilities should include real-client validation where practical.

A feature is not done merely because source exists.

The status artifact must identify missing exit evidence.

## 42. Required final document structure

The finished artifact should contain:
1. Purpose and scope
2. Product boundary
3. Evidence hierarchy
4. Current branch/verification context
5. Canonical editing contract
6. Current-state contract matrix
7. Interface parity
8. Security and safety contract
9. Snapshot/provenance/rollback/audit contract
10. Current implementation status
11. Evidence inventory
12. Known gaps/issues
13. Historical-status reconciliation
14. Roadmap authority and contradictions
15. Dependency-ordered roadmap
16. Phase/acceptance gates
17. Unverified claims and environment limitations
18. Final evidence summary
19. Strict scope confirmation

The executor may improve headings, but must retain all required information.

## 43. Required acceptance checklist

Before finalizing:
- current rust source inspected;
- current tests inspected;
- current CI/workflows inspected;
- current CLI inspected;
- current MCP implementation inspected;
- API/TUI paths inspected where relevant;
- persistence boundaries inspected;
- Prompts 01–16 reviewed for ownership;
- historical Trust Wedge material reviewed;
- historical issue-resolving material reviewed;
- historical status reconciled;
- editing contract documented;
- authorization documented;
- ExpectedState/conflict classified;
- filesystem/TOCTOU classified;
- snapshot/provenance classified;
- rollback classified;
- audit durability classified;
- MCP editing classified;
- CLI editing classified;
- interface parity assessed;
- service/store convergence assessed;
- security gaps evidence-backed;
- roadmap contradictions reconciled;
- roadmap dependencies explicit;
- no unsupported completion score introduced;
- every major claim has evidence;
- unverified claims are explicit;
- no production source changed;
- no other implementation prompt changed.

## 44. Linear execution sequence

Execute exactly in this order:

### Step 1 — Establish current branch
Confirm the task starts from current rust and record the base ref.

### Step 2 — Read existing Prompt 17
Inventory all existing requirements before replacing it.

### Step 3 — Read product and roadmap documents
Read all required roadmap, feature, architecture, security, CLI, MCP, and testing material.

### Step 4 — Read implementation ownership prompts
Read Prompts 01–16 and identify boundaries without assuming implementation.

### Step 5 — Read historical evidence
Inspect Trust Wedge, issue-resolving, archive, forensic, and status material.

### Step 6 — Inspect production source
Trace definitions and real call paths for editing, authorization, filesystem, snapshot, rollback, provenance, audit, MCP, CLI, API, TUI, identity, policy, and worktree boundaries.

### Step 7 — Inspect tests and CI
Map tests to production paths and determine which claims are actually verified.

### Step 8 — Build evidence map
Apply the status taxonomy.

### Step 9 — Reconstruct editing contract
Document target contract and current reality separately.

### Step 10 — Build status/parity/gap matrices
Populate them from evidence.

### Step 11 — Reconcile historical claims
Separate historical estimates from current evidence.

### Step 12 — Reconcile roadmap documents
State authority and contradictions explicitly.

### Step 13 — Build dependency-ordered roadmap
Use current source contracts, not prompt numbering, as dependencies.

### Step 14 — Write Prompt 17
Keep it standalone, linear, evidence-driven, and implementation-backed.

### Step 15 — Self-audit
Search the finished artifact for:
implemented, complete, verified, 100%, percentage, secure, TOCTOU-free, roadmap, planned, historical, unverified, AWE-018, AWE-019.

Every completion/security claim must have an evidence requirement.

### Step 16 — Scope audit
Confirm only Prompt 17 changed.

### Step 17 — Final verification
At minimum run:
git diff --check

If source/tests are unchanged, do not claim cargo verification was required or performed unless it was actually run.

### Step 18 — Final report
Report:
1. documents inspected;
2. source modules inspected;
3. tests/CI inspected;
4. evidence hierarchy;
5. current editing contract;
6. status classifications;
7. security/architecture gaps;
8. historical discrepancies;
9. roadmap contradictions;
10. dependency-ordered next work;
11. unverified claims;
12. environment limitations;
13. exact changed file;
14. scope confirmation.

## 45. Completion criteria

Prompt 17 is complete only when:
- the artifact is standalone;
- current rust is the implementation authority;
- roadmap intent is separated from runtime reality;
- historical claims are explicitly historical;
- canonical editing lifecycle is documented;
- every major editing stage has a status classification;
- ExpectedState/conflict behavior is documented;
- authorization behavior is documented;
- filesystem/path/TOCTOU behavior is documented;
- snapshot/provenance behavior is documented;
- rollback behavior is documented;
- audit durability/correlation are documented;
- MCP/CLI/API/TUI behavior is classified;
- service/store convergence is assessed;
- security/architecture gaps are evidence-backed;
- roadmap contradictions are reconciled;
- roadmap dependencies are explicit;
- no unsupported completion score is introduced;
- unverified claims are visible;
- evidence locations are recorded;
- no production source is modified;
- no other implementation prompt is modified.

## 46. Strict file scope

For implementation of this prompt, modify only:

docs/implementation-prompts/17-contract-status-and-roadmap.md

Do not modify:
- any other docs/implementation-prompts file;
- docs/trust-wedge;
- docs/issue-resolving-prompts;
- roadmap documents;
- source files;
- tests;
- workflows;
- README;
- CLI/MCP documentation;
- archive documents.

## 47. Final invariant

The finished Prompt 17 must make it possible to distinguish:

TARGET CONTRACT
≠ CURRENT IMPLEMENTATION
≠ VERIFIED BEHAVIOR
≠ HISTORICAL CLAIM
≠ FUTURE ROADMAP

The central product question is:

Can an authorized external coding agent use AWH's supported editing path through its real interfaces, with the documented identity/authorization, filesystem-safety, conflict, recovery, provenance, audit, and isolation guarantees actually present on current rust?

If evidence proves only part of the contract, document exactly that part.

If evidence shows failure, document the failure.

If evidence is incomplete, document the uncertainty.

Never convert uncertainty into completion.
