# AWH Evolution Master Prompt

## Mission

Evolve Agent Workspace Hub (AWH) into an AI-native workspace operating system, multi-agent orchestration platform, capability/security control plane, developer CLI, and eventually a full TUI.

The target is a coherent Rust-native system that combines the strongest ideas of Claude Code, Codex CLI, OpenCode, MCP control planes, container-style isolation, workflow orchestration, IDE workspace management, AI team collaboration, security policy enforcement, and observability.

## Core Architecture

Use this architecture as the target:

```text
Human / User
    |
 CLI / TUI / API
    |
 AWH Control Plane
    |
 +----------------------+-----------------------+
 |                      |                       |
Agent Registry      Task/Workflow Engine    Policy/Approval
 |                      |                       |
 +----------------------+-----------------------+
                        |
                Agent Runtime Manager
                        |
       +----------------+----------------+
       |                |                |
 Agent Runtime A   Agent Runtime B   Agent Runtime C
       |                |                |
 Agent MCP A        Agent MCP B       Agent MCP C
       |                |                |
       +----------------+----------------+
                        |
                   Tool Broker
                        |
                Policy / Capability
                        |
                 Workspace Runtime
```

The core design principle is **one isolated MCP server instance per agent**. Every agent receives its own identity, runtime, MCP instance, capability manifest, policy, context, memory scope, resources, audit stream, and cancellation lifecycle.

## 1. Per-Agent MCP Runtime

For every agent working in a workspace:

- Create a unique agent ID, runtime ID, and session ID.
- Create an agent-specific MCP server/runtime instance.
- Expose only the MCP tools permitted by that agent's capability policy.
- Maintain an agent-specific tool registry and resource limits.
- Attach an audit stream and cancellation token to the runtime.
- Scope workspace, project, memory, skills, connectors, and files according to policy.
- Never allow an agent to bypass the AWH control plane or directly obtain uncontrolled tool access.

Agent lifecycle states should include:

`CREATED`, `INITIALIZING`, `READY`, `RUNNING`, `WAITING`, `BLOCKED`, `PAUSED`, `CANCELLING`, `STOPPING`, `STOPPED`, `FAILED`, `CRASHED`.

CLI surface:

```text
awh agent create <role>
awh agent list
awh agent start <agent>
awh agent stop <agent>
awh agent pause <agent>
awh agent resume <agent>
awh agent kill <agent>
awh agent inspect <agent>
awh agent logs <agent>
awh agent capabilities <agent>
awh agent revoke <agent>
```

## 2. Capability-Based Security

Introduce explicit capabilities such as:

- `filesystem.read`
- `filesystem.write`
- `filesystem.delete`
- `filesystem.execute`
- `git.read`
- `git.write`
- `git.commit`
- `git.push`
- `shell.execute`
- `network.read`
- `network.write`
- `browser.*`
- connector-specific permissions
- `workspace.admin`
- `agent.spawn`
- `agent.control`

Capabilities must support:

- allow / deny
- conditional allow
- temporary grants
- expiration
- path restrictions
- command restrictions
- domain restrictions
- resource limits
- approval requirements

No agent receives unrestricted workspace access by default.

## 3. Central Tool Broker

Every tool invocation must follow:

```text
Agent
  -> Agent MCP
  -> Tool Broker
  -> Identity verification
  -> Capability verification
  -> Scope validation
  -> Argument/schema validation
  -> Risk classification
  -> Approval check
  -> Resource/rate check
  -> Tool execution
  -> Result sanitization
  -> Audit logging
```

Tool risk classes:

- `READ_ONLY`
- `LOW_RISK`
- `MODERATE`
- `HIGH_RISK`
- `CRITICAL`

Examples: reading a file is read-only; writing a file is low/moderate risk; committing or executing shell commands is higher risk; deleting a workspace is critical.

## 4. Multi-Agent Communication

Implement a durable agent message bus supporting:

- send message
- broadcast
- request/response
- delegation
- handoff
- subscribe/unsubscribe

Support persistence, retries, timeouts, cancellation, tracing, and dead-letter handling.

## 5. Multi-Agent Orchestration

Support:

- sequential workflows
- parallel workflows
- hierarchical teams
- dynamic agent creation
- controlled delegation
- task dependencies
- task DAGs

Agent spawning must be explicitly capability-controlled. Child agents must never automatically inherit unrestricted parent privileges.

## 6. Task DAG and Scheduler

Tasks should contain:

