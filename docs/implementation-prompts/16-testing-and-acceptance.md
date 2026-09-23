# Prompt 16 — Agent-Grade Editing Test + Trust-Wedge Acceptance (AWE-015 / AWE-017 / TW-008)

## 1. Mission
Build executable, reproducible acceptance evidence for the current AWH editing and Trust-Wedge boundary.
This is a testing, verification, failure-injection, and acceptance contract. It must not manufacture a green result by adding test-only bypasses or by implementing unrelated product features solely to satisfy assertions.
Current rust source and current tests are authoritative. Historical material is requirements evidence only.

Evidence chain:
caller → agent/session/workspace context → authorization/capability/policy → canonical service → filesystem coordination → edit transaction → snapshot/provenance/rollback → verification → persistent audit.

Where a production boundary is absent, the test suite must prove or report that absence. A fixture or mock must never be presented as proof that the missing production feature exists.

## 2. Product boundary
AWH is an agent-agnostic, local-first workspace runtime for coding agents.
AWH owns workspace/filesystem state, controlled editing, Git/worktrees, capabilities/policy, snapshots/provenance/rollback, context, memory, skills, agent/session state, tasks, audit/observability, and MCP/CLI/TUI/Control API interfaces.
External agents own reasoning, planning, model selection, and agent intelligence.

## 3. Prompt ownership
This prompt owns:
- executable acceptance evidence;
- integration and end-to-end test architecture;
- agent-grade editing acceptance;
- Trust-Wedge acceptance;
- security regression tests;
- concurrency and failure-injection tests;
- real MCP/CLI/API validation;
- cross-interface parity evidence;
- restart and persistence evidence;
- exact-byte filesystem correctness evidence;
- honest pass/fail/unproven reporting;
- regression guardrails for established security properties.

Minimal test seams, fixtures, and helpers may be added when they are required for deterministic tests and do not weaken production semantics.

## 4. Explicit non-goals
Do not use this prompt to:
- redesign EditTransaction or create another edit engine;
- create another filesystem service;
- create another authorization, policy, capability, identity, snapshot, provenance, rollback, audit, or MCP system;
- redesign MCP routing, Control API, or TUI;
- implement worktree lifecycle or distributed locking;
- add containers, VMs, a database, ORM, event bus, model routing, swarm scheduling, or workflow execution;
- replace production state with mocks;
- weaken authorization to make tests pass;
- skip required tests merely because they fail;
- change CI thresholds to obtain a green result;
- add external-agent infrastructure to runtime code;
- modify another implementation prompt.

## 5. Required repository forensics
Before writing tests, inspect:
- docs/roadmap/GROWTH_STRATEGY.md
- docs/roadmap/PROJECT_ROADMAP.md
- docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
- docs/FEATURES.md
- docs/PROJECT_CONTEXT.md
- docs/architecture.md
- docs/security.md
- docs/threat-model.md
- docs/mcp.md
- docs/CLI.md when present
- docs/implementation-prompts/README.md
- implementation prompts 01–15 and Prompt 17 when present.

Inspect relevant historical material under docs/trust-wedge/, docs/issue-resolving-prompts/, docs/archive/, AWH-FORENSIC-REPOSITORY-REPORT.md, AWH_FORENSIC_REPORT.md, and roadmap/status reports.
Search historical material for AWE-001 through AWE-017, TW-001 through TW-008, authorization, capability, policy, caller identity, workspace isolation, ExpectedState, edit conflict, rollback, snapshot, provenance, audit, MCP editing, CLI editing, acceptance, failure injection, and adversarial tests.

