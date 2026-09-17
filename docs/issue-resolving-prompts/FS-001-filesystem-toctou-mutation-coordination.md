# FS-001 — Filesystem TOCTOU and Mutation Coordination — Master Implementation Prompt

## 0. Mission

Harden AWH's canonical filesystem mutation path against final-component TOCTOU races and coordinate concurrent mutations across agents/processes without weakening workspace containment, symlink protections, atomicity, expected-state conflict detection, or the existing AWE editing contract.

The objective is a portable, evidence-backed filesystem mutation contract, not an unprovable claim of perfect race freedom on every operating system.

Current forensic evidence shows two important mutation paths that must be reconciled: `WorkspaceMcp::write_file` uses temporary-file installation but still performs security resolution before the final mutation, while `FilesService::write` has a direct `fs::write` path. The current `secure_path`/`secure_destination` implementation also performs a check/resolution followed by a later filesystem operation, creating a final-component check-then-use boundary that FS-001 must address.

## 1. Scope and non-negotiable rules

1. Work only on FS-001.
2. Do not begin GIT-001 or later issues.
3. Do not redesign ARCH-001's entire service/store architecture. Use its canonical filesystem ownership boundary; FS-001 supplies the concrete mutation-safety implementation.
4. Preserve AWE-001/AWE-002/AWE-003 semantics unless a concrete filesystem correctness defect requires a compatible change.
5. Establish current implementation truth before coding; do not rely on old forensic reports when source/tests disagree.
6. The canonical filesystem mutation path must be transport-independent.
7. Do not keep separate unsafe mutation implementations in MCP and service/core layers.
8. Do not claim TOCTOU elimination merely because a path was canonicalized before mutation.
9. Do not claim perfect race freedom unless a platform-specific proof establishes it.
10. Prefer the strongest portable security contract, with stronger platform-specific paths where safely available.
11. No unsafe fallback from a failed security check to an unchecked filesystem operation.
12. Keep unrelated refactors out of the patch.

## 2. Mandatory forensic preflight

Before changing code, inspect the current `rust` branch and FS-001 issue.

Read at minimum:

- `docs/PROJECT_CONTEXT.md`
- `docs/PROJECT_STATUS.md`
- `docs/MASTER_PROMPT.md`
- `docs/mcp.md`
- `docs/testing.md`
- relevant security documentation
- `src/services/edit.rs`
- canonical filesystem/file service modules
- `src/mcp/security.rs`
- `src/mcp/workspace.rs`
- `src/mcp/dispatcher.rs`
- MCP filesystem/workspace tool handlers
- existing filesystem, security, edit, and integration tests

Search repository-wide for:

- `FilesService`
- `WorkspaceMcp`
- `secure_path`
- `secure_destination`
- `atomic_write`
- `fs::write`
- `fs::rename`
- `fs::remove_file`
- `remove_dir`
- `OpenOptions`
- `NamedTempFile`
- `persist`
- `canonicalize`
- `symlink`
- `EditTransaction`
- `ExpectedState`
- SHA-256/state verification
- `StoreLock`
- filesystem mutation locks
- all MCP/CLI/TUI/control callers of filesystem operations

Produce this mutation-path matrix before implementation:

| Operation | Current caller | Current implementation | Security check | Final mutation | Coordination | Expected-state check | Atomicity | TOCTOU exposure |
|---|---|---|---|---|---|---|---|---|
| read | verify | verify | verify | n/a | verify | n/a | n/a | verify |
| write/create | verify | verify | verify | verify | verify | verify | verify | verify |
| replace/edit | verify | verify | verify | verify | verify | verify | verify | verify |
| rename | verify | verify | verify | verify | verify | verify | verify | verify |
| delete | verify | verify | verify | verify | verify | verify | verify | verify |
| directory mutation | verify | verify | verify | verify | verify | n/a | verify | verify |

Do not implement until all production mutation paths are known.

