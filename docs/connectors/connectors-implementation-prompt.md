# 27 — Connectors (CON-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 27 / CON-001  
**Feature:** Connectors  
**Primary roadmap area:** External connector/provider integration  
**Target branch:** rust  
**Canonical folder:** docs/connectors/

### Mission
Implement, converge, and verify Connectors inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Converge connector registration/discovery/invocation around ConnectorProvider/ProviderRegistry.
- Implement awh connector list|add|remove|inspect|test|invoke where current contract supports it.
- Integrate provider/custom-MCP tools with schema validation, policy, auth, secrets and audit.
- Keep Composio/provider-specific code behind the generic provider abstraction.
- Bound external calls.

## Out of scope
- No provider-specific security system.
- No dynamic tool trust.
- No secret leakage.
- No workflow engine.

## Product boundary
AWH owns connector lifecycle, provider abstraction, policy boundary and invocation bookkeeping. External providers own API semantics.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
src/mcp/providers.rs, connectors.rs, composio.rs, custom MCP clients, schema.rs, permissions/audit, CLI/API/TUI.

Search before adding abstractions:
rg -n "ConnectorProvider|ProviderRegistry|connector.invoke|connector list|Composio|CustomMcpProvider|input_schema"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
Connector request → provider resolution → identity → policy → schema validation → bounded external call → normalized result → audit. Malformed dynamic tools are not exposed.

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
Configured → Registered → Discoverable → Enabled/Disabled → Invoked → Removed. Provider config changes are atomic; removing a provider does not delete unrelated credentials.

# 4. Interfaces

## Rust/service interface
Preserve ConnectorProvider, ToolDescriptor, ToolCallResult and ProviderRegistry concepts. Typed errors distinguish unconfigured provider, schema, auth, network, timeout and provider failure.

## CLI interface
awh connector list|add|remove|inspect|test|invoke; explicit provider/tool, bounded JSON args; test is non-destructive by default.

## Control API interface
API uses the same registry/invocation service and scoped auth.

## MCP interface
MCP-exposed connector tools use existing schema and policy checks; provider.tool is an identifier, not a permission.

## TUI interface
TUI connector views call canonical registry and never persist credentials.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Registration/removal/invocation require appropriate capabilities. Provider credentials do not authorize the AWH caller.

## Input/resource safety
Validate provider/tool IDs, JSON schema, args/response sizes, provider URLs, timeouts and retries.

## Isolation
Provider configuration and connected accounts are scoped; cross-agent account access requires policy.

## Secrets and sensitive data
API keys/tokens are references/secret material, never logs/listings/audit. Redact provider error payloads.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Use existing provider registry persistence. Version config and atomically add/remove/update; credentials remain in secret authority.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | Connectors responsibility |
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
Test registry, qualified names, schema validation, normalization and error mapping.

## Integration tests
Register harmless provider/tool, list/invoke/remove, restart registry; real Composio/custom MCP where credentials are available.

## Security/adversarial tests
Malformed dynamic schema, unauthorized invoke, provider/tool confusion, credential leakage, oversized payload, malicious response.

## Failure/recovery tests
Provider timeout/unavailable, malformed JSON, auth denial, registry corruption, failed removal.

## Compatibility/real-interface tests
ProviderRegistry/ConnectorProvider, Composio/custom MCP, schema validator, audit and interfaces.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Preserve provider IDs and qualified naming where possible; configuration migrations are explicit.

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
Untrusted metadata is rejected before exposure; provider errors do not grant fallback authority.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] One canonical provider registry.
- [ ] Dynamic schemas validated.
- [ ] Every invocation policy checked.
- [ ] Credentials redacted.
- [ ] Restart-safe state.
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

- No provider-specific security boundary.
- No automatic trust.
- No unrestricted network.

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
