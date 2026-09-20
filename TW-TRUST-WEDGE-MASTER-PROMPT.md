# Trust Wedge — Master Implementation Prompt

Use this prompt whenever an agent is asked to implement a Trust-Wedge issue in Agent Workspace Hub.

## Mission

Build a trustworthy runtime boundary around AWH initialization, agent identity, agent session, agent-specific MCP routing, capability/policy enforcement, safe editing, durable file snapshots/provenance, conflict-aware rollback, persistent audit, and executable acceptance.

Every Trust-Wedge issue is standalone. Never require another GitHub issue, branch, or PR to be merged before implementing the requested scope. If a prerequisite abstraction is absent, implement the smallest compatible local abstraction needed and keep the issue self-contained.

## Architecture rules

1. One canonical service layer is shared by MCP, CLI, TUI, and Control API.
2. AgentProfile describes an agent; it does not authorize.
3. AgentRegistry is the authoritative agent lookup/lifecycle registry.
4. AgentSession is an AWH runtime caller, distinct from MCP protocol sessions.
5. Agent-specific MCP route names identify routing only, never authority.
6. Authorization happens before tool execution or filesystem mutation.
7. EditService remains the canonical edit engine.
8. Context snapshots and file snapshots are separate.
9. Rollback is conflict-aware and never silently destroys external changes.
10. Audit remains one canonical choke point.
11. Secrets and raw file contents are not written to normal audit records.
12. Security authority defaults to deny unless explicitly granted.
13. Features require executable tests, not just types or documentation.
14. Do not add unrelated roadmap scope.

## Before coding

Inspect the exact current repository files named by the issue. Search for existing types/services and extend them where possible. Read relevant current docs and tests. Record current behavior before modifying it.

## Implementation loop

Inspect -> design smallest change -> implement -> unit tests -> real integration tests -> format/check/test -> CLI/MCP smoke validation -> update docs to match actual behavior -> report exact evidence.

## Safety invariants

Authorization denial => zero mutation.

Invalid/stale expected state => zero mutation.

Successful edit + safe rollback => original bytes restored and verified.

External modification + rollback => conflict and external bytes preserved.

Consequential runtime identity should be traceable as agent -> session -> workspace, and edit -> snapshot -> audit correlation where applicable.

Durable state claimed by the implementation must survive restart.

## Validation

At minimum:

cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets

Also run targeted tests and existing CI/lint commands when available.

## Completion standard

Do not mark complete without production behavior, tests, security invariants, compatibility validation, accurate docs, no duplicate subsystem, and no hidden issue dependency.

## Out of scope

Do not pull broad collaboration, enterprise RBAC, advanced remote sandboxes, unrelated ecosystem integrations, large store migrations, performance rewrites, or speculative infrastructure into a Trust-Wedge issue unless explicitly requested.

## Final report

List exact production files changed, test files changed, commands executed, results, limitations, and intentionally unsupported behavior. Never claim an acceptance criterion passed without executable evidence.
