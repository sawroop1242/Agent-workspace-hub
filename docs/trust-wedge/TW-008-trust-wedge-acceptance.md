# TW-008 — Complete Trust-Wedge Acceptance Suite

## Master implementation prompt
Build executable integration tests for the Trust Wedge. This issue is standalone and must not depend on another issue.

Canonical scenario: awh init -> agent/session -> agent-scoped MCP -> discovery -> authorization -> read -> safe edit -> expected-state validation -> snapshot -> apply -> verify -> provenance/audit -> rollback -> verify -> restart -> reconstruct.

Prefer tests/trust_wedge with focused files for init, agent runtime, routing, authorization, edit, snapshot, rollback, audit, and full flow. Avoid duplicating service unit tests.

Mandatory happy path: fresh workspace, agent/session, agent route, authorized read, authorized edit, snapshot/provenance, audit, rollback to exact original bytes.

Mandatory denial: unknown agent, inactive agent, missing/expired capability, policy denial, wrong workspace/scope. Every denied mutation must prove zero mutation.

Mandatory recovery: stale state, verification failure/failure injection where supported, external modification before rollback, non-destructive rollback conflict, process restart persistence.

Mandatory multi-agent scenario: at least two agents/sessions with distinct identity and no cross-agent authorization.

Use real temporary files and production services. Do not replace missing implementation with mocks or weaken assertions.

Run cargo fmt --all -- --check, cargo check --all-targets, cargo test --all-targets, and real CLI/MCP smoke tests where supported.

Definition of done: executable integration tests demonstrate identity, routing, authorization, safe edit, snapshots, rollback, persistence, restart, and audit reconstruction.
