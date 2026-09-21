# Prompt 02 — Agent Runtime Identity (TW-002 / AGENT-001)

## Mission
Implement the AWH runtime identity boundary for external agents: AgentProfile, AgentRegistry, AgentSession, caller identity, workspace binding, lifecycle state, and identity-safe discovery. This prompt is standalone.

## Architectural boundary
AWH is agent-agnostic. External agents provide reasoning/planning; AWH provides controlled identity, workspace state, capabilities, tools, and observability. Do not build LLM reasoning, autonomous planning, workflow orchestration, or a model router.

## Required behavior
- AgentProfile describes an agent and declared capabilities; it does not grant authority.
- AgentRegistry owns lookup/lifecycle state; membership is not authorization.
- AgentSession binds a runtime caller to one agent and workspace.
- Resolve caller → agent → session → workspace deterministically.
- Reject unknown, disabled, expired, or mismatched identities.
- Preserve isolation for simultaneous agents.
- Keep MCP transport identity distinct from AWH AgentSession identity.
- Correlate agent/session identity with tasks/edits/audit without storing secrets.
- Reuse existing identity/registry types instead of creating parallel systems.

## Forensics
Inspect agent, identity, config, session, workspace, capability, MCP, CLI, persistence, and audit modules.

## Tests
Cover registration/lookup, duplicate IDs, disabled agents, session expiry, workspace mismatch, simultaneous agents, restart persistence, serialization, and identity substitution.

## Security
Identity is an input to authorization, not authorization itself. Never infer authority from names, routes, URLs, tool descriptions, or profile metadata.

## Non-goals
No LLM execution, scheduler, model provider, generic workflow engine, or external-agent runtime.

## Final report
Document the identity graph, lifecycle, persistence, security boundary, tests, verification, and limitations.