## 3. Canonical ownership boundary

All security-sensitive filesystem mutations must converge on one canonical service/core implementation.

```text
MCP / CLI / TUI / Control API
             ↓
     transport/interface adapter
             ↓
     canonical filesystem service
             ↓
 path validation + mutation coordination
             ↓
 secure final-component operation
             ↓
 verification / expected-state / audit as applicable
```

MCP must not independently implement path containment, symlink-escape policy, final-component race mitigation, mutation locking, atomic replacement semantics, expected-state conflict logic, or alternate file mutation engines.

`secure_path` and related helpers may remain reusable primitives, but a helper is not itself the canonical mutation boundary if callers can perform an unsafe operation after it returns. The implementation must make it difficult for a new caller to perform a check-then-use mutation outside the canonical path.

## 4. Threat model

Model an attacker or competing agent/process that can mutate filesystem entries within or adjacent to the workspace while another agent performs a checked mutation.

At minimum cover:

1. final-file replacement with a symlink or different object;
2. parent-directory replacement with a symlink or different directory;
3. create-vs-replace races;
4. rename destination races;
5. delete races;
6. concurrent legitimate writers causing lost updates;
7. check/use splits across service boundaries.

For every race, state whether it is prevented, detected, serialized, or outside the supported portable guarantee.

## 5. Final-component TOCTOU requirements

The key invariant is:

> A successful security check must not be treated as proof that the final filesystem object is unchanged at a later mutation point unless the implementation actually binds the check and mutation strongly enough to justify that claim.

Do not solve the problem by merely adding another `canonicalize()` immediately before `fs::write()`.

Evaluate platform-appropriate mechanisms such as:

- no-follow/exclusive open flags where supported;
- descriptor-based operations where available;
- directory/file handles that bind operations to checked objects;
- atomic temporary-file creation in the validated parent followed by safe replacement;
- mutation locks for cooperating agents/processes;
- isolated platform-specific implementations.

Choose mechanisms from APIs/dependencies actually available in the repository. Do not invent APIs or assume Unix-only flags are portable.

For each mechanism document which race it prevents, which it only detects, unsupported-platform behavior, and the resulting security contract.

## 6. Parent-directory and destination safety

Protect the entire path relevant to the mutation, not only the final filename.

Requirements:

- identify the workspace root safely;
- keep every relevant existing parent component inside the workspace policy;
- reject symlink traversal that escapes the workspace;
- ensure creation of missing parents cannot introduce an unchecked symlink component;
- analyze destination-parent replacement races;
- retain absolute-path and traversal rejection;
- test platform-specific path semantics.

A successful parent check followed by unchecked `create_dir_all` or equivalent must be treated as part of the same threat model.

## 7. Safe write/create/replace contract

Define distinct semantics for create-only, replace-existing, and generic write operations.

### Create-only

A concurrent creation must not be silently replaced. Use an exclusive/no-follow mechanism where supported or return a conflict.

### Replace-existing

Replace the intended object without following an attacker-controlled final symlink. Prefer atomic replacement where compatible with expected-state semantics.

### Generic write

Do not expose ambiguous semantics for create/replace/follow-symlink behavior.

### Temporary-file replacement

Temporary files must be created in the intended parent, have appropriate permissions, be fully written and flushed/synced according to the durability contract, and be installed using a replacement mechanism whose destination race is understood.

If `NamedTempFile::persist` or equivalent is used, explicitly analyze destination replacement between validation and installation.

## 8. Rename and delete safety

FS-001 is not complete if only writes are hardened.

For rename:

- validate source and destination independently;
- validate their parent directories;
- define cross-device behavior;
- define replacement semantics;
- analyze destination symlink behavior;
- coordinate concurrent mutations;
- preserve workspace containment for both endpoints.

For delete:

- do not rely solely on a stale pre-check;
- define behavior when the object changes between check and delete;
- never follow an attacker-controlled path outside the workspace;
- distinguish not-found, conflict, security, and I/O outcomes.

