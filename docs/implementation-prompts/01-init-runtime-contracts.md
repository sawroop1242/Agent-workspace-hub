# Prompt 01 — AWH Init + Runtime Bootstrap Contract (TW-001)

## Mission

Implement and verify the **workspace initialization and runtime bootstrap boundary** for Agent Workspace Hub (AWH).

This is the first prompt in the canonical `docs/implementation-prompts/` sequence, but it is **standalone**: do not require another prompt, another PR, or an assumed future implementation to make this prompt executable.

The Trust Wedge starts with one invariant:

> AWH must establish a durable, validated workspace boundary before any agent/session/tool authority can be attached to it.

The goal is not to build an agent orchestrator. The goal is to make `awh init` a reliable, restart-safe, fail-closed runtime bootstrap operation.

---

## Project context you must respect

Repository:

```text
https://github.com/sawroop1242/Agent-workspace-hub
```

Target branch:

```text
rust
```

AWH is a security-first Rust workspace runtime for AI agents. The runtime is agent-agnostic. External agents such as Claude Code, Codex, OpenCode, Qwen, or other executors provide reasoning; AWH provides the controlled workspace, policy boundary, files, tasks, context, skills, MCP access, Git operations, auditability, and recovery mechanisms.

The canonical product positioning in `docs/roadmap/GROWTH_STRATEGY.md` places `awh init` at the beginning of the **Trust Wedge**:

```text
Trust wedge:
Foundation -> snapshots/rollback

First user-facing foundation:
awh init
```

Do not add unrelated product scope merely because it appears in older planning documents.

---

## Required first action: repository forensics

Before modifying anything, inspect the **current** repository, not an assumed historical version.

Run, where available:

```bash
pwd
git status --short
git branch --show-current
git log --oneline -10
git remote -v
find . -maxdepth 3 -type f | sort
cargo metadata --no-deps
cargo check --all-targets
cargo test --all-targets
```

Inspect at minimum:

```text
Cargo.toml
README.md
AGENTS.md
docs/PROJECT_CONTEXT.md
docs/FEATURES.md
docs/architecture.md
docs/roadmap/PROJECT_ROADMAP.md
docs/roadmap/GROWTH_STRATEGY.md
docs/implementation-prompts/README.md
src/main.rs
src/core/
src/services/init.rs
src/core/identity.rs
src/core/workspace.rs
src/core/agents.rs
tests/init_cli.rs
```

Also inspect the current configuration/state and persistence implementation used by initialization.

Search for existing implementations before adding anything:

```bash
rg -n "initialize_workspace|awh init|workspace.json|WorkspaceId|AgentId|SessionId|TaskId|AuditEventId"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("
```

### Important preservation rule

The current Rust branch already contains TW-001 identity contracts and an initialization service. **Do not create duplicate identity types, duplicate initialization services, or a second workspace bootstrap path.**

Extend or correct the existing implementation only where evidence from the current repository shows that the contract is incomplete.

---

# 1. Canonical runtime boundary

A successful initialization must establish one durable workspace boundary.

Conceptually:

```text
selected filesystem root
        |
        v
validate root
        |
        v
canonical workspace root
        |
        v
.agent/workspace.json
        |
        v
minimum valid AWH state
```

The workspace identity is the durable root identity for later runtime operations.

The initialization operation must **not**:

- activate an agent;
- create an authorized agent session;
- grant capabilities;
- bypass policy;
- trust an MCP server;
- expose remote control;
- start an MCP server;
- execute arbitrary commands;
- create a worktree;
- modify unrelated project files;
- silently repair or replace corrupted state.

---

# 2. Workspace root validation

Validate the requested initialization root before mutating persistent state.

At minimum:

- accept an existing directory;
- create the requested directory only if current CLI semantics explicitly permit it;
- reject a regular file as a workspace root;
- resolve the canonical path used by the persisted manifest;
- prevent ambiguous relative/absolute root identity;
- reject invalid or unusable roots with a deterministic error;
- never initialize outside the intended selected root.

If the repository already has a secure path/root helper, reuse it.

