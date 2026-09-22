# Prompt 02 — Agent Runtime Identity, Profile, Registry & Session (TW-002 / AGENT-001)

## Mission

Implement one canonical **AWH-native agent runtime identity boundary** connecting an external coding agent to an AWH workspace and runtime session.

This prompt consolidates the Agent Profile / Agent Registry / Agent Session requirements represented by the Trust Wedge and AGENT-001 work. It is the **single implementation contract for Prompt 02**.

It must remain standalone and executable against the repository as it exists now.

The target identity graph is:

```text
Workspace
   │
   └── AgentProfile
          │
          └── AgentRegistry
                 │
                 └── AgentSession
                        │
                        └── caller identity
```

Later runtime entities may correlate to this identity:

```text
AgentSession
   ├── Task
   ├── Edit
   ├── Snapshot
   └── Audit/Event
```

This prompt does **not** implement those later subsystems. It establishes the identity contract they can reference.

---

# 1. Project boundary

AWH is an **agent-agnostic, local-first workspace runtime for coding agents**.

External agents own:

- reasoning;
- planning;
- model selection;
- agent intelligence;
- agent-specific orchestration.

AWH owns:

- workspace state;
- runtime identity;
- capabilities and policy boundaries;
- controlled tools;
- MCP/CLI/TUI/API interfaces;
- persistence and observability.

Do not turn this implementation into an LLM runtime, model router, autonomous planner, generic workflow engine, or multi-agent swarm scheduler.

The agent identity layer must be usable by Claude Code, Codex, OpenCode, Qwen, OpenHands, or another external agent without embedding any provider-specific reasoning logic.

---

# 2. Required first action — repository forensics

Before editing, inspect the **current target branch**, not an assumed historical implementation.

Run the applicable commands:

```bash
git status --short
git branch --show-current
git log --oneline -10
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
docs/threat-model.md
docs/roadmap/PROJECT_ROADMAP.md
docs/roadmap/GROWTH_STRATEGY.md
docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
docs/implementation-prompts/README.md
docs/implementation-prompts/01-init-runtime-contracts.md
docs/implementation-prompts/03-mcp-routing-and-security.md
docs/implementation-prompts/07-edit-authorization.md
docs/implementation-prompts/08-snapshots-provenance.md
docs/implementation-prompts/10-persistent-audit.md
src/core/identity.rs
src/core/agents.rs
src/core/workspace.rs
src/core/tasks.rs
src/main.rs
src/mcp/dispatcher.rs
src/mcp/
tests/
```

Also search for existing identity/lifecycle concepts:

```bash
rg -n "Agent|AgentStatus|AgentStore|CapabilityGrant|SessionLifecycle|SessionIdentity|WorkspaceId|AgentId|SessionId|TaskId"
rg -n "agent start|agent stop|agent status|agent run|AgentProfile|AgentRegistry|AgentSession"
rg -n "sessionId|session_id|agent_id|workspace_id"
```

### Historical-source rule

Older `docs/issue-resolving-prompts/` and `docs/trust-wedge/` material has been consolidated into `docs/implementation-prompts/`.

Treat the current implementation prompt collection, current roadmap, current project context, and current source code as the active contract.

Historical documents may explain intent, but must not cause a duplicate implementation or override verified current behavior.

---

# 3. Current implementation facts to preserve

The Rust branch already contains pieces of the intended identity foundation.

In particular, inspect and reuse:

- `src/core/identity.rs` typed runtime IDs;
- `src/core/agents.rs` persisted `AgentStore`;
- existing `Agent` / `AgentStatus` models;
- existing `CapabilityGrant` persistence;
- existing MCP `SessionLifecycle`;
- existing workspace manifest and workspace identity;
- existing task/edit/audit identity fields where present.

The current `AgentStore` is workspace-local under:

```text
.agent/agents/
```

and existing agent statuses include:

```text
Created
Active
Paused
Stopped
Failed
```

The current MCP `SessionLifecycle` is a **protocol session state machine**. It is not automatically the same thing as the AWH-native `AgentSession`.

Preserve that distinction.

Do not replace the existing MCP protocol session with the AWH agent session.

Do not create a second `AgentStore`, second identity module, second workspace identity, or second MCP session implementation.

---

# 4. Canonical identity model

Implement or converge on these concepts:

## 4.1 AgentProfile

