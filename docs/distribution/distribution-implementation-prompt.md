# 28 — Distribution (DST-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 28 / DST-001  
**Feature:** Distribution  
**Primary roadmap area:** Build, packaging, release and installation  
**Target branch:** rust  
**Canonical folder:** docs/distribution/

### Mission
Implement, converge, and verify Distribution inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Define reproducible AWH build/release artifacts.
- Cover version/build information, install/upgrade/uninstall, checksums, shell completion and practical platform targets.
- Verify Linux x86_64/ARM64, macOS x86_64/ARM64, Windows x86_64 and Android/Termux ARM64 where evidence exists.
- Make artifacts traceable to source commit/version and preserve runtime security defaults.

## Out of scope
- No package-manager rewrite.
- No runtime security changes.
- No unsupported platform claims.
- No secrets in artifacts.

## Product boundary
AWH owns binary/configuration/release artifact behavior; external package ecosystems are delivery channels.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
Cargo.toml, main/version/build code, .github/workflows, release/install scripts, completion generation, README/install docs and target support.

Search before adding abstractions:
rg -n "version|build_info|release|artifact|checksum|install|uninstall|completion|target|aarch64|x86_64"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
Source commit → deterministic build → CI verification → artifact → checksum/signature metadata → release manifest → install/upgrade/uninstall. Publishing requires verification.

### Architectural invariants
1. One canonical domain/service owner.
2. One canonical model and persistence authority where persistence is required.
3. Existing workspace, agent, session, worktree and project identity remain authoritative.
4. PolicyEngine/capability authorization remains authoritative.
5. Existing FilesService/EditService/Snapshot/Rollback/Audit/Git/MCP boundaries are reused.
6. Interface adapters do not create alternate business semantics.
7. Structured errors distinguish validation, authorization, state, transport and execution failures.
8. Existing locking/concurrency primitives are reused.
9. Restart and corruption behavior are explicit.
10. Source existence is not treated as verification evidence by itself.

## State and lifecycle
Source → Build → Test → Package → Verify → Publish → Install → Upgrade → Uninstall. Failed verification blocks publication. User .agent state is never deleted by ordinary upgrade/uninstall.

# 4. Interfaces

## Rust/service interface
Define version/build metadata with explicit determinism policy. Do not embed nondeterministic data without labeling it.

## CLI interface
Ensure awh version/status/doctor and shell completion work from release artifacts; installer behavior and exit semantics are documented.

## Control API interface
Not primary; version/build metadata may be exposed through API using existing service.

## MCP interface
MCP server artifacts inherit the same version/security defaults; no MCP-specific release semantics.

## TUI interface
TUI displays version through canonical build metadata.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Installers must not add default credentials or disable security. Installation directories use safe permissions.

## Input/resource safety
Validate target/version/artifact names/checksum inputs and avoid shell injection in packaging scripts.

## Isolation
Separate binaries/config/state; upgrades preserve .agent workspaces.

## Secrets and sensitive data
CI/release secrets are never embedded or written to generated config/logs.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Release manifests/checksums are atomically generated; installed versions are detectable for safe upgrade.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | Distribution responsibility |
|---|---|---|
| Workspace/project | init/workspace services | resolve/enforce scope |
| Agent/session | AgentRuntimeService | propagate identity |
| Capability/policy | PolicyEngine/authorization | enforce consequential actions |
| Filesystem | FilesService | reuse path/resource safety |
| Edit/recovery | EditService/Snapshot/Rollback | reuse canonical mutation/recovery |
| Audit | persistent AuditLog | emit correlated redacted events |
| Git/worktree | Git/worktree services | reuse where applicable |
| MCP | MCP dispatcher/session | adapter only |

## No-duplication rule
Do not introduce parallel identity, auth, policy, filesystem safety, edit, snapshot, rollback, audit, Git, MCP session, or persistence systems. Use adapters at boundaries.

# 8. Testing

## Unit tests
Test version parsing, target mapping, naming, checksums and completion.

## Integration tests
Build real artifacts for available targets; run version/status/doctor; verify checksum and isolated install/upgrade.

## Security/adversarial tests
Tampered artifact/checksum, installer path traversal, unsafe permissions, malicious version input, secret leakage.

## Failure/recovery tests
Build/toolchain failure, checksum mismatch, partial install, interrupted upgrade/uninstall.

## Compatibility/real-interface tests
Cargo metadata, CI/release workflows, install docs and target claims.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Preserve CLI config/state locations. Version schema/config changes explicitly.

## Rollout order
1. Inspect current contract and implementation.
2. Establish/extend canonical service and model.
3. Establish/migrate persistence only where required.
4. Integrate existing identity and security boundaries.
5. Add interface adapters.
6. Add unit/integration/security/recovery tests.
7. Run verification gates.
8. Update feature-specific docs/status only for behavior actually proven.

No step depends on another prompt or PR being merged first.

## Failure behavior
Nothing is published as verified when required verification failed; installers stop on integrity failure and preserve user state.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] Traceable version/build identity.
- [ ] Checksums/target metadata.
- [ ] Actual target build evidence.
- [ ] Safe install/upgrade/uninstall.
- [ ] Completion matches CLI.
- [ ] Release automation fails closed.

## Architecture
- [ ] One canonical implementation/domain owner exists.
- [ ] Existing contracts are reused or explicitly extended.
- [ ] No duplicate business-logic path exists.
- [ ] Interface adapters preserve identical semantics.
- [ ] Lifecycle and restart behavior are explicit.

## Security
- [ ] Authorization is enforced at the execution boundary.
- [ ] Scope/identity mismatches are rejected.
- [ ] Inputs and resources are bounded.
- [ ] Secrets are redacted.
- [ ] Consequential failures fail closed where required.

## Persistence/recovery
- [ ] One authoritative persistence owner exists.
- [ ] Required state survives restart.
- [ ] Corruption/partial publication is detected.
- [ ] Concurrency behavior is deterministic.
- [ ] Recovery semantics are tested.

## Testing
- [ ] Unit tests pass.
- [ ] Integration tests pass.
- [ ] Adversarial tests pass.
- [ ] Failure/recovery tests pass.
- [ ] Applicable real-interface tests pass.
- [ ] Repository verification gates pass.

## Documentation
- [ ] Feature contract is documented.
- [ ] Runtime behavior is not overstated.
- [ ] Known limitations/evidence gaps are reported.

# 11. Explicit non-goals

- No package manager rewrite.
- No runtime security bypass.
- No unsupported target claims.

Do not expand into model routing, autonomous orchestration, generic DAG scheduling, or unrelated infrastructure.

# 12. Final implementation report

Report:
Feature:
Canonical implementation:
Existing contracts reused:
Files changed:
Interfaces added/changed:
Persistence/migrations:
Security controls:
Audit/observability:
Tests added:
Real-interface evidence:
Verification results:
Known limitations:

## Standalone execution rule
A developer must be able to read this prompt, inspect current rust, identify existing implementations, implement only this feature's missing/incomplete contract, test it, and verify acceptance without another prompt, PR, or undocumented assumption.