Do not introduce a weaker path validation implementation merely to simplify `init`.

---

# 3. Durable workspace manifest

Use the repository's existing workspace manifest contract if present.

The manifest must contain enough information to:

- identify the workspace;
- identify the manifest/schema version;
- bind the manifest to the canonical workspace root;
- survive process restart;
- detect incompatible future versions;
- detect a manifest belonging to a different filesystem root.

The exact field names and serialization format must follow the current Rust implementation unless a demonstrated bug requires changing them.

### Fail-closed manifest behavior

If `.agent/workspace.json` exists:

1. parse it;
2. validate its schema/version;
3. validate its workspace identity;
4. validate its root binding;
5. preserve it byte-for-byte when it is already valid;
6. return the existing initialization result without generating a new identity.

Do **not** silently overwrite:

- malformed JSON;
- unsupported manifest versions;
- a manifest belonging to another root;
- partially initialized state that cannot be proven safe.

A corrupt state must produce an actionable error.

---

# 4. Idempotence

`awh init` must be safely repeatable.

For a valid already-initialized workspace:

```text
first init  -> Initialized
second init -> AlreadyInitialized / equivalent existing-state result
```

The second invocation must preserve:

- workspace identity;
- valid configuration;
- policy state;
- agent records;
- task state;
- memory/context;
- audit state;
- snapshots or other existing state;
- unrelated project files.

Do not regenerate IDs simply because initialization was invoked again.

Do not rewrite the manifest unnecessarily.

---

# 5. Minimum bootstrap state

Determine from the current repository which directories/files are actually required for an initialized AWH workspace.

Initialize only the minimum state required by the current runtime contract.

The bootstrap must be:

- deterministic;
- restart-safe;
- safe under repeated invocation;
- compatible with the current persistence layer;
- compatible with existing policy/security stores.

If an existing persistent store is present but malformed, initialization must fail closed rather than replacing it.

Do not manufacture future Trust Wedge state simply because later prompts may use it.

---

# 6. Identity contract

The repository currently has strongly typed identity contracts in `src/core/identity.rs`.

Preserve the separation:

```text
WorkspaceId
AgentId
SessionId
TaskId
AuditEventId
```

and the existing distinction between:

```text
identity != authority
```

Initialization may establish a `WorkspaceId`, but it must not interpret that as permission to perform agent actions.

Likewise, do not add agent/session authorization behavior to this prompt merely because those concepts exist in the identity graph.

If identity validation is required by bootstrap, call the existing typed contracts rather than defining competing types.

---

# 7. Concurrency and atomicity

Consider two `awh init` invocations targeting the same root.

The implementation must avoid ending in a state where:

- two different workspace identities are both treated as canonical;
- one invocation truncates another's manifest;
- a partially written manifest is accepted as valid;
- a concurrent failure leaves misleading bootstrap state.

Use the repository's existing locking/atomic-write/persistence primitives where available.

If the persistence layer cannot provide a stronger concurrency guarantee, document the exact guarantee instead of pretending the operation is fully race-free.

For manifest creation, prefer an atomic write/replace strategy already established by the repository.

---

# 8. Security requirements

Initialization is a security boundary.

The implementation must:

- fail closed on malformed security-sensitive state;
- never log secrets;
- never copy credentials into workspace context;
- never broaden permissions as a side effect of initialization;
- never enable remote access implicitly;
- never trust an agent merely because its identity exists;
- never trust an MCP server merely because it is registered;
- preserve deny-by-default behavior where policy is not explicitly configured.

Do not weaken sandbox, path, policy, or authorization code to make initialization tests pass.

---

# 9. CLI contract

Verify the real compiled `awh init` command, not only the service function.

Check:

- successful initialization;
- repeated initialization;
- invalid root;
- malformed state;
- unsupported state/version;
- useful error messages;
- correct exit status;
- stable machine-readable output if the current CLI exposes structured output;
- no accidental diagnostic noise on protocol stdout when the command is used in an MCP-related context.

Preserve existing CLI conventions.

Do not invent a new output format unless the current contract is demonstrably broken.

