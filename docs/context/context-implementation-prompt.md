# 18 — Context Engine (CTX-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 18 / CTX-001  
**Feature:** Context Engine  
**Primary roadmap area:** Context engine/runtime context management  
**Target branch:** rust  
**Canonical folder:** docs/context/

### Mission
Implement, converge, and verify Context Engine inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Complete and converge the existing src/context/* subsystem.
- Own scoped context items, budgets, deterministic scoring/selection, compression, offloading/restoration, context snapshots and policy decisions.
- Provide the final awh context show|save|update|clear|search contract and existing MCP context adapter.
- Keep source files read-only and consume the existing memory authority.

## Out of scope
- No LLM inference, model routing or prompt-generation engine.
- No second memory store/vector database.
- No source-file mutation.
- No replacement PolicyEngine, AuditLog, SnapshotStore or FilesService.

## Product boundary
AWH owns context state and context optimization. External agents consume assembled context and remain responsible for reasoning. Files remain owned by FilesService; durable developer memory remains owned by the memory subsystem.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
src/context/mod.rs, engine.rs, budget.rs, item.rs, selector.rs, scoring.rs, policy.rs, compressor.rs, offload.rs, snapshot.rs, tokens.rs; src/mcp/context_engine.rs; CLI context handlers; tests; .agent/context-engine persistence.

Search before adding abstractions:
rg -n "ContextEngine|ContextItem|ContextBudget|OffloadStore|ContextSnapshot|context_engine|awh context|memory_enabled"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
ContextEngine → request/scope validation → budget/scoring/policy → active/offloaded/snapshot state → result. Keep active context, offloaded context, source files and long-term memory as distinct domains.

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
Uninitialized → Ready → Active → Optimized/Offloaded → Snapshot/Restore → Cleared. Reject invalid IDs, cross-scope restores, over-budget items and source-file writes. Restart reloads durable offloads/snapshots only according to their contract.

# 4. Interfaces

## Rust/service interface
Keep ContextEngine, ContextRequest, ContextItem, ContextScope, ContextBudget, ContextDecision, OffloadStore and ContextSnapshot stable. Errors must distinguish scope, budget, missing state, corruption and disabled-engine failures.

## CLI interface
awh context show|save|update|clear|search; validate scope/query/task/budget, bound output, make clear explicit and scope-bound.

## Control API interface
If exposed, Control API delegates to ContextEngine with scoped auth, limits and pagination; no API-local context state.

## MCP interface
Existing MCP context tools delegate to ContextEngine; tool names never grant scope; preserve session/workspace identity.

## TUI interface
TUI context views/actions call ContextEngine; no TUI-local scoring, persistence or memory.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Require workspace/project and caller identity; mutating/restore/clear operations use existing capability/policy checks; internal callers cannot silently bypass authorization.

## Input/resource safety
Validate item IDs, scope IDs, token budgets, snapshot IDs, query/task sizes; bound active items/content/search results; reject unsafe persisted paths.

## Isolation
Bind context to workspace/project and agent/session/task scope where represented. Cross-scope reads/restores require explicit policy.

## Secrets and sensitive data
Treat context as potentially sensitive. Do not log raw content or secret values; audit identifiers/outcomes with redaction.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Use existing .agent/context-engine storage. Define version/integrity/atomic publication/reload. Corrupt offload/snapshot data must not become silently altered context.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | Context Engine responsibility |
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
Test token/budget arithmetic, scope matching, deterministic selection, protected items, compression, snapshot serialization and Unicode/empty/max-size content.

## Integration tests
Real workspace insert → optimize → offload → restore; restart/reload; verify memory is read through existing authority and source files are never changed.

## Security/adversarial tests
Cross-workspace/agent access, corrupted state, oversized items, path traversal, MCP route-as-auth attempts, concurrent optimize/restore.

## Failure/recovery tests
Interrupted publication, corrupt blob/metadata, missing item, failed restore, restart after partial write, repeated clear/restore, concurrent optimization.

## Compatibility/real-interface tests
CLI, MCP, TUI/API where present, existing .agent/context-engine data and memory integration.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Preserve existing context schemas where possible; version incompatible changes and never silently discard recoverable context.

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
If integrity, scope or budget cannot be guaranteed, reject the operation. Context failure must not mutate source files or silently lose recoverable offloads.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] ContextEngine is the single context implementation.
- [ ] Budget/scope enforcement is deterministic.
- [ ] Offload/restore and snapshots are durable/integrity checked.
- [ ] Context consumes the existing memory authority.
- [ ] awh context and MCP use the canonical engine.
- [ ] Context optimization never rewrites source files.

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

- No model/provider integration.
- No vector database.
- No second memory system.
- No source editor or generic backup system.

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