An `AgentProfile` describes one configured external agent.

It should contain only the profile information actually required by the current runtime contract.

The profile may describe:

- stable agent identity/name;
- display metadata;
- role/type where required;
- enabled/disabled lifecycle configuration;
- declared/requested capabilities;
- configured workspace association where the current implementation supports it;
- profile-specific runtime configuration that is actually consumed by AWH.

A profile declaration is **not authority**.

The invariant is:

```text
AgentProfile ≠ Authorization
```

Do not allow a profile field such as:

```text
permissions = ["filesystem.write"]
```

to bypass the authoritative capability/policy decision point.

If the current repository already has configuration structures that can represent the required profile, extend them rather than inventing a parallel configuration format.

---

# 5. AgentRegistry

Create one authoritative registry abstraction for resolving configured/persisted agents.

The registry must:

- resolve an agent deterministically;
- distinguish unknown agents from known agents;
- expose lifecycle state;
- preserve workspace ownership;
- support safe listing/discovery;
- reject duplicate/conflicting identities;
- survive restart according to the repository's persistence contract;
- avoid treating registry membership as authorization.

The registry is responsible for **identity resolution**, not permission granting.

Required invariant:

```text
AgentRegistry membership ≠ capability authorization
```

An agent can exist in the registry while still being:

- disabled;
- stopped;
- inactive;
- unauthorized for a particular capability;
- unable to access a particular workspace/resource.

---

# 6. Agent identity

Use the existing strongly typed identity system where possible.

The canonical graph is:

```text
WorkspaceId
   ↓
AgentId
   ↓
SessionId
   ↓
TaskId
```

with edit/snapshot/audit identifiers correlated separately.

A session identity must identify at least:

```text
agent_id
workspace_id
session_id
```

Do not use raw route names, URLs, display names, MCP client names, or arbitrary metadata as substitutes for the canonical agent identity.

The existing `SessionIdentity` concept in `src/core/identity.rs` must be reused or extended rather than duplicated.

---

# 7. AgentSession

Implement the AWH-native session model.

An `AgentSession` represents one runtime association between:

```text
external caller
      ↓
Agent
      ↓
Workspace
```

A session must have:

- stable session identity for its lifetime;
- owning `AgentId`;
- owning `WorkspaceId`;
- explicit lifecycle state;
- creation/last-activity metadata where useful;
- deterministic lookup;
- safe termination;
- no implicit capability grant.

### Critical distinction

There are two session concepts:

```text
MCP protocol session
    = transport/protocol lifecycle

AWH AgentSession
    = application/runtime identity
```

They may be correlated, but they must not be silently conflated.

A single external agent may create multiple protocol connections.

A protocol session must never be allowed to impersonate another AWH agent session merely by supplying an agent name or route.

---

# 8. AgentSession lifecycle

Define explicit lifecycle semantics using the existing repository conventions.

At minimum, distinguish:

```text
Created / Starting
Active
Paused
Stopped / Closed
Failed
Expired
```

Use only states actually needed by the implementation; do not create a decorative state machine.

Define legal transitions.

For example:

```text
Created → Active
Active → Paused
Paused → Active
Active → Stopped
Paused → Stopped
Active → Failed
Created → Failed
```

If expiration is supported, an expired session must not remain usable.

Do not silently reactivate a stopped, failed, disabled, or expired session because a caller presents a valid ID.

The implementation must document the actual transition table.

---

# 9. Agent lifecycle versus session lifecycle

Keep these levels separate:

```text
AgentProfile / Agent
        ↓
agent lifecycle

AgentSession
        ↓
individual runtime lifecycle
```

Stopping an agent must invalidate or stop its active sessions according to the chosen runtime contract.

Stopping one session must not silently disable the entire agent unless the repository explicitly defines that behavior.

A disabled agent must not create new active sessions.

A stopped/failed session must not be accepted as an active caller.

Do not make lifecycle state itself an authorization grant.

---

# 10. Workspace binding

Every AWH agent session must belong to exactly one workspace.

Resolve:

```text
caller
 → AgentId
 → SessionId
 → WorkspaceId
 → canonical workspace root
```

before any consequential operation.

Reject:

- unknown workspace;
- uninitialized workspace;
- wrong workspace binding;
- session belonging to another workspace;
- agent belonging to another workspace;
- forged workspace/session combinations.