For directory mutation, explicitly define recursive-operation and symlink semantics if supported.

## 9. Mutation coordination

Filesystem security and concurrent-agent coordination are separate but related concerns.

Introduce per-path or per-resource advisory mutation coordination where it materially reduces races and lost updates.

The mechanism must be process-aware when required, bounded, reliably released, leak-resistant, safe under contention, keyed consistently across equivalent paths, and scoped so unrelated files do not unnecessarily serialize.

Document it as advisory unless it genuinely provides a stronger kernel-enforced property.

Reuse an existing locking primitive only when its semantics fit. Do not assume the JSON-store `StoreLock` is automatically the correct filesystem mutation lock.

A lock must never be the sole security argument against a malicious process that can ignore advisory locks.

## 10. Expected-state conflict enforcement

Preserve the existing AWE expected-state model.

For mutations carrying expected state:

```text
identify target
→ establish expected state
→ coordinate mutation
→ verify expected state at the correct mutation boundary
→ mutate safely
→ verify resulting state
```

Requirements:

- stale expected state produces a conflict, not silent overwrite;
- existing AWE hash/content/metadata semantics remain intact unless a required correction is documented;
- concurrent legitimate agents cannot silently lose updates;
- conflicts remain distinguishable from security denials;
- failed conflict checks do not partially mutate the target.

Do not weaken optimistic concurrency merely because a mutation lock exists.

## 11. Atomicity and durability

Define what atomic means for each operation. Distinguish:

- atomic visibility of replacement;
- no partially written contents;
- durability after process crash;
- durability after power/machine failure;
- directory-entry durability where relevant.

Do not claim crash durability merely because a file `sync_all()` was called. If directory synchronization or platform-specific guarantees are required, document them. If full durability is outside the project contract, state the weaker guarantee explicitly.

## 12. Platform contract

Define the actual contract for supported environments:

| Platform | Strong mechanism | Fallback | Security guarantee | Durability guarantee | Tests |
|---|---|---|---|---|---|
| Linux | verify | verify | verify | verify | verify |
| Android/Termux | verify | verify | verify | verify | verify |
| macOS | verify | verify | verify | verify | verify |
| Windows | verify | verify | verify | verify | verify |

Do not make unsupported claims about `O_NOFOLLOW`, `openat`, Windows sharing flags, rename semantics, or filesystem behavior without verifying actual APIs/dependencies.

Isolate stronger platform-specific implementations behind small abstractions. The fallback must not silently become an unchecked unsafe path operation.

## 13. Race-testing strategy

Unit tests alone are insufficient. Add deterministic and stress-oriented tests where practical.

Required categories:

1. checked-path → final-symlink replacement;
2. checked-parent → parent replacement;
3. concurrent create;
4. concurrent replace;
5. rename destination race;
6. delete race;
7. two legitimate concurrent writers;
8. expected-state conflict under concurrency;
9. lock contention and timeout;
10. stale-lock recovery where supported;
11. process crash while holding a mutation lock;
12. restart after interrupted mutation;
13. Unicode/path-normalization edge cases;
14. nested workspace/path containment;
15. permission/read-only failure.

Prefer barriers, channels, synchronization hooks, and controlled subprocesses over arbitrary sleeps. Where exact scheduling cannot be made deterministic, use repeated stress iterations and assert the security invariant.

## 14. Cross-process testing

When cross-process coordination is part of the chosen design, at least one test path must use independent processes or equivalent process isolation.

Prove lock visibility, bounded contention, no unsafe fallback, concurrent-update behavior, and recovery after unexpected process termination.

If the environment prevents a required cross-process test, report that limitation and do not claim the property was verified.

## 15. Security regression requirements

Preserve:

