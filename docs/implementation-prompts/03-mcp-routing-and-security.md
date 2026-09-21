# Prompt 03 — MCP Routing + Security Gates (TW-003 / SEC-001 / SEC-002)

## Mission
Implement one authoritative security boundary for agent-scoped MCP routing and high-risk MCP operations. This prompt is standalone.

## Required behavior
- Resolve trusted caller identity before consequential tool execution.
- Route agent-scoped MCP requests without treating route names as authority.
- Use one capability/policy decision point with default deny for high-risk operations.
- High-risk built-in tools require explicit authorization.
- Denials perform zero consequential mutation and return structured non-sensitive errors.
- Reject ambiguous, unknown, disabled, or mismatched caller identity.
- For non-loopback HTTP/SSE MCP binds, refuse plaintext operation unless valid TLS satisfies the repository security contract.
- Never expose bearer credentials over plaintext transport.
- Keep discovery separate from authorization; tool metadata is not permission.
- Preserve workspace containment and path-security checks.

## Forensics
Inspect MCP server/transport, authentication, capability/policy stores, tool registry, HTTP/SSE configuration, CLI configuration, and security tests. Trace every consequential invocation path.

## Tests
Cover authorized/denied high-risk tools, missing grants, wrong agent/workspace, disabled session, forged route identity, loopback/non-loopback bind, invalid/missing TLS, and zero-mutation denial.

## Non-goals
No second policy engine, model router, agent planner, or alternate MCP implementation.

## Final report
Record the authoritative decision path, bind/TLS behavior, denial evidence, tests, and residual boundaries.
