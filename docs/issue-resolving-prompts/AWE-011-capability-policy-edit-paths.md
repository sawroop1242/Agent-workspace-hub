# AWE-011 / #32 — Capability and Policy Enforcement

## Task
Make every edit and rollback path pass one authoritative capability/policy boundary.

## Forensic baseline
AWH has real external-MCP trust and built-in gates plus a narrow DENY-only policy, but agent capability grants are currently inert and built-in Medium/High tools default to allow without a trust record.

## Required path
`Caller/Agent → Session → Workspace → Capability → Resource → Policy → EditService`.

## Requirements
- one policy engine
- one capability decision path
- MCP and CLI equivalence
- path/resource scoping
- grant expiry enforcement
- denied mutation = zero side effects
- route/URL namespace is identity/routing, never authorization
- rollback is authorized too

## Agent profile compatibility
Prepare the boundary for `TOML → AgentProfile → AgentRegistry → AgentSession → PolicyEngine`. Do not embed TOML parsing inside the policy engine.

## Tests
Least-privilege denial, expired grant, wrong agent, wrong workspace, policy denial, rollback denial, and equivalent MCP/CLI behavior.
