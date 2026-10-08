# Prompt 14 — Filesystem TOCTOU + Mutation Coordination (FS-001)

## Mission
Implement one canonical AWH filesystem mutation-coordination boundary that closes practical race windows around path validation, expected-state checking, and final mutation while preserving workspace containment, symlink safety, atomic writes, edit semantics, snapshot/recovery guarantees, and authorization.

This prompt owns filesystem coordination, not a replacement filesystem service. Harden the existing canonical FilesService/EditService and filesystem primitives. Do not create a second editor, path resolver, lock service, snapshot store, rollback engine, authorization system, or audit store.

The core race is: resolve and validate path → read/check state → prepare → mutate. A concurrent actor can change the target between these steps. Revalidate at the final mutation boundary as far as the supported platform permits. Never claim universal TOCTOU freedom.

## 1. Product boundary
AWH owns controlled filesystem state and agent-grade editing. CLI, MCP, TUI, Control API, and service callers must converge on the same mutation coordination semantics.

Desired flow:
caller identity → workspace/worktree resolution → authorization/policy → canonical path validation → coordination → final target revalidation → live FileState/ExpectedState check → preparation → required recovery capture → canonical atomic commit → verification → audit → release.

The OS remains the ultimate filesystem authority. AWH coordination is not an OS/container/VM sandbox.

## 2. Scope
Own:
- final-component TOCTOU mitigation;
- concurrent filesystem mutation coordination;
- per-resource locking and resource-key derivation;
- expected-state revalidation at the mutation boundary;
- create/replace/delete/patch and multi-file mutation coordination;
- lock timeout, cancellation, release, and deadlock avoidance;
- atomic-write coordination;
- symlink substitution and deletion/recreation races;
- concurrency, crash, and failure-injection tests;
- integration with existing FilesService/EditService.

Do not own:
- edit transaction vocabulary or operation algorithms;
- snapshots/provenance;
- rollback;
- persistent audit;
- capability/policy authorization;
- agent/session identity;
- Git worktree lifecycle;
- MCP routing, CLI redesign, TUI redesign, Control API redesign;
- distributed locks;
- OS/container sandboxing;
- Git merge/conflict resolution;
- a generic filesystem database.

## 3. Required repository forensics
Before coding, read the current rust branch:
- docs/roadmap/GROWTH_STRATEGY.md
- docs/roadmap/PROJECT_ROADMAP.md
- docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
- docs/FEATURES.md
- docs/PROJECT_CONTEXT.md
- docs/architecture.md
- docs/security.md
- docs/threat-model.md
- docs/implementation-prompts/README.md
- implementation prompts 01–13, especially 04–10, 12, and 13
- docs/trust-wedge/
- docs/issue-resolving-prompts/
- AWH-FORENSIC-REPOSITORY-REPORT.md
- relevant archive/forensic material.

Inspect the actual source, not assumed filenames, including:
- src/services/files.rs
- src/services/edit.rs
- src/services/snapshot.rs
- src/services/authorization.rs
- src/services/audit.rs
- src/services/mod.rs
- the actual StoreLock/locking implementation;
- workspace/path helpers;
- agent/session/worktree context;
- MCP/CLI/TUI/Control API mutation paths;
- all relevant tests.

Search for StoreLock, Mutex, RwLock, write_atomic, OpenOptions, canonicalize, symlink, rename, remove_file, ExpectedState, FileState, sha256, TOCTOU, worktree, and direct filesystem writes. Produce a race map before changing code.

## 4. Existing contract and reuse rule
The current branch contains substantial filesystem/edit safety behavior and a StoreLock/storage coordination primitive. Verify the implementation before relying on it.

Reuse the canonical filesystem and atomic-write primitives. If an existing helper is insufficient, harden it or introduce the smallest shared coordination abstraction. Do not create a parallel FilesService or editor.

The forensic material identifies final-component TOCTOU and unserialized cross-agent file mutations as concrete risks. Confirm their current status before selecting synchronization.

## 5. Race model
Explicitly analyze at least:
1. validation-to-write race;
2. symlink substitution;
3. deletion followed by recreation;
4. same-file concurrent edits;
5. concurrent creation of a missing file;
6. overlapping multi-file transactions;
7. workspace/worktree root replacement where relevant;
8. lock acquisition versus cancellation;
9. process crash during a coordinated mutation.

For every race, state whether the implementation provides serialization, conflict detection, rejection, or only a documented best-effort mitigation.

