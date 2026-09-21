# Prompt 11 — MCP Editing + Real Client Validation (AWE-009 / AWE-016)

## Mission
Expose the canonical editing service through MCP and validate the real MCP protocol boundary with a real client. This prompt is standalone.

## Required behavior
- MCP tools are thin adapters over canonical AWH services.
- Preserve caller identity, capability/policy checks, workspace containment, expected-state validation, snapshots, verification, provenance, rollback, and audit semantics.
- Never implement a second MCP-specific editor.
- Tool schemas reject malformed requests deterministically.
- Error responses preserve structured service semantics without leaking secrets.
- Exercise actual MCP transport/protocol behavior, not only direct function calls.
- Validate discovery, invocation, denial, malformed input, stale state, mutation, verification, and rollback.

## Forensics
Inspect MCP server, transport, tool registry, service wiring, authentication/session mapping, and tests. Remove divergent tool implementations rather than wrapping wrappers.

## Tests
Use an actual MCP client/transport where supported. Cover happy path, denied high-risk edit, wrong workspace/session, malformed operation, stale state, multi-file edit, verification failure, and rollback.

## Non-goals
No model routing, agent planner, or MCP-only domain logic.

## Final report
Provide protocol-level evidence, service mapping, security behavior, tests, and transport limitations.
