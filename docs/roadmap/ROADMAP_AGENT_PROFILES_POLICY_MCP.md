# AWH Roadmap Addendum — Agent Profiles & Policy-Routed MCP

**Feature:** Agent Profiles & Policy-Routed MCP  
**Primary roadmap phases:** 1, 4, 9, 12, 13  
**Priority:** P0/P1 foundation work for multi-agent runtime

This addendum records the new agent-server lifecycle and per-agent MCP routing model without duplicating the complete project roadmap.

## Why this belongs in the core roadmap

AWH already has MCP infrastructure, agent structures, permissions and policy primitives. The missing architectural layer is a coherent runtime identity that connects a named external agent to an AWH session, workspace, worktree and authorization context.

The feature provides that boundary:

```text
TOML
  ↓
AgentProfile
  ↓
AgentRegistry
  ↓
AgentServerManager
  ↓
/{agent}/mcp or /{agent}/sse
  ↓
AgentSession
  ↓
Capability / Policy Engine
  ↓
AWH core services
```

## Roadmap integration

### Phase 1 — MCP Infrastructure

Add:

- [ ] Generic `/{agent}/mcp` routing
- [ ] Generic `/{agent}/sse` routing
- [ ] Agent-aware MCP session binding
- [ ] Per-agent MCP server activation/deactivation
- [ ] Per-agent tool discovery filtering
- [ ] Concurrent agent server support
- [ ] Route and agent-name validation
- [ ] Unknown/disabled/inactive agent rejection
- [ ] Real-client verification for agent-specific endpoints

The existing global MCP server should remain available only where the compatibility model explicitly requires it; agent-specific routes are the preferred boundary for named profiles.

### Phase 4 — Capability & Policy Engine

Add:

- [ ] TOML tool allowlists mapped into `AgentProfile`
- [ ] Canonical agent capability context
- [ ] Authoritative tool authorization for every agent request
- [ ] Agent + session + workspace policy evaluation
- [ ] No authorization-by-URL-only rule
- [ ] Structured permission-denied errors
- [ ] Discovery filtering plus invocation-time enforcement
- [ ] Zero-side-effect denial tests

Architecture rule:

```text
TOML configuration
      ↓
AgentProfile
      ↓
PolicyEngine
```

TOML must not become a second policy engine.

### Phase 9 — Sessions & Agent Management

Make this the main implementation home for the runtime lifecycle:

- [ ] Canonical `AgentProfile` model
- [ ] Agent registry backed by configuration
- [ ] Agent server lifecycle manager
- [ ] Agent/session identity binding
- [ ] `awh agent list`
- [ ] `awh agent show <name>`
- [ ] `awh agent start <name>...`
- [ ] `awh agent start --all`
- [ ] `awh agent stop <name>`
- [ ] `awh agent restart <name>`
- [ ] `awh agent status`
- [ ] `awh agent run <name>`
- [ ] Workspace/worktree association
- [ ] Agent identity propagation into audit/provenance

Expected runtime model:

```text
AgentProfile
 └── Agent
      └── Session
           ├── Workspace
           ├── Worktree
           ├── Capabilities
           ├── Context
           ├── Memory
           ├── Snapshots
           └── Audit
```

### Phase 12 — Multi-Agent Collaboration

Agent Profiles & Policy-Routed MCP is a prerequisite rather than a separate swarm system.

Add:

- [ ] Multiple configured agents active concurrently
- [ ] Independent sessions per agent
- [ ] Independent worktree assignment
- [ ] Agent-specific ownership
- [ ] Handoff between agent sessions
- [ ] Conflict/merge events
- [ ] Agent lifecycle events
- [ ] Per-agent runtime status

Example:

```text
AWH
 ├── Claude → Session A → Worktree A → /claude/mcp
 ├── Qwen → Session B → Worktree B → /qwen/mcp
 └── OpenCode → Session C → Worktree C → /opencode/mcp
```

This remains local-first and does not introduce a distributed agent swarm, general workflow engine, or model router.

### Phase 13 — Control API

Expose the same runtime model through the Control API:

- [ ] Agent profile inspection
- [ ] Agent server status
- [ ] Agent start/stop/restart lifecycle
- [ ] Session association
- [ ] Effective capability inspection
- [ ] Agent event stream
- [ ] Scoped authorization

CLI, TUI, MCP and Control API must call the same runtime services rather than implementing separate agent lifecycle logic.

## Configuration model

Initial configuration should support:

```toml
[server]
host = "127.0.0.1"
port = 9000

[agents.claude]
enabled = true

[agents.claude.mcp]
enabled = true
sse = true

[agents.claude.permissions]
tools = [
    "filesystem.read",
    "filesystem.search",
    "filesystem.patch",
    "git.status",
    "git.diff",
]

[agents.claude.workspace]
root = "./workspace/claude"
```

Future profile settings may include workspace/worktree, Git operations, terminal/process permissions, resource/concurrency limits and approval requirements. Do not add every setting at once; add them when the corresponding runtime capability exists.

## CLI server-selection semantics

```bash
awh agent start claude
```

must activate only the Claude profile's configured MCP/SSE server routes.

```bash
awh agent start claude qwen
```

must activate only Claude and Qwen.

```bash
awh agent start --all
```

must activate all enabled profiles.

```bash
awh agent run claude
```

should provide a foreground lifecycle mode suitable for local development and debugging.

If Claude is the only active profile:

```text
/claude/mcp   ACTIVE
/claude/sse   ACTIVE
/qwen/mcp     INACTIVE
/opencode/mcp INACTIVE
```

Inactive routes should not be treated as active servers with merely denied tools.

## Dependency order

```text
Existing MCP infrastructure
        ↓
AgentProfile + TOML config
        ↓
AgentRegistry
        ↓
AgentServerManager
        ↓
Generic agent MCP router
        ↓
AgentSession identity
        ↓
PolicyEngine enforcement
        ↓
CLI lifecycle
        ↓
Audit/provenance integration
        ↓
Concurrent multi-agent tests
        ↓
TUI / Control API
        ↓
Worktree-backed collaboration
```

## Acceptance workflow

The feature is complete only when a real end-to-end test proves:

1. Define Claude and Qwen profiles in TOML with different tool permissions.
2. Start only Claude.
3. Confirm Claude MCP discovery works.
4. Confirm Qwen's agent route is inactive.
5. Confirm Claude can invoke an allowed tool.
6. Confirm Claude cannot invoke a denied tool and no side effect occurs.
7. Start Qwen concurrently.
8. Confirm both sessions retain independent identities and permissions.
9. Confirm audit events contain agent/session identity.
10. Confirm CLI status reports both active servers.
11. Confirm stopping Claude does not terminate Qwen.
12. Confirm malformed/unknown/disabled agent routes are rejected safely.
13. Run the workflow through real MCP clients where supported.

## Non-goals

This feature does not add:

- an autonomous agent scheduler
- a distributed agent swarm
- a generic model router
- a LangGraph-style workflow engine
- authorization logic duplicated inside each MCP tool

AWH remains the runtime/control layer; external agents remain responsible for reasoning and planning.