Do not trust a client-provided filesystem path as proof of workspace identity.

Use the workspace manifest/identity contract established by the existing initialization implementation.

---

# 11. Caller resolution

Create one deterministic identity-resolution path.

Conceptually:

```text
untrusted transport/input
        ↓
authenticate transport where required
        ↓
resolve AWH session
        ↓
resolve AgentId
        ↓
resolve AgentRegistry entry
        ↓
verify agent lifecycle
        ↓
verify workspace binding
        ↓
produce trusted AgentSession context
        ↓
authorization layer
        ↓
AWH service
```

This prompt owns the identity-resolution portion.

It does **not** implement the complete capability/policy decision engine.

If authorization is not yet implemented for a given path, do not fake it inside the identity layer. Return the identity context to the existing security boundary or document the missing authorization boundary.

---

# 12. Identity is not authorization

This is a mandatory security invariant.

Never authorize an operation merely because:

- the agent exists;
- the profile lists a capability;
- the caller supplies a matching agent name;
- the URL contains the agent name;
- the MCP route is `/{agent}/mcp`;
- the client claims to be Claude/Codex/Qwen/OpenCode;
- the session ID is syntactically valid;
- the session belongs to an active agent.

The correct relationship is:

```text
Identity
   ↓
Capability context
   ↓
Policy decision
   ↓
Operation
```

Prompt 03 owns the MCP routing/security boundary and the authoritative high-risk MCP decision point.

Do not duplicate that policy engine here.

---

# 13. Configuration and profile loading

Use the repository's actual configuration mechanism.

The roadmap describes a declarative model conceptually like:

```text
configuration
    ↓
AgentProfile
    ↓
AgentRegistry
    ↓
runtime session
```

If TOML is already supported, integrate with the existing configuration loader.

If TOML profile loading is not yet implemented, implement only the minimum profile representation required by this prompt and do not invent a complete future configuration system.

Configuration precedence must remain consistent with the current project contract.

Do not allow environment variables or CLI flags to silently grant capabilities that the policy system does not authorize.

Malformed profile configuration must fail closed and produce a useful error.

---

# 14. Persistence

Persist only the durable state that actually needs persistence.

Reasonable durable entities include:

- agent profile/configuration;
- agent registration/lifecycle state where required;
- stable agent identity;
- session records only if the runtime contract requires restart recovery.

Do **not** persist a live session as active merely because the process crashed.

A process restart must not resurrect an old runtime session with active authority unless the repository explicitly defines and verifies resumable sessions.

Prefer:

```text
persisted agent identity/configuration
        +
new runtime session after restart
```

over:

```text
old live session automatically becomes trusted again
```

Persisted state must be validated before use.

Corrupt agent/profile/session state must fail closed.

Never silently replace corrupted state with a new agent or session.

---

# 15. ID safety

Agent IDs and session IDs are security-sensitive identifiers.

Reuse the existing validation primitives.

At minimum reject:

- empty IDs;
- path separators;
- traversal sequences;
- control characters;
- unbounded input;
- ambiguous workspace identity.

Do not permit an agent ID to escape:

```text
.agent/agents/
```

through filename/path manipulation.

Do not use display names as filesystem identifiers unless the existing contract explicitly makes them safe.

---

# 16. Simultaneous agents

The runtime must isolate identity between concurrent agents.

For example:

```text
AWH
 ├── Claude
 │    └── Session A
 │         └── Workspace A
 │
 ├── Qwen
 │    └── Session B
 │         └── Workspace B
 │
 └── OpenCode
      └── Session C
           └── Workspace C
```

At minimum, concurrent sessions must not accidentally:

- overwrite each other's identity;
- resolve to another agent;
- inherit another session's workspace;
- inherit another agent's capabilities;
- share mutable session state without explicit design;
- stop another agent/session accidentally.

If multiple agents intentionally share a workspace, that must be an explicit workspace relationship rather than an identity-resolution accident.

---

# 17. Task/edit/audit correlation

Do not implement task, edit, snapshot, or audit subsystems here.

However, expose stable identity fields so those systems can correlate later.

Where the current models already support fields such as:

```text
agent_id
session_id
workspace_id
```

reuse them.

Do not create a second provenance schema.

Do not write audit events merely to prove identity unless the existing audit service already provides an appropriate integration point.

Identity resolution should be observable through existing structured logging/error mechanisms without leaking credentials or private content.