---

# 10. Tests

Use **real temporary filesystem roots**.

Add or update focused tests for at least:

### Fresh initialization

- fresh directory becomes an AWH workspace;
- workspace manifest exists;
- workspace identity is valid and durable;
- required bootstrap state exists.

### Idempotence

- second initialization succeeds with the existing identity;
- manifest bytes are unchanged when no mutation is required;
- existing state is preserved.

### Corruption

- malformed manifest is rejected;
- unsupported manifest version is rejected;
- wrong-root manifest is rejected;
- malformed security/policy state is rejected;
- rejected initialization does not silently replace the corrupt state.

### Root validation

- regular file is rejected;
- invalid/unusable root is rejected;
- canonical root binding is correct.

### Concurrency

Where practical with the existing persistence implementation:

- concurrent initialization does not create competing identities;
- no accepted partial manifest is possible;
- the final workspace has exactly one valid canonical identity.

### CLI

Test the real compiled binary for the externally visible `awh init` behavior, including exit codes and important output.

Do not weaken assertions merely to make existing tests pass.

---

# 11. Verification gates

After implementation, run the applicable repository gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Run focused tests first when iterating, then the complete applicable suite.

If a gate cannot run in the current environment, report:

```text
NOT VERIFIED
```

and explain why.

Never claim a test passed without actually running it.

---

# 12. Scope boundary

This prompt owns only:

- workspace root validation;
- durable workspace identity/bootstrap;
- initialization state creation;
- initialization idempotence;
- initialization corruption handling;
- initialization concurrency/atomicity guarantees;
- the direct CLI/service tests required to prove the above.

Do **not** implement:

- AgentProfile;
- AgentRegistry;
- AgentSession lifecycle/authority;
- per-agent MCP routing;
- capability enforcement;
- edit transactions;
- edit execution;
- snapshots;
- rollback;
- persistent audit architecture;
- worktree isolation;
- connector orchestration;
- model routing;
- autonomous agent orchestration;
- remote execution.

Those are separate implementation contracts.

---

# 13. Independence rule

This prompt must be executable against the repository as it exists now.

Do not write instructions such as:

```text
"after Prompt 02"
"after AGENT-001"
"after the Trust Wedge PR"
"once another prompt implements..."
```

If a later feature is absent, initialize only the state required by the current runtime and document the boundary.

The prompt may describe the larger Trust Wedge architecture for context, but implementation must remain limited to this prompt's ownership.

---

# 14. Completion criteria

Consider this prompt complete only when all applicable statements are demonstrably true:

- [ ] Current repository was inspected before modification.
- [ ] Existing init/identity/persistence abstractions were reused rather than duplicated.
- [ ] A valid workspace receives one durable identity.
- [ ] Repeated init is idempotent.
- [ ] Valid existing state is preserved.
- [ ] Corrupt/incompatible state fails closed.
- [ ] Root binding prevents accidental workspace identity reuse.
- [ ] Initialization does not grant agent authority.
- [ ] Initialization does not enable remote access.
- [ ] Concurrency behavior is tested or its exact limitation is documented.
- [ ] Real CLI behavior is tested.
- [ ] Regression tests cover the security-sensitive failure paths.
- [ ] Formatting/check/tests/Clippy/diff verification were run where available.
- [ ] No unrelated features were added.
- [ ] No duplicate service, identity, or persistence implementation was introduced.

---

# 15. Final report

After implementation, report exactly:

```text
Prompt 01 — Result

Status:
COMPLETE / PARTIAL / BLOCKED

Baseline:
<commit/ref inspected>

Implementation:
- ...

Files changed:
- ...

Workspace contract:
- ...

Idempotence behavior:
- ...

Corruption/fail-closed behavior:
- ...

Concurrency guarantee:
- ...

Tests added/updated:
- ...

Commands executed:
- ...

Verification results:
- ...

Security impact:
- ...

Known limitations:
- ...

Out of scope:
- ...

Next prompt:
02-agent-runtime-identity.md
```

Do not claim the broader Trust Wedge is complete because this prompt is complete.
