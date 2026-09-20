# TW-003 — Agent-Specific MCP Routing + Authorization

## Master implementation prompt
Implement standalone agent-scoped MCP routing and one authorization path.

Inspect src/mcp/server.rs, src/mcp/sse.rs, src/mcp/tool_broker.rs, src/mcp/execution_gate.rs, src/services/authorization.rs, src/core/capability_grants.rs, src/core/policy.rs, and current agent/session code.

Support agent-scoped routes such as /{agent}/mcp and /{agent}/sse. The route identifies routing only; it never grants authorization.

Required flow: transport -> resolve agent -> resolve active session -> capability context -> policy decision -> tool broker -> tool.

Reject unknown/inactive agents. Reject missing, expired, or scope-mismatched capabilities before tool execution. Policy denial overrides grants. A denied request must never invoke the underlying tool. Caller identity must remain available to audit/observability. Two agents must not be confused.

Extend the existing broker/execution gate. Add agent_router or capability_context only if an equivalent does not exist.

Tests: route parsing, unknown/inactive agent, active session resolution, grant allow/deny, expiry, scope mismatch, policy override, route-name-is-not-authorization, active tools/list, authorized tool call, denied call proving the tool was not invoked, and two-agent isolation.

Do not hard-code Claude/Codex/OpenCode permissions and do not create a second security gate.
