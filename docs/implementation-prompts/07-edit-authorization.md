# Prompt 07 — Edit Caller Authorization (AWE-011 / TW-004)

## Mission
Ensure every consequential edit and edit-level rollback crosses one authoritative caller/capability/policy decision before mutation. This prompt is standalone.

## Required behavior
- Resolve caller identity from trusted runtime state.
- Bind caller to agent/session/workspace context.
- Evaluate capability and policy against the concrete operation and target.
- Default deny when authority is absent or ambiguous.
- Perform authorization before consequential filesystem mutation.
- Apply the same gate regardless of entry transport.
- Return structured denial without leaking sensitive content.
- Correlate decisions through existing audit/provenance facilities when available.
- Do not treat AgentProfile declarations, MCP routes, tool descriptions, or configuration alone as authority.

## Forensics
Trace every edit and rollback entry point to the canonical service and identify bypass paths or duplicate policy checks.

## Tests
Cover authorized/denied operations, path-scoped grants, wrong identity/workspace, disabled session, MCP/CLI entry paths, rollback denial, and zero mutation on denial.

## Non-goals
No second policy engine or identity redesign.

## Final report
Show the single decision boundary, bypasses removed, denial semantics, tests, and compatibility impact.
