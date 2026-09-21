# TW-002 — AgentProfile + AgentRegistry + AgentSession

## Master implementation prompt

Implement the AWH agent runtime identity model as a standalone Trust-Wedge slice. This issue must be executable against the current `rust` branch without requiring another unmerged TW issue or PR.

### 1. Repository forensics

Before editing code, inspect the current implementation and tests for:

- `src/core/agents.rs` and related domain models/services.
- `src/main.rs`, CLI command wiring, configuration, persistence, and storage.
- Existing capability and policy types.
- Existing MCP/transport protocol-session code.
- Existing workspace/project/runtime identity types.
- Existing agent-related commands, tests, docs, and security/error conventions.

Search for existing `AgentProfile`, `AgentRegistry`, `AgentSession`, agent IDs, session IDs, lifecycle enums, and stores before creating anything. Reuse authoritative abstractions instead of creating duplicates. Record compatibility constraints from the current repository.

### 2. AgentProfile contract

`AgentProfile` describes an agent; it is not an authorization grant.

It should represent, using existing domain types where available:

- stable agent ID;
- human-readable name;
- agent kind/type;
- enabled/disabled state;
- launch or configuration metadata when the existing product model supports it;
- route metadata when applicable;
- references to policy/capability configuration without treating those references as authority.

Requirements:

- IDs are stable and validated.
- Profile serialization is deterministic and restart-safe.
- Unknown/invalid fields or malformed authoritative state are handled explicitly.
- Profile metadata cannot itself grant capabilities.
- Secrets are not persisted or displayed through ordinary profile inspection unless the existing security contract explicitly requires a protected reference.

### 3. AgentRegistry contract

The registry is the authoritative agent lifecycle/lookup boundary.

Support the operations that the current architecture needs:

- register;
- lookup/get;
- list;
- activate/enable;
- deactivate/disable;
- safe removal/de-registration where supported.

Enforce:

- duplicate IDs are rejected rather than overwritten;
- unknown agents cannot be activated, deactivated, or used;
- disabled agents cannot create new active sessions;
- removing an agent must not silently orphan or mutate unrelated agents;
- registry persistence uses the repository's existing storage/locking conventions;
- concurrent updates cannot silently lose an agent record.

Do not make the registry an authorization engine. Authorization remains a separate capability/policy decision.

### 4. AgentSession contract

`AgentSession` is an AWH runtime identity and must remain distinct from an MCP protocol/transport session.

Use existing identity types where available and include the minimum required relationship:

- stable session ID;
- exactly one agent ID;
- exactly one workspace ID;
- lifecycle state;
- creation/update timestamps;
- termination/failure information where applicable;
- task/correlation information only when already supported by the architecture.

Define explicit lifecycle transitions. At minimum, prevent:

- session creation for an unknown or inactive agent;
- operations through a terminated/stopped session;
- invalid state transitions;
- cross-workspace session reuse;
- changing a session's owning agent after creation unless the repository already has an explicit transfer contract.

A session establishes identity context; it does not by itself grant capability authority.

### 5. Identity and isolation invariants

Preserve the Trust-Wedge chain:

`Workspace -> AgentProfile -> AgentRegistry -> AgentSession -> authorization context`

The implementation must guarantee:

- two agents remain identity-isolated;
- one agent cannot use another agent's session;
- agent identity cannot be substituted by a route name;
- profile metadata cannot bypass policy/capability checks;
- disabled/inactive lifecycle state is enforced before consequential operations;
- persistent IDs survive process restart where the product contract requires durability;
- identity records cannot silently downgrade to a fresh state after corruption.

### 6. CLI and runtime integration

Reconcile the existing CLI rather than inventing a parallel interface. Where supported, provide honest behavior for:

- `awh agent list`
- `awh agent inspect/show <id>`
- `awh agent start/enable <id>`
- `awh agent stop/disable <id>`
- `awh agent restart <id>`
- `awh agent status <id>`

Do not claim to start an external OS process if AWH does not own process supervision. Return structured state/errors that accurately describe what AWH controls.

The same domain services should be usable by CLI and later MCP/control-plane adapters.

### 7. Persistence, restart, and failure behavior

Use the existing persistence abstraction where possible. Cover:

- fresh registry initialization;
- repeated loading;
- restart/reload;
- malformed records;
- missing records;
- duplicate records;
- interrupted/partial persistence where relevant;
- concurrent registry/session changes;
- disabled agent with existing sessions;
- terminated session after restart.

Do not silently convert malformed authoritative state into an empty registry.

### 8. Tests

Add focused tests for:

- profile validation and round trip;
- duplicate registration;
- registry lookup/list;
- enable/disable lifecycle;
- unknown-agent handling;
- session creation;
- invalid lifecycle transitions;
- disabled-agent session rejection;
- stopped-session rejection;
- workspace/agent identity binding;
- two-agent isolation;
- persistence and reload/restart;
- CLI smoke behavior where supported.

For security-sensitive failures, assert the protected state did not change—not merely that an error was returned.

Use real persistence and temporary workspaces for integration tests; avoid mocks that bypass the identity/storage invariants being tested.

### 9. Security gates

Before completion verify:

- no implicit authorization is derived from profile metadata;
- no unrestricted capability is granted by default;
- caller/agent/session/workspace identities are not conflated;
- unknown or inactive identities fail closed;
- secrets are not exposed in logs or ordinary status output;
- path/workspace boundaries remain enforced by downstream authorization;
- no second AgentStore or second session abstraction was introduced.

### 10. Definition of done

This issue is complete only when:

- the implementation is wired into the real runtime/CLI paths;
- identity and lifecycle invariants are executable in tests;
- persistence/restart behavior is covered;
- negative/denial cases are covered;
- relevant repository checks pass;
- documentation reflects actual behavior;
- the diff contains no unrelated Trust-Wedge or architectural refactor.

Final report must state changed files, identity/lifecycle decisions, tests added, commands run, results, and any explicitly remaining limitation.

### Non-goals

Do not implement the full agent-specific MCP authorization flow, capability issuance/redesign, edit authorization, snapshot storage, rollback, persistent audit, remote execution, external-agent orchestration, or unrelated refactors. Those concerns may consume this identity model later but are not prerequisites for completing this issue.