- task ID
- parent task
- dependencies
- children
- assigned agent
- priority
- deadline
- retry policy
- required capabilities
- inputs/outputs
- artifacts
- logs

The scheduler should execute runnable tasks while respecting dependencies, resource limits, policies, priorities, cancellation, and failures.

## 7. Event-Driven Core

Introduce a durable event model covering:

- workspace events
- project events
- agent lifecycle events
- task events
- workflow events
- tool events
- file events
- Git events
- memory events
- approval events
- MCP lifecycle events

Events should carry timestamps, IDs, actor/agent identity, task/workflow context, and trace/request IDs where appropriate.

## 8. Checkpoints and Recovery

Support checkpoints for:

- agents
- tasks
- workflows
- runtimes
- workspace state

Recover from:

- process crashes
- model failures
- network failures
- MCP failures
- tool failures
- machine restarts

Recovery must preserve enough state to safely resume without duplicating destructive actions.

## 9. Layered Memory

Provide memory scopes:

- working memory
- task memory
- project memory
- workspace memory
- agent-private memory
- shared team memory
- episodic memory
- semantic memory

Memory must have explicit scope and access policy: private, project, workspace, or global.

CLI examples:

```text
awh memory search <query>
awh memory get <id>
awh memory list
awh memory inspect <scope>
```

## 10. Context Engine

Context assembly should consider:

1. system/control policy
2. workspace context
3. project context
4. task context
5. agent identity
6. capabilities
7. relevant memory
8. relevant skills
9. relevant events
10. relevant files

Use token-budget-aware prioritization based on relevance, recency, dependency, importance, and cost.

## 11. Skill System 2.0

Skills should declare:

- metadata
- instructions
- tools
- required capabilities
- dependencies
- version
- permissions
- tests
- examples

Support install, uninstall, enable, disable, update, versioning, validation, sandboxing, and permission inspection.

A skill must not silently grant permissions to an agent.

## 12. MCP Ecosystem Manager

Build an MCP lifecycle manager supporting:

- registry
- discovery
- configuration
- health checks
- lifecycle management
- isolation
- permissions
- resource limits
- logs
- restart
- circuit breakers

Each agent's MCP runtime should expose only approved MCP servers and tools.

Expose a machine-readable MCP/tool manifest showing exactly what each agent can access.

## 13. Approval Engine

Approval requests should include:

- agent
- task/workflow
- tool
- arguments
- risk level
- reason
- affected resources
- estimated impact

Support:

```text
awh approvals list
awh approvals approve <id>
awh approvals deny <id>
```

Approval scopes may include once, task, agent, project, permanent, or deny.

## 14. Human-in-the-Loop Modes

Provide:

- autonomous
- supervised
- approval-required
- read-only
- safe-mode
- emergency-stop

Emergency stop must propagate cancellation to active agents, MCP requests, tools, subprocesses, and child agents.

## 15. Resource Governor

Limit:

- CPU
- memory
- disk
- network
- process count
- child-agent count
- runtime duration
- tool-call rate
- parallel tasks
- MCP server count

Resource exhaustion must fail safely and be observable.

## 16. Cancellation, Retry and Failure Handling

Cancellation should propagate through:

```text
Workflow -> Task -> Agent -> MCP request -> Tool -> Child process
```

Implement timeouts, retries, exponential backoff, circuit breakers, and failure classification:

- transient
- rate-limit
- authentication
- permission
- validation
- resource
- bug
- fatal

Model failures should support fallback to another provider/model and resume from checkpoint where safe.

## 17. Model Routing

Introduce a model routing abstraction based on:

- provider
- model
- reasoning level
- context size
- cost
- latency
- availability
- local vs remote
- specialization

Allow model fallback and role-specific model policies.

## 18. Workspace Snapshots

Snapshots should capture relevant state including:

- Git state
- tasks
- agents
- memory
- context
- configuration
- policies
- events
- checkpoints
- artifacts

CLI:

```text
awh snapshot create
awh snapshot list
awh snapshot inspect <id>
awh snapshot restore <id>
```

## 19. Git-Native Agent Development

Use isolated Git worktrees for concurrent agent work.

Support:

- branch
- worktree
- diff
- commit
- stash
- checkpoint
- merge
- conflict handling

Conflicts may be resolved automatically, delegated to a resolver agent, or escalated to the human.

## 20. Artifact and Provenance System

Track artifacts such as:

- files
- patches
- commits
- reports
- logs
- test results
- builds
- screenshots
- datasets

Maintain lineage:

```text
Agent -> Task -> Tool -> File -> Artifact
```