---

# 18. MCP boundary

The existing MCP subsystem has its own protocol session lifecycle.

Do not rewrite it in this prompt.

Instead, establish the clean application-level relationship:

```text
MCP connection/session
        ↓
authenticated/trusted caller
        ↓
AWH AgentSession
        ↓
AgentId + WorkspaceId
        ↓
capability/policy decision
        ↓
MCP/AWH tool
```

A route such as:

```text
/claude/mcp
```

is only a routing hint.

It is not proof that the caller is Claude.

Similarly:

```text
/claude/sse
```

does not grant Claude permissions.

Do not implement agent-specific MCP endpoint routing here; Prompt 03 owns that security/routing boundary.

---

# 19. CLI surface

Converge with the project's intended agent lifecycle surface without implementing unrelated orchestration.

The roadmap target includes:

```bash
awh agent list
awh agent show <name>
awh agent start <name>
awh agent start --all
awh agent stop <name>
awh agent restart <name>
awh agent run <name>
awh agent status
```

The current repository already has earlier agent commands such as:

```bash
awh agent create
awh agent list
awh agent inspect
awh agent grant
awh agent revoke
```

Do not delete working compatibility commands without evidence that the current canonical CLI contract replaces them.

Where lifecycle commands are implemented by this prompt, they must call the canonical AgentRegistry/AgentSession services.

Do not put identity lifecycle logic directly into multiple CLI handlers.

CLI output must distinguish:

- unknown agent;
- disabled agent;
- inactive/stopped agent;
- active agent;
- unknown session;
- invalid workspace binding.

---

# 20. Shared-service rule

CLI, MCP, TUI, and Control API must not each implement their own identity semantics.

Create or converge on one application/service boundary that can be called from multiple interfaces.

Conceptually:

```text
CLI ────────┐
MCP ────────┤
TUI ────────┼──→ AgentRuntime / Registry / Session services
Control API ┘
```

Do not create:

```text
CLI AgentRegistry
MCP AgentRegistry
API AgentRegistry
TUI AgentRegistry
```

with different behavior.

---

# 21. Security requirements

The implementation must fail closed when identity cannot be established confidently.

Reject:

- unknown agent;
- unknown session;
- disabled agent;
- inactive/expired session;
- malformed identity;
- wrong workspace;
- missing workspace;
- corrupt persisted identity state;
- ambiguous identity;
- identity substitution;
- session belonging to another agent.

Never:

- infer identity from a display name;
- trust URL path alone;
- trust MCP client metadata as authorization;
- copy capabilities from another agent;
- reuse another agent's session;
- auto-activate an agent during lookup;
- auto-grant permissions during session creation;
- log secrets/tokens;
- expose private profile configuration unnecessarily.

---

# 22. Tests

Use real temporary workspaces and real persistence.

## AgentProfile

Test:

- valid profile serialization;
- malformed profile rejection;
- duplicate/conflicting profile IDs;
- disabled profile behavior;
- persistence/reload;
- safe identifier validation.

## AgentRegistry

Test:

- create/register;
- lookup;
- list;
- deterministic ordering if applicable;
- unknown agent;
- duplicate identity;
- disabled/inactive agent;
- corrupted registry/profile state;
- restart persistence.

## AgentSession

Test:

- create session;
- session has distinct `SessionId`;
- correct AgentId;
- correct WorkspaceId;
- lifecycle transitions;
- stopped/failed/expired session rejection;
- unknown session rejection;
- restart behavior;
- multiple concurrent sessions.

## Workspace isolation

Test:

- agent A + workspace A resolves correctly;
- agent B + workspace B resolves correctly;
- agent A cannot claim session B;
- agent A cannot claim workspace B;
- wrong workspace identity is rejected;
- malformed workspace identity is rejected.

## Identity substitution

Explicitly test attacks such as:

```text
agent=A, session=B
agent=A, workspace=B
agent=B, session=A
unknown-agent + valid-looking session
disabled-agent + valid session
route=/agent-a + caller claiming agent-b
```

Every mismatch must fail without granting authority.

## Concurrency

Run multiple session creation/resolution operations concurrently and verify:

- no duplicate runtime identity;
- no cross-session state leakage;
- no accidental agent replacement;
- no corrupted persistence;
- no cross-workspace binding.

## Compatibility