## 6. Current-source forensics
At minimum inspect:
- src/main.rs and src/lib.rs;
- src/services/mod.rs, files.rs, edit.rs, authorization.rs, snapshot.rs, provenance.rs, audit.rs, git.rs;
- src/core/agents.rs, capability_grants.rs, policy.rs, workspace.rs, project.rs, context.rs, memory.rs, tasks.rs;
- src/context/*;
- src/mcp/mod.rs, dispatcher.rs, workspace.rs, context.rs, context_engine.rs, auth.rs, http.rs, sse.rs, server.rs;
- src/tui/backend.rs;
- src/api/control.rs;
- tests/* and examples/mcp-interop/*;
- Cargo.toml and relevant CI workflows.

Search definitions and call sites for EditTransaction, EditOperation, ExpectedState, EditService, FilesService, write_atomic, SnapshotStore, ProvenanceRecord, rollback, audit, AgentProfile, AgentSession, capability, policy, execution_gate, StoreLock, WorkspaceMcp, filesystem.* and workspace.*.

## 7. Before-state acceptance map
Create a concrete matrix before adding tests:
| Contract | Production owner | Interfaces | Status | Existing evidence | Missing evidence |
| workspace containment | ... | MCP/CLI/API/TUI | ... | ... | ... |
| caller identity | ... | MCP/CLI/API | ... | ... | ... |
| capability/policy | ... | MCP/CLI/API | ... | ... | ... |
| edit transaction | ... | ... | ... | ... | ... |
| replace/insert/delete/patch/diff | ... | ... | ... | ... | ... |
| ExpectedState conflict | ... | ... | ... | ... | ... |
| atomic mutation | ... | ... | ... | ... | ... |
| snapshot/provenance | ... | ... | ... | ... | ... |
| rollback | ... | ... | ... | ... | ... |
| audit | ... | ... | ... | ... | ... |
| MCP/CLI/API/TUI parity | ... | ... | ... | ... | ... |
| worktree/session isolation | ... | ... | ... | ... | ... |
| restart persistence | ... | ... | ... | ... | ... |

Classify evidence as implemented-and-verified, implemented-but-unverified, partial, declared-but-disconnected, not-implemented, test-harness defect, environment limitation, or security regression.

## 8. Test architecture
Use the smallest real layer that proves the contract.
- Unit tests: pure edit semantics, ExpectedState matching, path/resource-key logic, serialization, deterministic error mapping.
- Service integration: real temporary workspaces and canonical services.
- Binary/CLI integration: the actual awh binary where practical.
- MCP integration: the real dispatcher/transport and real-client interoperability where supported.
- Control API: actual HTTP routing/middleware plus canonical services.
- TUI: backend/service mapping for consequential operations.
- End-to-end: at least one real workflow across interface, identity, authorization, edit, filesystem, verification, audit, and restart where those production boundaries exist.

Mocks may prove adapter behavior. They cannot prove real filesystem containment, atomicity, authorization, persistence, rollback, transport authentication, or cross-process coordination.

## 9. AWE-015 agent-grade editing workflow
Build a real temporary workspace, create a fixture, resolve the real caller/session/workspace context, read current state, obtain ExpectedState, submit the canonical edit request, enforce authorization, mutate through the canonical service, verify post-state, inspect snapshot/provenance/audit when implemented, restart, and read back the result.

Cover every implemented edit operation: Replace, Insert, DeleteRange, Patch/multi-operation, and ApplyDiff.
Cover single-file and multi-file edits, creation where allowed, empty files, zero-byte files, no-final-newline, LF, CRLF, mixed newline, Unicode, Devanagari, emoji, allowed large files, and oversized input rejection.
Never duplicate the edit algorithm in test code.

## 10. ExpectedState and conflict acceptance
For each supported ExpectedState path:
1. create fixture;
2. capture expected state;
3. modify it externally;
4. submit the stale mutation;
5. assert deterministic conflict;
6. assert no stale overwrite;
7. assert current external bytes remain intact;
8. assert audit/error evidence where the production contract requires it.

Test wrong hash, wrong byte size, wrong line count, wrong context, malformed state, missing-vs-existing mismatches, and concurrent modification between validation and commit.

## 11. Authorization acceptance
Prove authorization precedes consequential mutation.
Test missing identity, unknown/inactive agent, wrong session, wrong workspace, denied capability, denied policy, malformed security state, invalid transport credentials, and valid authorization.
For each denial capture filesystem existence, bytes, size, directory entries, relevant recovery artifacts, and audit outcome.
A denial response is insufficient evidence if a protected file changed.

## 12. Path and workspace security
Test absolute paths, traversal, repeated separators, dot segments, symlink-to-outside, symlinked parents, final-component replacement races, sibling workspaces, worktree roots, missing parents, and rename/recreate races where the platform supports the scenario.
Use the production path-validation and coordination boundary. Do not normalize paths differently in the test.

## 13. TOCTOU and coordination acceptance
Exercise real concurrent actors.
- Same file with competing ExpectedState: no stale overwrite and no deadlock.
- Different files: independent progress where resource-level coordination promises it.
- Multi-file edit with concurrent change: required prepare-before-commit/conflict semantics.
- Symlink race: no workspace escape.
Use barriers, channels, deterministic hooks, and bounded timeouts instead of sleeps as the primary synchronization mechanism.
Do not claim that all OS-level races are universally impossible; record platform limitations.

## 14. Snapshot/provenance acceptance
When file-edit snapshots exist, prove exact pre-edit bytes, missing-versus-zero-byte distinction, newline preservation, Unicode preservation, integrity validation, durable reload, corruption failure, and safe blocking when required capture fails.
Do not confuse src/context/snapshot.rs Context Engine snapshots with file-edit recovery snapshots.
If the file snapshot boundary is absent, report it as unimplemented.

## 15. Rollback acceptance
When canonical rollback exists, test successful edit, EditId linkage, snapshot/provenance linkage, rollback authorization, produced-state validation, exact restoration, post-rollback verification, audit, repeated rollback, external modification, concurrency, restart before rollback, and restart after rollback.
Never restore over unrelated external changes merely to make a test pass.
Do not implement rollback logic inside the test.

## 16. Audit acceptance
When persistent audit exists, prove success, denial, conflict, rollback correlation, agent/session/workspace correlation, unique event identity, deterministic order, restart persistence, corruption failure, redaction of bearer/API/private-key/password secrets, absence of full file contents, and workspace-scoped queries.
Do not treat an in-memory ring buffer as durable evidence unless current source explicitly makes it authoritative.

## 17. Cross-interface parity
For each operation exposed by multiple interfaces, verify:
MCP → canonical service; CLI → canonical service; TUI → canonical service; Control API → canonical service.
Presentation differences are acceptable. Different authorization, mutation, conflict, persistence, or security semantics are not acceptable unless explicitly documented.
Test representative read, mutation, conflict, rollback, and audit/history operations.

## 18. Agent/session/workspace/worktree isolation
Use at least two isolated identities/workspaces and, when implemented, two worktrees.
Prove agent A cannot read or mutate B, session substitution is rejected, route/CLI arguments cannot change authorized workspace, one agent can stop without altering another, and audit correlation remains isolated.
If AWH-native session/worktree production boundaries are absent, report those assertions unproven rather than faking them.

## 19. MCP acceptance
Use the real MCP stack.
Test initialize, protocol negotiation, tools/list, tools/call, malformed JSON-RPC, unknown method/tool, invalid arguments, clean disconnect, authentication failures, valid authentication, session mismatch, agent-route mismatch where supported, denied invocation, and zero-side-effect denial.
For editing, distinguish legacy whole-file write from canonical edit, verification, rollback, and audit. A successful MCP connection is not evidence of agent-grade editing.

## 20. CLI acceptance
Invoke the actual awh binary in an isolated workspace.
Test valid commands, invalid arguments, workspace selection, traversal, authorization denial, ExpectedState conflict, successful edit, verify, rollback/history where available, machine-readable output where supported, stdout/stderr separation, non-zero failures, noninteractive operation, shell quoting, Unicode, and cancellation where practical.
The CLI must remain an adapter over canonical services.

## 21. Control API and TUI acceptance
Control API tests must use real HTTP routing and middleware for authentication/authorization, workspace scoping, mutation, invalid input, denial, conflict, persistence, restart, and sanitized errors.
TUI backend tests must exercise project selection, filesystem operations, context/memory/skills, Git/terminal where applicable, service reuse, audit behavior, and workspace isolation.
Presentation state must not become authoritative AWH domain state.

## 22. Exact-byte matrix
Use byte comparisons for:
- ASCII;
- Unicode;
- Devanagari;
- emoji;
- LF;
- CRLF;
- mixed newline;
- no final newline;
- empty text;
- zero-byte files;
- supported binary-like bytes;
- oversized content rejection.

## 23. Failure injection
Inject failures at validation, authorization, ExpectedState, preparation, snapshot capture, persistence, atomic commit, post-commit verification, provenance, audit, rollback, and restart boundaries where the production design exposes deterministic seams.
For every failure record whether mutation occurred, whether that was expected, whether recovery is possible, whether audit/provenance truth is correct, and whether retry is safe.
Do not modify production code to ignore errors just for testing.

## 24. Crash/restart acceptance
Simulate restart before mutation, after snapshot publication, after commit, before/after audit publication, during migration where applicable, and after rollback.
After restart verify canonical state, no fabricated success, no silent history loss, corruption failure, and specified idempotent recovery behavior.

## 25. Concurrency matrix
| Scenario | Required evidence |
| same file / same expected state | deterministic serialization or conflict; no corruption |
| stale expected state | no stale overwrite |
| different files | independent progress where promised |
| concurrent create | no duplicate identity corruption |
| edit + rollback | conflict-aware recovery |
| edit + external write | no silent overwrite |
| two MCP sessions | isolation |
| MCP + CLI | same canonical semantics |
| MCP + API | same canonical semantics |
| unrelated workspaces | no cross-talk |
| concurrent audit writers | no corruption or duplicate order |
| concurrent snapshot readers | no torn recovery data |

All concurrency tests require timeouts, cleanup, diagnostics, and bounded resources.

## 26. Security regression suite
Cover path traversal, symlink escape, wrong workspace, wrong agent/session, denied capability, denied policy, malformed security state, MCP authentication failure, session confusion, route confusion, stale ExpectedState, TOCTOU, secret leakage, audit leakage, rollback over external changes, cross-workspace audit queries, and interface bypass of canonical services.
Assert both the error and the protected side effect.

## 27. Secret-data testing
Use only synthetic sentinels such as TEST_BEARER_TOKEN_SECRET, TEST_API_KEY_SECRET, TEST_PRIVATE_KEY_SECRET, TEST_PASSWORD_SECRET, and TEST_FILE_CONTENT_SECRET.
Check stdout, stderr, audit, provenance, snapshot metadata, errors, debug formatting, and generated test artifacts.
Forbidden secret values must not appear in protected outputs. Never use real credentials.

## 28. Negative and malformed-input testing
Every meaningful success path should have invalid counterparts: invalid path, unknown identity, denied capability, denied policy, stale state, missing file, corrupt schema, invalid token, foreign workspace, snapshot failure, and rollback conflict.
Feed bounded malformed JSON-RPC, tool arguments, CLI input, ExpectedState, patch/diff, persisted store data, audit records, snapshot manifests, provenance records, and path strings.
Expected behavior is deterministic rejection or safe error, not panic, resource exhaustion, escape, or silent acceptance.

## 29. Persistence and migration
For each changed schema: create an old-format fixture, load it, migrate when required, verify semantic equivalence, atomically publish, restart, reload, repeat migration, and test malformed/truncated input.
Migration failure must never silently become an empty workspace.

## 30. Architecture-regression tests
Prevent reintroduction of known duplicate mechanisms where practical:
- MCP-local authoritative filesystem mutation;
- second edit engine;
- second snapshot/rollback/audit store;
- duplicate memory/task authority;
- direct authorization bypass;
- edit bypassing EditService;
- interface-specific authoritative persistence;
- test-only authorization bypass.
Prefer stable dependency/API boundaries over brittle filename grep, with source-level checks only as supplementary guards.

## 31. Test isolation and determinism
Every integration test uses isolated temporary state, unique directories, cleanup, no real credentials, and no production user files.
Tests must not depend on enumeration order, uncontrolled randomness, external network availability, implicit global environment, or test ordering.
Use deterministic seeds when randomness is needed and bounded diagnostics for failures.
If a global in-memory audit object exists, explicitly account for its process-wide state.

## 32. Context Engine distinction
The repository contains a Context Engine with item, budget, planner, selector, scoring, compression, offload, token, policy, and context-snapshot concepts.
Test these contracts independently where relevant.
Do not use Context Engine snapshots as proof that file-edit snapshots/rollback exist.
Keep project context persistence, runtime context assembly, Context Engine item state, and file-edit recovery as separate evidence categories.

## 33. Platform matrix
Consider Linux, macOS, Windows, and Android/Termux/proot where supported.
Record platform-specific symlink, canonicalization, atomic rename, permission, signal, locking, case-sensitivity, and newline behavior.
Do not claim identical guarantees when OS primitives differ.

## 34. Resource-limit acceptance
Exercise oversized MCP input, oversized files/patches, excessive operations, concurrent requests, excessive context items, large audit queries, large snapshot material, and repeated failed authentication where the product contract defines limits.
Verify rejection occurs before unsafe resource consumption where required.

## 35. Real-interface evidence
Do not validate only internal functions.
At least one real end-to-end path must demonstrate interface → canonical service → canonical state → restart → interface readback.
Record client, transport, command/request, exit/response, resulting state, and evidence location.

## 36. Acceptance truth table
| Case | Preconditions | Action | Expected result | Mutation allowed? | Evidence |
| authorized edit | valid identity/state | edit | success | yes | ... |
| stale state | external change | edit | conflict | no | ... |
| denied capability | denied capability | edit | denial | no | ... |
| foreign workspace | foreign resource | edit | denial | no | ... |
| traversal | invalid path | edit | rejection | no | ... |
| symlink escape | external target | edit | rejection | no | ... |
| snapshot failure | required capture fails | edit | safe failure | no | ... |
| verification failure | bad post-state | edit | recovery-safe result | contract-dependent | ... |
| rollback conflict | external post-edit change | rollback | conflict | no | ... |
| concurrent edit | competing writers | edit | serialized/conflict | contract-dependent | ... |
| restart | persisted success | restart/read | state preserved | n/a | ... |

## 37. Interface parity matrix
| Operation | MCP | CLI | TUI | Control API | Canonical owner | Same semantics? |
| read | ... | ... | ... | ... | ... | ... |
| replace | ... | ... | ... | ... | ... | ... |
| patch | ... | ... | ... | ... | ... | ... |
| conflict | ... | ... | ... | ... | ... | ... |
| rollback | ... | ... | ... | ... | ... | ... |
| audit/history | ... | ... | ... | ... | ... | ... |
Unavailable interfaces must be marked unavailable, not falsely passed.

## 38. Failure-semantics matrix
| Failure point | Mutation occurred? | Expected? | Recovery possible? | Audit/provenance truth | Test |
| authorization | ... | ... | ... | ... | ... |
| ExpectedState | ... | ... | ... | ... | ... |
| snapshot | ... | ... | ... | ... | ... |
| commit | ... | ... | ... | ... | ... |
| verification | ... | ... | ... | ... | ... | ... |
| audit | ... | ... | ... | ... | ... | ... |
| rollback | ... | ... | ... | ... | ... | ... |

## 39. Failure diagnosis
Classify every failure before changing code: production defect, test defect, fixture defect, environment defect, platform difference, contract ambiguity, or missing feature.
Never weaken an assertion solely to obtain a pass.
Reconcile contradictions against current source, current tests, roadmap, and security requirements.

## 40. CI discipline
Required acceptance tests must terminate, use bounded resources, clean temporary state, and return non-zero on failure.
Do not add ignore/skip behavior to hide a failing security or acceptance test.
Use existing platform-specific mechanisms only when the platform genuinely cannot support the contract, and document that limitation.

## 41. Required verification
Run:
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
Also run targeted editing, authorization/security, filesystem race, snapshot/rollback, audit, MCP, CLI, API, TUI, migration, concurrency, and architecture-regression tests.
Never report a command as passed unless it actually ran and passed.

## 42. Final acceptance report
Report:
1. Forensics — documents, historical collections, and source modules inspected.
2. Before-state map — production owners and status.
3. Test architecture — unit/service/binary/MCP/API/TUI/E2E.
4. AWE-015 evidence.
5. AWE-017 evidence.
6. TW-008 Trust-Wedge acceptance matrix.
7. Authorization/capability/policy evidence.
8. Filesystem/path/TOCTOU evidence.
9. Edit/conflict/multi-file evidence.
10. Snapshot/provenance evidence.
11. Rollback evidence.
12. Audit evidence.
13. Cross-interface parity.
14. Concurrency and failure-injection evidence.
15. Restart/persistence evidence.
16. Security regressions.
17. Platform/environment limitations.
18. Exact verification command results.
19. Unproven guarantees.
20. Production and test-harness defects discovered.
21. Changed files.
22. Scope confirmation.

## 43. Completion criteria
- [ ] Current source and existing tests were inspected first.
- [ ] Roadmap, feature, security, threat, Trust-Wedge, issue-resolving, forensic, and implementation-prompt material was reviewed.
- [ ] Existing tests were inventoried before adding duplicates.
- [ ] Every assertion maps to a concrete production boundary.
- [ ] Real temporary workspaces are used for filesystem acceptance.
- [ ] Authorization tests verify zero unauthorized mutation.
- [ ] ExpectedState tests verify zero stale overwrite.
- [ ] Path/symlink/traversal tests exist where supported.
- [ ] Concurrency tests use bounded deterministic coordination.
- [ ] Failure injection covers consequential boundaries where practical.
- [ ] Restart tests prove persistence where persistence exists.
- [ ] MCP/CLI/API/TUI parity is tested where exposed.
- [ ] Exact-byte Unicode/newline cases are covered.
- [ ] Secret leakage assertions exist.
- [ ] Architecture regression tests protect canonical ownership.
- [ ] Missing production features are reported rather than simulated.
- [ ] No test-only authorization bypass exists.
- [ ] Required tests are not silently ignored.
- [ ] Verification commands actually ran.
- [ ] Final reporting distinguishes verified, unverified, partial, unavailable, and environment-limited behavior.

## 44. Independence and strict scope
Execute against the current rust branch. Do not assume Prompt 15, Prompt 17, or historical PRs are merged.
Do not modify another docs/implementation-prompts/*.md file.
Do not modify docs/trust-wedge/ or docs/issue-resolving-prompts/.
If a referenced production capability is absent, report it rather than implementing an unrelated subsystem.
The documentation file changed by this prompt is only docs/implementation-prompts/16-testing-and-acceptance.md.

## 45. Linear implementation sequence
1. Read product, roadmap, security, architecture, and current implementation contracts.
2. Inspect Prompts 01–15 and Prompt 17 ownership.
3. Inspect historical Trust-Wedge and issue-resolving evidence.
4. Inventory current tests and fixtures.
5. Trace production call paths.
6. Build the before-state acceptance map.
7. Identify missing high-value evidence.
8. Build isolated fixtures and minimal test seams.
9. Add service integration tests.
10. Add authorization and zero-side-effect tests.
11. Add ExpectedState/conflict tests.
12. Add filesystem/path/symlink/TOCTOU tests.
13. Add snapshot/provenance/rollback tests where implemented.
14. Add audit persistence/redaction/correlation tests where implemented.
15. Add MCP, CLI, API, and TUI tests where exposed.
16. Add concurrency, failure-injection, and restart tests.
17. Add architecture-regression tests.
18. Run full verification.
19. Re-run the acceptance map against final behavior.
20. Classify every result and produce the final report.

## 46. Final invariant
The finished test suite must answer, with executable evidence and without overstating implementation status:
Can an authorized agent safely perform the supported edit workflow, can an unauthorized or stale request be rejected without unsafe mutation, can recovery and audit be trusted where implemented, do interfaces converge on the same canonical semantics, and are Trust-Wedge security boundaries preserved under concurrency, restart, malformed input, and failure?
Where the answer is no or unproven, the report must say so explicitly.