Record WHO, WHAT, WHEN, WHY, model, tool, policy, file, task, and agent for meaningful actions.

## 21. Observability

Provide metrics for:

- agent runtime
- task duration
- workflow duration
- tool/MCP latency
- model latency
- failures
- token usage
- cost
- CPU/memory
- queue depth

CLI:

```text
awh status
awh metrics
awh events
awh trace <id>
awh logs
```

## 22. Connector System

Support connectors such as:

- GitHub
- Google Drive
- Notion
- Slack
- databases
- cloud storage
- browser
- custom APIs

Every connector must have explicit authentication, resource scope, permissions, and risk policy.

## 23. Security Architecture

Security pipeline:

```text
Identity
  -> Authentication
  -> Authorization
  -> Capability
  -> Scope
  -> Schema validation
  -> Sandbox
  -> Resource limits
  -> Audit
```

Threat model must cover:

- prompt injection
- malicious skills/MCP
- tool argument abuse
- path traversal
- symlink escape
- command injection
- secret exfiltration
- unauthorized agent-to-agent access
- privilege escalation
- confused deputy
- capability leakage
- memory poisoning
- malicious workspace files
- compromised connectors
- runaway agents
- fork bombs
- resource exhaustion

## 24. Secret Management

Use secret references such as:

```text
secret://github/token
```

Tools should resolve secrets internally. Models and untrusted agent context should not receive raw secret values unless explicitly and safely required.

## 25. Prompt Injection Defense

Use a strict trust hierarchy:

```text
SYSTEM
> CONTROL
> USER
> AGENT
> SKILL
> WORKSPACE DATA
> EXTERNAL DATA
> TOOL OUTPUT
```

Lower-trust content must never override policy or security controls.

## 26. Policy Engine

Policies should be declarative and composable across:

- global
- workspace
- project
- agent
- task
- tool

The most restrictive applicable policy wins.

Support path, command, network, resource, connector, capability, and approval constraints.

## 27. CLI as the Primary Interface

The CLI comes before the TUI.

Target command groups:

```text
awh workspace
awh project
awh agent
awh task
awh workflow
awh memory
awh context
awh skill
awh mcp
awh tool
awh connector
awh approval
awh policy
awh checkpoint
awh snapshot
awh git
awh event
awh log
awh metrics
awh config
awh doctor
awh status
awh tui
```

Support human-readable and machine-readable output:

- human
- JSON
- JSONL
- YAML
- quiet

Use stable schemas, meaningful exit codes, structured errors, request IDs, trace IDs, agent IDs, and task IDs.

## 28. Unified Error Model

Errors should contain:

- code
- message
- category
- retryable
- cause
- request ID
- agent ID
- task ID
- suggested action

## 29. Doctor and Health System

`awh doctor` should validate:

- installation
- workspace
- configuration
- permissions
- MCP
- connectors
- Git
- filesystem
- sandbox
- runtime dependencies
- state integrity
- security configuration

## 30. Configuration

Use typed configuration with precedence:

```text
defaults
 -> system
 -> user
 -> workspace
 -> project
 -> agent
 -> runtime
 -> CLI override
```

Separate configuration from runtime state, event logs, memory, tasks, agent metadata, audit data, and artifacts. Writes must be atomic and crash-safe.

## 31. Concurrency

The Rust core should use:

- async runtime
- bounded channels
- structured concurrency
- cancellation
- low-contention state
- parallel task execution
- parallel agents

Keep the core lightweight and deterministic where possible.

## 32. Testing

Build:

- unit tests
- integration tests
- security tests
- MCP interoperability tests
- failure/recovery tests
- concurrency tests
- property tests
- policy tests
- path-validation tests
- state-machine tests
- scheduler/DAG tests

Security and isolation tests are mandatory for every capability-sensitive component.

## 33. TUI — After Core and CLI

Only after the core and CLI are solid, build a rich TUI with:

- Dashboard
- Workspaces
- Projects
- Agents
- Agent Detail
- Agent Tools
- Tasks
- Workflow DAG
- Memory
- Context
- Skills
- MCP Servers
- Connectors
- Approvals
- Policies
- Events
- Logs
- Git
- Artifacts
- Checkpoints
- Snapshots
- Settings
- System Health

The TUI should provide live agent status, approval management, agent inspection, event streams, workflow visualization, and operational controls.

## 34. Autonomous High-Level Command

A target command is:

```text
awh run "Build authentication system"
```

AWH should:

