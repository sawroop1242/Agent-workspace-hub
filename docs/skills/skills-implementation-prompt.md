# 20 — Skills (SKL-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 20 / SKL-001  
**Feature:** Skills  
**Primary roadmap area:** Skills and capability packages  
**Target branch:** rust  
**Canonical folder:** docs/skills/

### Mission
Implement, converge, and verify Skills inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Make src/skills/* the canonical skill lifecycle.
- Complete list/show/install/remove/enable/disable, registry/lock behavior and package validation.
- Validate SKILL.md/manifests, references, package paths and declared capabilities.
- Integrate install/activation with PolicyEngine, FilesService, MCP/tool registry and audit.
- Harden GitHub/community remote sources.

## Out of scope
- No arbitrary plugin runtime.
- No automatic capability grant from a manifest.
- No replacement MCP/tool security boundary.
- No unrestricted remote Git execution.

## Product boundary
AWH owns skill metadata, package lifecycle, integrity and declared requirements. PolicyEngine decides whether capabilities are actually granted.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
src/skills/mod.rs, registry, installer, parser, package, references, project, lockfile, remote, registry client; src/mcp/skills.rs; capability/policy/audit; CLI.

Search before adding abstractions:
rg -n "Skill|SkillRegistry|GlobalSkillRegistry|SkillInstaller|SkillLock|SKILL.md|awh skill|capability.*skill"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
Source → parse/validate → integrity → transactional install → registry/lock → enable/disable → policy/capability evaluation → tool exposure/execution.

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
Discovered → Validated → Installed → Enabled/Disabled → Removed. Failed installs leave no partially trusted skill. Removal is restricted to skill-owned paths.

# 4. Interfaces

## Rust/service interface
Preserve existing skill parser/registry types; typed errors distinguish malformed manifest, duplicate/version conflict, integrity failure, unsupported requirement, path escape, remote failure and policy denial.

## CLI interface
awh skill list|show|install|remove|enable|disable; show installed vs enabled; install supports established source/ref/version semantics; output must not expose secrets.

## Control API interface
If exposed, API delegates to registry/installer; remote installation is bounded and authenticated as configured.

## MCP interface
MCP skill surface reads canonical registry; skill-provided tools still pass normal schema/policy checks.

## TUI interface
TUI skill browser/install/enable uses canonical service; display declared capabilities as requested, not granted.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Install/remove/enable/disable require appropriate capability/policy. A skill cannot grant itself authority; runtime tool actions are separately authorized.

## Input/resource safety
Validate manifest names/versions/paths, refs/URLs, package size, symlinks and extraction paths; bound registry/download responses.

## Isolation
Project/workspace skill state is isolated. Global skills, if supported, are explicit. Installed files stay under canonical skill root.

## Secrets and sensitive data
Never persist API keys in manifests/locks/logs. Use existing secret references and redaction.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Use existing registry/lock/project state. Transaction: stage → validate → publish. Detect corruption and make repeated installs deterministic.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | Skills responsibility |
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
Test parsing, versioning, references, capability declarations, lock serialization, path safety and package integrity.

## Integration tests
Real local install; remote source validation where available; restart registry/lock reload; enable then verify tool use still hits policy.

## Security/adversarial tests
Malicious paths/symlinks, oversized packages, bad URLs/refs, self-grant attempts, secret leakage, cross-scope install.

## Failure/recovery tests
Interrupted install, clone/download failure, corrupt lock, registry write failure, repeated enable/remove.

## Compatibility/real-interface tests
Existing SKILL.md packages, CLI/MCP/TUI, registry/lock data and remote syntax.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Preserve established manifest semantics; newly installed skills are not automatically enabled unless the existing contract says so.

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
Integrity/policy/install failure preserves prior trusted state. Remote failure cannot silently become an untrusted local interpretation.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] One canonical skill registry/package lifecycle.
- [ ] Durable scope-safe install/remove/enable/disable.
- [ ] Manifest cannot self-grant.
- [ ] Local/remote paths are validated.
- [ ] Registry/lock survive restart.
- [ ] Interfaces converge.

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

- No arbitrary code execution framework.
- No automatic capability grants.
- No second package/tool registry.

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