## 6. Canonical coordination model
Define one canonical coordination abstraction.
A resource key must be derived only after canonical workspace/worktree and path validation. Conceptually it contains workspace identity + worktree identity when applicable + canonical relative resource path.

Never key correctness-sensitive coordination solely by an unvalidated caller path or by an absolute host path.

For one file, the protected sequence should be approximately:
authorize → validate root/path → derive resource key → acquire coordination → revalidate target identity/path → read live state → compare expected state → prepare/commit → verify → release.

Use the smallest scope that protects correctness. Do not globally serialize unrelated files.

## 7. Lock ordering and deadlock prevention
Inspect the real existing lock graph before choosing an order.
If multiple resources are needed, sort resource keys deterministically before acquisition and release in a deterministic safe order.

Never create cyclic acquisition such as A→B in one path and B→A in another.

Do not hold a filesystem coordination lock while awaiting model inference, network calls, MCP calls, interactive input, or unrelated subprocess work.

Cancellation while waiting must stop safely where possible. Cancellation after acquisition must release every acquired resource.

Do not create a distributed locking service.

## 8. Final-component TOCTOU
Identify exactly where current code validates the workspace root, parent, target, symlinks, and expected state.

Move or repeat the necessary checks immediately before mutation.

Where supported, investigate descriptor-relative or contained-resolution primitives. On Linux, evaluate openat2 with suitable resolution restrictions where it can be integrated without breaking portability. Do not make Linux-only behavior the universal contract unless the product explicitly accepts that.

A portable approximation may canonicalize the existing parent, validate the target, create the temporary file inside the verified parent, revalidate immediately before rename, and abort on an identity change. Document the residual race window. A second canonicalize call alone is not proof of race freedom.

## 9. Path, symlink, and object identity
Preserve canonical workspace/worktree containment.
Reject absolute paths, traversal, sibling-prefix confusion, ambiguous paths, main-workspace targets where a managed worktree is required, and symlink escapes.

Distinguish path identity, canonical path identity, filesystem object identity where available, and file-content state.

Do not collapse missing and zero-byte files.

Where the platform exposes stable file identity, use it only if it is appropriate and portable enough for the contract. Document platform limitations rather than faking universal identity guarantees.

## 10. Expected-state semantics
ExpectedState remains the canonical stale-state contract.

After acquiring coordination, read the live state and compare it with the expected state. If it differs, return the canonical conflict and do not overwrite. Never silently refresh the expected state and retry.

Do not turn coordination into authorization or into a replacement for ExpectedState.

Where an existing operation intentionally permits no expected state, preserve its existing contract while ensuring that coordination does not create an unsafe stale-write path.

## 11. Atomic write integration
Reuse the existing canonical write_atomic behavior.
Do not implement a second temporary-file writer, fsync implementation, rename primitive, or permission policy.

Temporary files must remain within the intended filesystem boundary and atomic-rename constraints.

After commit, use the existing verification contract. If atomic multi-file commit cannot be guaranteed, do not describe it as all-or-nothing.

## 12. Create, replace, delete, and edit operations
For create:
authorize → validate → coordinate → recheck absence → commit → verify.
If another actor creates the target, do not overwrite it merely because an earlier read observed absence.

Replace, insert, delete-range, patch, and apply-diff must continue to use the canonical EditService/EditTransaction semantics.

Do not implement a second occurrence search, line algorithm, diff parser, transaction model, or editor.

A prepared edit may be computed before locking for performance only if live state is re-read and validated after coordination is acquired.

## 13. Multi-file transactions
For a multi-file edit:
1. resolve and validate all paths;
2. derive and sort resource keys deterministically;
3. acquire all required coordination resources;
4. re-read every live state;
5. validate ExpectedState for every member;
6. prepare all results;
7. capture required recovery material through the canonical snapshot boundary;
8. commit according to the existing EditService contract;
9. verify every result;
10. release resources.

If one member fails, follow the existing Prompt 06 bounded partial-commit/recovery semantics. Do not create a new transaction or rollback model.

Test overlapping sets such as A=[a,b], B=[b,c], C=[c,d] and prove deterministic acquisition without deadlock.

## 14. Interaction with worktrees
Prompt 13 owns Git worktree lifecycle and ownership. Prompt 14 owns filesystem mutation coordination inside the effective workspace/worktree root.

