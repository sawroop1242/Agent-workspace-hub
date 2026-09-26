# 19 — Memory (MEM-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 19 / MEM-001  
**Feature:** Memory  
**Primary roadmap area:** Developer-oriented persistent memory  
**Target branch:** rust  
**Canonical folder:** docs/memory/

### Mission
Implement, converge, and verify Memory inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Converge memory on one canonical service/store.
- Preserve and version the established .agent/memory.jsonl compatibility where it is authoritative.
- Implement list/get/search/add/update/delete with explicit scope and bounded queries.
- Integrate context, MCP, CLI, TUI and API through one memory authority.

## Out of scope
- No vector database or semantic model service.
- No secret vault.
- No cross-workspace access by default.
- No second memory store.

## Product boundary
AWH owns durable developer workflow memory. Agents consume memory; AWH does not perform agent reasoning or autonomous summarization.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
src/core/memory.rs, src/mcp/memory.rs, src/models/memory.rs, context integration, CLI memory handlers, tests, .agent/memory.jsonl fixtures.

Search before adding abstractions:
rg -n "MemoryStore|MemoryEntry|MemoryMcp|memory.jsonl|awh memory|memory.search|memory.add"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
Caller → scope/auth → canonical MemoryService/Store → validated record → bounded query/result → audit. Compatibility adapters may remain, but all reads/writes converge on one authority.

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
Missing → Created → Active → Updated/Deleted. Invalid scope/ID and stale update are rejected. Restart reproduces durable records.

# 4. Interfaces

## Rust/service interface
Define stable record/service APIs and typed errors for invalid ID, scope, malformed record, missing record and persistence failure. Extend legacy records with versioned fields rather than duplicating models.

## CLI interface
awh memory list|get|search|add|update|delete; explicit scope for ambiguous writes, bounded content/query/limits and safe delete semantics.

## Control API interface
If exposed, Control API delegates to the same store with scoped authentication and bounded pagination.

## MCP interface
Existing MCP memory tools delegate to the canonical store and carry agent/session identity.

## TUI interface
TUI memory views use canonical list/search/add/update/delete; no local JSONL writer.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Reads/writes/deletes are scope checked; context access uses the same scope; shared/global memory is explicit and policy-controlled.

## Input/resource safety
Bound content/query/metadata/result size; validate IDs and scopes; never interpret memory identifiers as arbitrary filesystem paths.

## Isolation
At minimum isolate by workspace/project and apply agent/session/task scope when modeled. Cross-scope access requires policy.

## Secrets and sensitive data
Memory may contain sensitive user content. Redact secrets from audit/logs and never add credential expansion.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Choose one durable format/owner. For JSONL define append/update/delete strategy, schema version, corruption behavior and atomic rewrite rules. Test restart.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | Memory responsibility |
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
Test ID/scope matching, filtering, pagination, serialization, legacy compatibility and malformed-line handling.

## Integration tests
Real add → restart → get/search; update/delete → restart; context reads same store; concurrent operations.

## Security/adversarial tests
Cross-scope access, oversized content, malformed IDs, secret leakage, concurrent update/delete.

## Failure/recovery tests
Truncated/corrupt JSONL, disk failure, partial rewrite, restart after interruption, repeated delete/update.

## Compatibility/real-interface tests
CLI, existing MCP, context engine and persisted JSONL data.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Preserve readable old records or provide explicit idempotent migration. Never silently change their scope.

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
Corrupt records are not silently converted. Failed updates/deletes do not report success.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] One canonical memory service/store.
- [ ] Existing JSONL is readable or explicitly migrated.
- [ ] Scope and CRUD/search are deterministic and bounded.
- [ ] Restart/corruption behavior is tested.
- [ ] All interfaces converge on one authority.

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

- No vector DB.
- No LLM memory summarizer.
- No secret manager.
- No interface-local memory store.

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
