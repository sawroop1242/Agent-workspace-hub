# SEC-001 — High-Risk Built-ins Default Deny

## Task
Change built-in authorization so High-risk MCP tools require explicit authorization.

## Forensic baseline
PR #43 found `authorize_builtin_tool` defaults to allow when no `awh.builtin` record exists. This makes `terminal.run` unrestricted in the default deployment.

## Requirements
- Reuse existing trust/authorization machinery.
- High-risk: `terminal.run`, `connector.invoke`, and High-risk provider mutations must require explicit authorization.
- Preserve lower-risk compatibility only where intentionally documented.
- Unknown tools remain denied.
- Produce actionable missing-capability errors.
- Add migration/onboarding documentation.

## Tests
Default denial, explicit allow, least privilege, wrong permission, missing/corrupt store, unknown tool, and regression tests for existing low-risk tools.

## Rule
Do not add a second policy engine or special-case individual tools outside the registry/risk metadata model.