Independent worktrees must not be globally serialized merely because they contain the same relative filename.

If two sessions intentionally share one worktree, they must converge on the same resource key and coordination boundary.

Worktree identity must therefore participate in resource identity where applicable.

## 15. Snapshots, provenance, rollback, and audit
Prompt 08 owns snapshots/provenance. Prompt 09 owns rollback. Prompt 10 owns persistent audit.

Coordination must ensure the pre-edit state used for required recovery capture is the state being mutated.

If required snapshot capture fails before mutation, preserve the existing safety rule that mutation is blocked.

Rollback must acquire its own appropriate coordination and produced-state checks; do not assume an edit lock survives across operations.

Audit lifecycle/conflict events through the canonical audit service where required. Do not create a coordination-specific audit store or persist secrets/full file contents.

## 16. Authorization boundary
Authorization and policy remain authoritative.

Conceptual ordering:
identity → capability/policy authorization → workspace/worktree ownership → path validation → coordination → ExpectedState → mutation → verification → audit.

Locks do not grant permission. A denied request must not mutate the filesystem.

Do not create a filesystem-specific authorization engine.

## 17. Lock failure, cancellation, and recovery
Define structured outcomes for acquisition timeout, cancellation, invalid key, underlying filesystem lock failure, unavailable coordination infrastructure, and any stale metadata state supported by the chosen primitive.

Correctness-sensitive mutation must fail closed on lock failure; never continue unlocked after a timeout.

Inspect whether the existing primitive is process-local, file-based, OS advisory, or another mechanism. Recovery must match its real semantics.

Never delete a possibly live lock merely because it is old. Prefer OS/process-owned release behavior when available.

Do not hold blocking waits on an async runtime worker when the current architecture provides a safe blocking-task mechanism.

## 18. Resource limits and fairness
Bound lock wait time, multi-file transaction size, lock-key length, and in-memory lock-registry growth if such a registry is introduced.

Do not build an unbounded map keyed by attacker-controlled paths.

Document fairness guarantees. Do not claim FIFO fairness without evidence.

## 19. Required security invariants
All must remain true:
1. no mutation escapes the canonical workspace/worktree root;
2. symlink substitution cannot silently redirect a protected mutation where the supported platform can detect it;
3. stale ExpectedState cannot be silently overwritten;
4. lock failure never means proceed unlocked;
5. authorization denial produces no mutation;
6. lock state never grants authorization;
7. same-file conflicting mutations are coordinated or rejected;
8. disjoint files are not unnecessarily globally serialized;
9. multi-file lock ordering cannot deadlock;
10. atomic single-file semantics remain intact;
11. missing and zero-byte states remain distinct;
12. no secrets enter coordination metadata/errors;
13. no shell is involved in filesystem coordination;
14. platform limitations are documented honestly.

## 20. Required tests
Use real temporary files/directories and, where Git interaction matters, isolated temporary repositories.

Test:
- same-file concurrent replace/replace;
- replace/delete;
- insert/replace;
- patch/patch;
- create/create;
- stale ExpectedState;
- deletion/recreation;
- symlink substitution;
- parent/path replacement;
- sibling-prefix confusion;
- absolute/traversal paths;
- cross-worktree isolation;
- overlapping multi-file transactions;
- disjoint multi-file transactions;
- lock timeout;
- cancellation;
- process/crash recovery;
- temporary-file cleanup;
- atomic-write failure injection;
- post-commit verification failure;
- required snapshot failure;
- canonical audit integration;
- CLI/MCP/TUI/Control API convergence where supported.

Race tests should deliberately pause between validation, lock acquisition, revalidation, commit, and verification so a competing task can modify the target.

## 21. Exact-byte tests
Prove coordination does not alter bytes. Include:
- empty file;
- zero-byte file;
- LF;
- CRLF;
- mixed newline;
- no final newline;
- UTF-8;
- Devanagari;
- emoji;
- supported arbitrary valid bytes.

Compare exact SHA-256 and length before and after relevant operations.

## 22. Failure truth table
| Condition | Required result |
|---|---|
| Authorization denied | No mutation |
| Invalid path | Reject before mutation |
| Lock timeout | Fail closed |
| Lock canceled | No mutation; release held resources |
| ExpectedState mismatch | Conflict; no stale overwrite |
| Target replaced before final validation | Detect/reject safely |
| Symlink substitution | Reject or use proven contained operation |
| Required snapshot unavailable | Block mutation |
| Atomic commit failure | Structured failure; recovery contract applies |
| Verification failure | Verification/recovery contract applies |
| Concurrent same-file mutation | Coordinate or conflict |
| Concurrent disjoint files | Avoid unnecessary global serialization |
| Corrupt coordination state | Fail closed |

