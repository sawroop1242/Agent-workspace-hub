# AWH Features

## Agent Profiles & Policy-Routed MCP

AWH supports named agent profiles that define how an external coding agent connects to the AWH runtime and which capabilities/tools it may use.

The feature is deliberately split into two layers:

```text
TOML configuration
      ↓
AgentProfile
      ↓
AgentRegistry
      ↓
AgentServerManager
      ↓
AgentRouter
      ↓
AgentSession
      ↓
PolicyEngine
      ↓
Core AWH services
```

The TOML file is declarative configuration. The runtime `PolicyEngine` remains the authoritative authorization boundary.

### Per-agent MCP endpoints

Enabled agents can receive namespaced MCP endpoints:

```text
/{agent}/mcp
/{agent}/sse
```

Example:

```text
/claude/mcp
/claude/sse

/qwen/mcp
/qwen/sse

/opencode/mcp
/opencode/sse
```

The agent name identifies the runtime profile, but **the URL namespace is not itself authentication or authorization**. Every request must still bind to an AWH agent/session identity and pass the normal capability/policy checks.

### TOML configuration

A profile can configure:

- agent name and enabled state
- MCP and SSE availability
- tool allowlists
- workspace selection
- filesystem permissions
- Git permissions
- terminal/process permissions
- resource/concurrency limits
- authentication/approval requirements as those capabilities mature

Example:

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

[agents.qwen]
enabled = true

[agents.qwen.permissions]
tools = [
    "filesystem.read",
    "filesystem.search",
    "filesystem.patch",
    "terminal.execute",
]
```

### CLI server lifecycle

The CLI controls which configured agent servers are actually active.

```text
awh agent list
awh agent show <name>
awh agent start <name>...
awh agent start --all
awh agent stop <name>
awh agent restart <name>
awh agent status
awh agent run <name>
```

Examples:

```bash
# Start only Claude's MCP server.
awh agent start claude

# Start two independent agent servers.
awh agent start claude qwen

# Start every enabled profile.
awh agent start --all

# Run one agent server in the foreground until shutdown.
awh agent run claude
```

If only Claude is started, only Claude's configured endpoint is registered:

```text
/claude/mcp   ACTIVE
/claude/sse   ACTIVE

/qwen/mcp     NOT ACTIVE
/opencode/mcp NOT ACTIVE
```

Inactive agents should not merely receive denied tools; their server endpoints should not be registered as active listeners/routes.

### Agent isolation

Each active agent should have an explicit runtime identity that propagates through:

```text
Agent
  ↓
Session
  ↓
Workspace
  ↓
Worktree
  ↓
CapabilityContext
  ↓
Tool invocation
  ↓
Audit / provenance
```

This identity must also be available to consequential operations such as filesystem edits, Git operations, snapshots, context, memory, and process execution.

### Tool discovery and invocation

AWH should maintain one canonical tool registry and one authorization boundary.

For an agent with an allowlist:

1. tool discovery should expose only permitted tools where practical;
2. invocation must independently re-check authorization;
3. a direct/internal invocation must not bypass the policy engine;
4. denied operations must produce structured errors and zero unauthorized side effects.

Example denial:

```text
PermissionDenied {
    agent: "claude",
    tool: "terminal.execute",
    reason: "tool_not_granted"
}
```

### Concurrency

Multiple agent servers may run concurrently:

```text
                 AWH
                  │
        ┌─────────┼─────────┐
        │         │         │
     Claude      Qwen    OpenCode
        │         │         │
     Session A Session B Session C
        │         │         │
    Worktree A Worktree B Worktree C
```

Agent identity, policy, workspace and session state must remain isolated even when requests are processed concurrently.

### Security invariants

- An unknown agent name is rejected.
- A disabled agent cannot start or accept MCP requests.
- An inactive agent has no active agent-specific MCP route.
- Agent route identity must be validated and bound to a session.
- Route names must not permit path traversal or encoded-path bypasses.
- Tool permissions are enforced by the authoritative policy engine.
- MCP discovery and invocation cannot bypass capability checks.
- Internal service calls cannot silently acquire additional agent capabilities.
- Agent identity is included in audit/provenance for consequential operations.

### Relationship to the AWH product boundary

This feature does **not** turn AWH into an agent framework. External agents continue to own reasoning, planning and model selection. AWH owns the controlled runtime boundary around those agents: identity, sessions, capabilities, workspace state, MCP exposure, isolation, audit and lifecycle.

## Other core AWH feature areas

- MCP-first interoperability
- Controlled workspace/filesystem operations
- Agent-grade patch editing
- Git and agent worktrees
- Capability and policy enforcement
- Snapshots, undo and provenance
- Context and project state
- Developer-oriented memory
- Skills and capability packages
- Agent/session lifecycle
- Audit and observability
- TUI and Control API
- Multi-agent workspace coordination
- Local-first and optional remote runtime

See `docs/PROJECT_ROADMAP.md` and `docs/ROADMAP_AGENT_PROFILES_POLICY_MCP.md` for implementation sequencing.