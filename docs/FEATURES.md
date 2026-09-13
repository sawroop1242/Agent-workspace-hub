# AWH Final Features

> This document describes the final target feature contract. Implementation status is tracked separately; planned functionality must not be represented as currently available.

## Product boundary

AWH is an **agent-agnostic, local-first workspace runtime for coding agents**.

AWH owns workspace/runtime state, controlled editing, Git/worktree isolation, capabilities/policy, snapshots/provenance, context, memory, skills, agent/session state, audit/observability, and MCP/CLI/TUI/Control API interfaces.

External agents own reasoning, planning, model selection, and agent intelligence.

AWH is not an agent framework, generic model router, or general-purpose workflow engine.

## Final feature set

### Foundation and distribution

- Configuration with deterministic precedence
- Persistent state/storage abstractions
- Cross-platform paths
- Structured logging and errors
- Version/build information
- Install, upgrade, uninstall, release artifacts and checksums
- Linux x86_64/ARM64, macOS x86_64/ARM64, Windows x86_64, Android/Termux ARM64 where practical
- Shell completion

Core commands: `awh init`, `awh version`, `awh status`, `awh doctor`, `awh config`.

### Workspace runtime

`awh workspace create|list|open|info|remove`

Workspace is the primary scope for filesystem, Git, agent, session, context, memory, skills, snapshots and policy.

### Agent-grade filesystem editing

Basic: `awh fs read|write|stat|search|hash|verify`

Controlled edits: `awh fs patch|replace|insert|delete-range|apply-diff|history|rollback`

Every consequential edit follows:

```text
request → capability/policy check → locate/read → context validation
→ conflict detection → atomic apply → verification → snapshot/provenance → audit
```

No silent stale-state overwrite and no separate edit semantics per interface.

### Git and first-class worktrees

Git: `status`, `diff`, `staged-diff`, `log`, `branch`, `branches`, `worktree`, `stage`, `unstage`, `commit`, `push`, `pull`, `reset`, `clean`, `validate`.

Worktree: `create`, `list`, `inspect`, `remove`, `merge`, `status`.

Agent, session, workspace, worktree, branch and modified-file identity remain associated.

### Capability and policy engine

Capabilities cover resources/actions such as `filesystem.read`, `filesystem.write`, `filesystem.delete`, `git.read`, `git.write`, `process.execute`, `network.request`, `mcp.invoke`, and `secrets.read`.

```text
awh capability list|show|grant|revoke|check
awh policy list|show|check|validate|explain
```

The PolicyEngine is authoritative. TOML configuration, MCP route names, tool discovery and internal callers cannot bypass it.

### Snapshots, undo and provenance

```text
awh snapshot create|list|show|restore|delete|diff
```

Track where practical: file, agent, session, capability, tool, timestamp, before hash, after hash and snapshot. Snapshots are workspace safety/recovery, not a replacement for OS/container/VM sandboxing.

### Context engine

`awh context show|save|update|clear|search`

Context integrates filesystem, Git, workspace metadata, session history, changes, skills, project configuration and tool results while respecting budgets and stale-context detection.

### Developer-oriented memory

`awh memory list|get|search|add|update|delete`

Memory is scoped to projects/workspaces/sessions/agents as appropriate and focuses on coding workflow state.

### Skills and capability packages

`awh skill list|show|install|remove|enable|disable`

A skill declares tools/capabilities, inputs, outputs and policy requirements. Requested capabilities are evaluated by PolicyEngine.

### Agent Profiles and policy-routed MCP

```text
TOML → AgentProfile → AgentRegistry → AgentServerManager
→ AgentRouter → AgentSession → PolicyEngine → AWH services
```

Configured endpoints are `/{agent}/mcp` and `/{agent}/sse`. Lifecycle:

```text
awh agent list
awh agent show <name>
awh agent start <name>...
awh agent start --all
awh agent stop <name>
awh agent restart <name>
awh agent run <name>
awh agent status
```

The route namespace is routing identity only, not authorization. Disabled/inactive agents have no active agent-specific route.

### MCP infrastructure

`awh mcp serve|list|add|remove|inspect|test|logs`

MCP must support lifecycle, discovery, transports, concurrent clients, cancellation/timeouts, structured errors, authentication hooks, auditing, limits and real-client interoperability.

### Sessions and tasks

```text
awh session list|show|create|stop|status
awh task list|show|create|update|cancel|assign
```

A session binds agent identity to workspace, worktree, capabilities, context, memory, snapshots and audit.

### Audit and observability

```text
awh audit list|show|search|export
awh logs show|follow|clear
```

Audit consequential actions including edits, Git mutations, terminal execution, capability decisions, MCP requests and connector activity without persisting secrets.

### Terminal runtime

`awh terminal run|list|kill`

Terminal access is high-risk and requires policy/capability checks, session identity, bounded execution, termination/timeouts, resource limits and audit.

### Multi-agent collaboration

`awh collaboration agents|status|handoff|assign|conflicts|events`

AWH coordinates state, ownership, isolation and events. Agents remain responsible for reasoning. Distributed swarms, generic DAG workflow engines and autonomous schedulers are out of scope.

### Control API

`awh api serve|status|tokens|logs`

The API exposes stable control operations over the same application services used by CLI/TUI/MCP, with scoped authentication/authorization and local-first operation.

### TUI

`awh tui`

The TUI is a control/observability interface, not a second application core or full IDE. It consumes the same service/backend abstractions as CLI and API.

### Connectors and ecosystem

`awh connector list|add|remove|inspect|test|invoke`

Adapters integrate external services without bypassing policy, authentication or audit.

### Advanced infrastructure

Optional modular adapters may provide process/OS/container/WASM sandboxing, quotas, network policies, secrets managers, remote execution, snapshot deduplication and enterprise RBAC. These are demand-driven and must not destabilize the core.

## Security invariants

- Unknown agents are rejected.
- Disabled agents cannot start or accept MCP requests.
- Inactive agents have no active agent-specific MCP route.
- URL namespaces never grant authorization.
- Every consequential tool invocation is policy checked.
- Direct/internal service calls cannot silently acquire extra capabilities.
- Path traversal and encoded-path bypasses are rejected.
- Agent/session identity propagates to audit and provenance.
- Dangerous Git, terminal, snapshot, capability and connector mutations require their enforcement layers.
- Secrets are never written to logs or audit records.

## Implementation source of truth

- `docs/PROJECT_ROADMAP.md` — phases, dependencies and build order
- `docs/CLI.md` — final CLI contract
- `docs/ROADMAP_AGENT_PROFILES_POLICY_MCP.md` — agent profile/MCP details
- `docs/RECONCILED_ROADMAP_V2.md` — historical reconciled status snapshot