Where existing `awh agent create/list/inspect/grant/revoke` commands remain supported, ensure the identity implementation does not regress them.

Do not interpret existing capability grants as fully enforced authorization unless the current authorization implementation actually consumes them.

---

# 23. Verification gates

Run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Also run focused tests for the identity/registry/session modules.

If a verification gate cannot run, report it as:

```text
NOT VERIFIED
```

with the exact reason.

Never claim a verification result that was not executed.

---

# 24. Scope boundary

This prompt owns:

- AgentProfile representation;
- AgentRegistry;
- AWH-native AgentSession;
- caller → agent → session → workspace identity resolution;
- lifecycle semantics for agents/sessions;
- workspace binding;
- identity persistence required by the above;
- identity validation;
- identity isolation;
- direct CLI/service integration required to exercise the identity lifecycle;
- tests proving the identity boundary.

This prompt does **not** own:

- MCP agent-specific routing/security policy;
- capability authorization engine;
- policy-engine redesign;
- edit authorization;
- edit execution;
- snapshots;
- rollback;
- audit subsystem redesign;
- Git worktrees;
- connector orchestration;
- model routing;
- LLM execution;
- autonomous planning;
- distributed agent orchestration;
- multi-agent collaboration/handoff/merge systems.

---

# 25. Independence rule

This prompt must be executable without requiring Prompt 01, Prompt 03, AGENT-002, AGENT-003, or any future PR to exist.

Inspect the current repository and use whatever valid workspace/bootstrap state already exists.

If the repository lacks a prerequisite required for a safe identity operation, implement the **minimum local contract needed** or report the blocker. Do not assume a future prompt will magically provide it.

Do not write implementation instructions such as:

```text
"after Prompt 03"
"after AGENT-002"
"after the Trust Wedge PR"
"once the MCP routing prompt is merged"
```

The prompt may reference neighboring architecture to define boundaries, but it must never depend on another prompt's implementation.

---

# 26. Completion criteria

Prompt 02 is complete only when all applicable criteria are demonstrably satisfied:

- [ ] Existing identity, agent, workspace, MCP-session, and persistence implementations were inspected first.
- [ ] No duplicate identity/registry/session system was introduced.
- [ ] AgentProfile has a clear representation and validation boundary.
- [ ] AgentRegistry is the canonical identity lookup/lifecycle boundary.
- [ ] Agent membership is not treated as authorization.
- [ ] AgentSession is distinct from MCP protocol SessionLifecycle.
- [ ] Every AgentSession has a valid AgentId and WorkspaceId.
- [ ] Caller resolution is deterministic.
- [ ] Unknown/disabled/inactive identities fail closed.
- [ ] Workspace mismatches fail closed.
- [ ] Identity substitution attacks are rejected.
- [ ] Session lifecycle transitions are explicit and tested.
- [ ] Restart behavior does not resurrect unauthorized live sessions.
- [ ] Concurrent agents/sessions remain isolated.
- [ ] Existing compatible agent CLI behavior is preserved.
- [ ] Identity fields are available for later task/edit/snapshot/audit correlation.
- [ ] No capability/policy engine was duplicated.
- [ ] No MCP transport implementation was duplicated.
- [ ] Real persistence tests pass.
- [ ] Formatting/check/tests/Clippy/diff verification was run where available.
- [ ] No unrelated feature scope was added.

---

# 27. Final implementation report

After implementation, report:

```text
Prompt 02 — Result

Status:
COMPLETE / PARTIAL / BLOCKED

Baseline:
<commit/ref inspected>

Identity model:
<AgentProfile → AgentRegistry → AgentSession → Workspace>

Agent lifecycle:
<actual states/transitions>

Session lifecycle:
<actual states/transitions>

Persistence:
<what is persisted and restart behavior>

Caller resolution:
<exact resolution path>

Workspace binding:
<exact validation behavior>

Security:
<identity substitution / disabled / mismatch behavior>

Tests:
<tests added/updated>

Commands executed:
<verification commands>

Verification:
<results>

Files changed:
<files>

Compatibility impact:
<existing behavior preserved/changed>

Known limitations:
<limitations>

Out of scope:
<explicit exclusions>
```

Do not claim that MCP routing, capability enforcement, editing, snapshots, rollback, audit, or multi-agent collaboration are complete merely because AgentProfile/AgentRegistry/AgentSession is complete.