- workspace containment;
- traversal rejection;
- absolute-path policy;
- symlink-escape prevention;
- correct workspace identity;
- caller/session authorization where applicable;
- expected-state conflict enforcement;
- canonical MCP mutation routing;
- no new secret leakage through errors/logs.

Existing `secure_path`/`secure_destination` property tests are regression coverage, not proof of final-component TOCTOU safety. Add tests for the gap between validation and final mutation.

## 16. Error and failure contract

Define behavior for:

- target missing;
- target already exists during create-only operation;
- target changed after expected-state capture;
- final component is a symlink;
- parent is a symlink;
- path escapes workspace;
- permission denied;
- lock timeout;
- stale lock;
- concurrent mutation conflict;
- unsupported platform capability;
- cross-device rename;
- atomic installation failure;
- temporary-file cleanup failure;
- interrupted process;
- malformed expected state.

Errors must never trigger a retry through an unsafe primitive. Security failures, conflicts, not-found results, and I/O failures should remain distinguishable at the canonical service boundary.

## 17. Compatibility requirements

Before changing behavior inspect MCP workspace tools, FilesService callers, EditService/EditTransaction callers, CLI/TUI/control callers, exact-error tests, platform path handling, and existing atomic-write behavior.

If an API must change, provide a safe compatibility adapter and document the behavioral difference. Do not preserve an unsafe API merely for compatibility if it bypasses the new security contract.

## 18. Verification and architecture checks

After implementation:

1. Search for all direct filesystem mutation calls in MCP/domain code.
2. Confirm they route through the canonical mutation boundary or have an explicit reviewed exception.
3. Review every `secure_path`/`secure_destination` caller for unsafe check-then-use behavior.
4. Confirm write, replace, rename, and delete semantics are covered.
5. Confirm expected-state conflicts remain enforced.
6. Confirm mutation coordination is consistent across MCP and non-MCP callers.
7. Confirm no unsafe unlocked fallback exists.
8. Run formatting and relevant unit/integration/security/concurrency tests.
9. Run available platform-specific CI tests.
10. Review the final diff for unrelated changes.
11. Document actual platform guarantees and residual limitations.

If a race cannot be proven impossible, explicitly classify it as prevented, detected, serialized, or outside the supported threat model.

## 19. Definition of Done

FS-001 is complete only when:

- one canonical filesystem mutation implementation exists;
- MCP and non-MCP callers converge on it;
- final-component TOCTOU exposure is materially reduced under the documented threat model;
- parent/destination symlink races are addressed;
- writes have clear create/replace semantics;
- rename and delete are hardened;
- process-level coordination works where required;
- advisory locking is not mistaken for a security boundary;
- expected-state conflict enforcement remains intact;
- concurrent updates cannot be silently lost under the documented contract;
- atomicity/durability guarantees are explicit;
- Linux/Android and other supported platforms have verified behavior or explicit limitations;
- deterministic/stress/concurrent tests cover important race classes;
- failure paths never fall back to unchecked mutation;
- repository-wide review finds no unintended parallel unsafe mutation path.

## 20. Non-goals

Do not use FS-001 to:

- redesign all AWH stores;
- redesign MCP authorization;
- implement agent worktree isolation;
- redesign Git coordination;
- replace the AWE edit model without a concrete correctness reason;
- claim complete kernel-level filesystem race freedom across all platforms;
- perform unrelated filesystem cleanup/refactoring.

## 21. Hard stop conditions

Stop and report rather than guessing when:

- the canonical mutation path cannot be established;
- chosen platform API semantics are uncertain;
- a fallback would weaken containment or symlink safety;
- atomic replacement semantics differ materially and no safe contract can be defined;
- a change would break AWE expected-state guarantees;
- concurrency requirements conflict with public behavior and no compatible contract is established;
- required cross-process/platform verification cannot be performed.

The final result must be a demonstrably safer, coordinated, canonical filesystem mutation path with explicit guarantees—not merely additional path checks around the same TOCTOU window.