## 23. Duplicate-mechanism audit
After implementation, search for newly introduced filesystem locks, path validators, atomic writers, symlink checkers, ExpectedState comparators, mutation queues, and lock registries.

There must be one canonical owner for each concern. Avoid a second FilesService, EditService, StoreLock, path-security system, transaction engine, snapshot system, rollback system, authorization system, or audit store.

If a pre-existing duplicate cannot safely be removed, document it and ensure the new code does not become a competing authority.

## 24. Linear implementation sequence
1. Read all current product, feature, roadmap, Trust Wedge, issue-resolving, forensic, and implementation-prompt material.
2. Inventory every filesystem mutation path and synchronization primitive.
3. Build the concrete race map.
4. Identify existing mitigations and residual races.
5. Define one coordination abstraction and resource-key contract.
6. Define lock scope, ordering, timeout, cancellation, and recovery.
7. Harden final-component/path identity checks.
8. Integrate coordination into the canonical filesystem/edit mutation path.
9. Preserve ExpectedState and atomic-write semantics.
10. Integrate multi-file coordination.
11. Integrate snapshot/recovery, authorization, worktree ownership, and audit without duplicating them.
12. Add deterministic race, concurrency, symlink, crash, cancellation, and failure-injection tests.
13. Perform duplicate-mechanism audit.
14. Validate performance and platform limitations.
15. Run verification gates.
16. Review the final diff for scope leakage.

Do not begin with a global mutex. Prove the race model first and choose the smallest correct synchronization boundary.

## 25. Platform limitations
Document actual behavior for Linux, macOS, Windows, and Android/Termux where practical.

Address rename atomicity, symlink behavior, file identity, advisory-lock behavior, descriptor-relative operations, openat2 availability, and network-filesystem limitations.

Do not describe portable best-effort checks as kernel-enforced containment.

## 26. Verification gates
Run:
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

Also run all applicable filesystem security, edit/concurrency, race, failure-injection, and real-terminal/integration tests.

Do not declare completion from compilation alone.

## 27. Completion criteria
Complete FS-001 only when:
- the real current race model is documented;
- one canonical coordination mechanism exists;
- final-component TOCTOU is mitigated as far as supported platforms permit;
- path/symlink containment remains intact;
- ExpectedState remains authoritative;
- atomic write semantics remain intact;
- same-file mutations are coordinated or safely rejected;
- overlapping multi-file operations have deterministic lock ordering;
- disjoint resources are not globally serialized without justification;
- timeout/cancellation/crash behavior is deterministic;
- no deadlock is introduced;
- snapshot/recovery and rollback boundaries remain correct;
- authorization/worktree ownership remain authoritative;
- canonical audit is used;
- adversarial and failure-injection tests exist;
- platform limitations are documented;
- verification gates pass;
- no duplicate filesystem/lock/edit/security subsystem was introduced.

## 28. Independence and strict scope
Implement against the current rust branch. Do not wait for Prompt 13, 15, 16, or 17 or any historical PR.

Reuse current contracts. If a referenced contract is absent, implement only the minimum compatibility boundary required for FS-001 and report the limitation.

Do not modify another docs/implementation-prompts file.

## 29. Final implementation report
Report:
1. product/roadmap/Trust Wedge/issue-resolving/forensic material inspected;
2. filesystem mutation paths inspected;
3. existing synchronization primitives reused;
4. concrete race windows and existing mitigations;
5. coordination/resource-key design;
6. lock scope/order/deadlock analysis;
7. final-component TOCTOU mitigation;
8. path/symlink/object-identity handling;
9. ExpectedState and atomic-write integration;
10. multi-file behavior;
11. snapshot/recovery/rollback interaction;
12. authorization/worktree/audit interaction;
13. crash/cancellation behavior;
14. platform limitations;
15. race/concurrency/security/failure-injection evidence;
16. performance considerations;
17. duplicate-mechanism audit;
18. exact verification results;
19. changed files and known limitations;
20. explicit confirmation that no other implementation prompt was modified and no duplicate subsystem was introduced.

The report must distinguish verified guarantees from best-effort and platform-dependent behavior.