1. understand the request
2. inspect the workspace
3. create a plan
4. create appropriate agents
5. assign least-privilege capabilities
6. create isolated runtimes and MCP instances
7. execute tasks
8. coordinate agents
9. request human approval when required
10. modify code
11. test
12. review
13. fix issues
14. checkpoint
15. produce artifacts
16. report results

## 35. Built-In Agent Teams

Software team:

- manager
- architect
- backend
- frontend
- tester
- security
- reviewer

Research team:

- researcher
- fact checker
- analyst
- synthesizer
- reviewer

Debugging team:

- diagnostician
- reproducer
- fixer
- tester
- reviewer

Each role must define instructions, skills, capabilities, model policy, resources, memory policy, and tool policy.

## 36. Controlled Self-Improvement

AWH may propose improvements to:

- skills
- workflows
- policies
- roles
- adapters
- integrations

But changes to the control plane, security policy, or privilege model require explicit human approval.

## 37. Extensibility Interfaces

Design clear Rust traits/interfaces for:

```text
AgentRuntime
AgentRegistry
ToolBroker
CapabilityProvider
PolicyEngine
Scheduler
WorkflowEngine
MemoryStore
EventBus
CheckpointStore
ArtifactStore
ModelProvider
McpRuntime
Connector
Skill
Sandbox
ApprovalProvider
```

## 38. Compatibility

Do not unnecessarily break existing MCP tools or public behavior. When changes are unavoidable, provide migration paths and documentation.

## 39. Documentation

Update documentation for:

- architecture
- agents
- multi-agent orchestration
- capabilities
- policies
- workflows
- memory
- checkpoints
- CLI
- TUI
- connectors
- skills
- development
- testing
- performance
- operations
- migration

## 40. Implementation Strategy

Implement in this order:

### Milestone 1 — Core Runtime

- agent identity
- registry
- lifecycle
- runtime
- per-agent MCP
- capability model
- Tool Broker
- policy engine

### Milestone 2 — Multi-Agent

- messaging
- spawning
- task DAG
- scheduler
- orchestrator
- cancellation
- resources

### Milestone 3 — Reliability

- checkpoints
- recovery
- retries
- failure handling
- events
- audit

### Milestone 4 — AI Context

- memory scopes
- context engine
- skill permissions
- model router

### Milestone 5 — Developer Experience

- CLI
- stable JSON output
- doctor
- status
- workflows

### Milestone 6 — Git and Artifacts

- worktrees
- parallel development
- conflict resolution
- artifacts
- provenance

### Milestone 7 — TUI

Build the TUI only after the preceding layers are production-grade.

## 41. Definition of Done

Every major feature requires:

- implementation
- unit/integration tests
- security tests where applicable
- CLI support
- MCP support where applicable
- documentation
- structured errors
- observability
- migration/compatibility consideration

## 42. Core Security Invariants

These invariants must never be violated:

```text
NO AGENT -> DIRECT UNCONTROLLED TOOL ACCESS
NO AGENT -> AUTOMATIC FULL WORKSPACE ACCESS
NO CHILD AGENT -> AUTOMATIC PARENT PRIVILEGE ESCALATION
NO EXTERNAL DATA -> AUTOMATIC POLICY AUTHORITY
```

## 43. First Engineering Task

Before implementing the target architecture:

1. Inspect the complete `rust` branch.
2. Inspect all existing core modules.
3. Inspect MCP implementation and all existing MCP tools.
4. Inspect security and threat-model documentation.
5. Inspect all tests and workflows.
6. Classify every subsystem as existing, partial, missing, or obsolete.
7. Produce an **AWH Evolution Report** containing:
   - current architecture
   - feature inventory
   - gap analysis
   - security risks
   - performance risks
   - target architecture
   - module plan
   - migration strategy
   - milestones
   - dependency graph
   - testing strategy
   - risk register

Only then begin implementation.

## Final Vision

AWH should become a Rust-native AI workspace operating system where humans remain in control while multiple AI agents can safely collaborate at high autonomy.

The system should combine:

- AI coding-agent capabilities
- persistent workspace state
- isolated per-agent runtimes
- per-agent MCP servers
- least-privilege capability security
- policy and approval control
- multi-agent orchestration
- task DAGs
- memory and context engineering
- skills
- connectors
- Git-native collaboration
- checkpoints and recovery
- artifacts and provenance
- observability
- CLI-first operations
- and eventually a powerful TUI.

The objective is not merely to add features. The objective is to create a coherent, secure, extensible, observable, high-performance AI workspace platform with a strong Rust core and a clear path from one developer agent to large autonomous agent teams.
