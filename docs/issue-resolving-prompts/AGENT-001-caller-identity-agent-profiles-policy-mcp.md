# AGENT-001 — Caller Identity, Agent Profiles, Policy-Routed MCP

## Task
Connect persisted agents/capabilities to actual runtime identity and tool authorization.

## Forensic baseline
Agent records and capability grants are currently bookkeeping only; protocol sessions do not carry agent identity; audit lacks identity.

## Architecture
```text
TOML
→ Config/AgentProfile
→ AgentRegistry
→ AgentSession
→ PolicyEngine/Capability
→ Tool Registry
→ canonical service
```

## Routing
`/{agent}/mcp` and `/{agent}/sse` may identify an enabled agent, but route names are never authorization.

## CLI
`awh agent list/show/start/stop/restart/run/status`.
`awh agent start claude` activates only Claude; `--all` activates all enabled profiles.

## Requirements
- identity propagated into tool calls and audit
- grants and expiry enforced
- agent-specific discovery and invocation
- inactive route rejection
- same authorization engine for every agent
- simultaneous-agent tests

## Do not build
LLM orchestration, agent reasoning, workflow planning, or a model router.
