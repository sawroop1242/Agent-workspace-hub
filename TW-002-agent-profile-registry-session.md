# TW-002 — AgentProfile + AgentRegistry + AgentSession

## Master implementation prompt
Implement a standalone agent runtime model. Do not require another issue.

Inspect src/core/agents.rs, src/main.rs, capability/policy storage, configuration, persistence, and existing MCP protocol-session code.

AgentProfile must describe an agent, not authorize it. Include stable ID, name, kind, launch/config metadata when applicable, enabled state, route metadata, and policy/capability references.

AgentRegistry must be the authoritative registry for register, lookup, list, activate, deactivate, and safe removal.

AgentSession must be distinct from an MCP protocol session and contain session ID, agent ID, workspace ID, lifecycle state, timestamps, and termination/failure information where applicable. Use explicit lifecycle transitions.

Invariants: duplicate IDs rejected; unknown agents cannot create sessions; disabled agents cannot have active sessions; stopped sessions cannot act; each session belongs to exactly one agent/workspace; multiple agents remain identity-isolated; profile metadata cannot bypass authorization.

Reconcile CLI commands: awh agent list, inspect/show, start, stop, restart, status. Do not fake OS process management if unsupported; expose an honest structured state/error.

Tests: profile round trip/validation, duplicate registration, registry lookup/list, activation/deactivation, session creation, invalid transitions, disabled-agent rejection, stopped-session rejection, two-agent isolation, persistence/reload.

Run formatting, compilation, all tests, and CLI smoke tests.

Do not create a second AgentStore, second protocol-session abstraction, worktree system, or authorization bypass.
