# TW-001 — AWH Init + Runtime Contracts

## Master implementation prompt
Implement this task against the current rust branch. It is standalone and must not require another GitHub issue or PR.

First inspect src/main.rs, configuration, workspace, persistence, error handling, existing agent models, and all existing ID types. Read the current roadmap/docs before changing code. Reuse equivalent abstractions; do not create duplicate identity or storage systems.

Implement or reconcile awh init so a fresh workspace can be initialized safely and repeatedly. It must be idempotent, preserve existing state/configuration, reject invalid roots, use secure defaults, avoid implicit agent activation, and avoid unrestricted capability grants.

Establish stable typed IDs/contracts for agent, session, workspace, task, edit, snapshot, and audit records, or extend equivalent existing types. Serialization must be deterministic and reload must preserve identity.

Likely touch points: src/main.rs, src/lib.rs, src/core/agents.rs, src/core/capability_grants.rs, src/core/policy.rs, configuration/storage modules, and a focused init or identity module only when necessary.

Tests must cover fresh init, repeated init, preservation of config, invalid workspace, secure defaults, no implicit active agent, ID round trips, identity relationship validation, and restart/reload.

Run cargo fmt --all -- --check, cargo check --all-targets, cargo test --all-targets, plus real CLI smoke tests in a temporary workspace.

Do not implement unrelated agent routing, worktrees, collaboration, remote infrastructure, or speculative abstractions.

Definition of done: fresh AWH initialization works safely and the identity contracts persist/reload without destructive changes or implicit authority.
