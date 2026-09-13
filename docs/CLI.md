# AWH Final CLI Reference

> This is the canonical **target CLI contract**. It describes the final product surface, not a claim that every command is currently implemented.
>
> Implementation must follow `docs/PROJECT_ROADMAP.md` and use shared AWH application services.

## Interface rule

```text
CLI ─┐
MCP ─┤
TUI ─┼→ AWH application services → core/runtime
API ─┘
```

The CLI must not contain an independent implementation of filesystem, Git, editing, policy, snapshots, sessions, or other domain behavior.

## Command tree

```text
awh
├── init
├── version
├── status
├── doctor
├── config
├── agent
│   ├── list
│   ├── show
│   ├── start
│   ├── stop
│   ├── restart
│   ├── run
│   └── status
├── mcp
│   ├── serve
│   ├── list
│   ├── add
│   ├── remove
│   ├── inspect
│   ├── test
│   └── logs
├── workspace
│   ├── create
│   ├── list
│   ├── open
│   ├── info
│   └── remove
├── fs
│   ├── read
│   ├── write
│   ├── stat
│   ├── search
│   ├── patch
│   ├── replace
│   ├── insert
│   ├── delete-range
│   ├── apply-diff
│   ├── hash
│   ├── verify
│   ├── history
│   └── rollback
├── git
│   ├── status
│   ├── diff
│   ├── staged-diff
│   ├── log
│   ├── branch
│   ├── branches
│   ├── worktree
│   ├── stage
│   ├── unstage
│   ├── commit
│   ├── push
│   ├── pull
│   ├── reset
│   ├── clean
│   └── validate
├── worktree
│   ├── create
│   ├── list
│   ├── inspect
│   ├── remove
│   ├── merge
│   └── status
├── capability
│   ├── list
│   ├── show
│   ├── grant
│   ├── revoke
│   └── check
├── policy
│   ├── list
│   ├── show
│   ├── check
│   ├── validate
│   └── explain
├── snapshot
│   ├── create
│   ├── list
│   ├── show
│   ├── restore
│   ├── delete
│   └── diff
├── context
│   ├── show
│   ├── save
│   ├── update
│   ├── clear
│   └── search
├── memory
│   ├── list
│   ├── get
│   ├── search
│   ├── add
│   ├── update
│   └── delete
├── skill
│   ├── list
│   ├── show
│   ├── install
│   ├── remove
│   ├── enable
│   └── disable
├── session
│   ├── list
│   ├── show
│   ├── create
│   ├── stop
│   └── status
├── task
│   ├── list
│   ├── show
│   ├── create
│   ├── update
│   ├── cancel
│   └── assign
├── audit
│   ├── list
│   ├── show
│   ├── search
│   └── export
├── logs
│   ├── show
│   ├── follow
│   └── clear
├── terminal
│   ├── run
│   ├── list
│   └── kill
├── connector
│   ├── list
│   ├── add
│   ├── remove
│   ├── inspect
│   ├── test
│   └── invoke
├── collaboration
│   ├── agents
│   ├── status
│   ├── handoff
│   ├── assign
│   ├── conflicts
│   └── events
├── api
│   ├── serve
│   ├── status
│   ├── tokens
│   └── logs
├── tui
└── completion
    ├── bash
    ├── zsh
    ├── fish
    └── powershell
```

## Command-to-phase map

| Command family | Primary phase | Dependencies |
|---|---:|---|
| root/init/version/status/doctor/config | 0 | CLI, configuration, state |
| `mcp *` | 1 | MCP server, transport, registry |
| `workspace *` | 2 | workspace/filesystem |
| basic `fs *` | 2 | secure filesystem service |
| edit `fs *` | 2/5 | EditTransaction, EditService, policy |
| `git *` | 3 | workspace, Git service, policy |
| `worktree *` | 3 | Git + workspace isolation |
| `capability *` | 4 | capability model |
| `policy *` | 4 | PolicyEngine |
| `snapshot *` | 5 | edit transactions, provenance |
| `context *` | 6 | workspace/session/context engine |
| `memory *` | 7 | scoped memory store |
| `skill *` | 8 | registry, policy, capabilities |
| `agent *` | 1/4/9 | profile, registry, policy, server/session |
| `session *` | 9 | agent + workspace + policy |
| `task *` | 9 | session + agent ownership |
| `audit *` / `logs *` | 10 | events/tracing |
| `terminal *` | 10/16 | policy, capability, session, resource limits |
| `collaboration *` | 12 | agents, sessions, worktrees, tasks, events |
| `api *` | 13 | application services, auth, policy |
| `tui` | 11/14 | service/backend abstraction |
| `connector *` | 15 | adapter registry, auth, policy, audit |
| `completion *` | 0/15 | stable CLI definition |

## Implementation order

### P0 — Core

1. CLI framework
2. configuration
3. `init`, `version`, `status`, `doctor`
4. workspace service and `workspace *`
5. secure filesystem primitives
6. `fs read/stat/search/hash`

### P1 — Agent-grade editing

7. `EditTransaction`
8. `fs replace`
9. `fs insert`
10. `fs delete-range`
11. `fs patch`
12. `fs apply-diff`
13. conflict detection
14. atomic apply/rollback
15. post-edit verification
16. `fs history`
17. `fs rollback`

### P2 — MCP + security

18. `mcp serve`
19. tool/resource registry
20. `mcp inspect/test/list`
21. capability model
22. PolicyEngine
23. AgentProfile/AgentRegistry
24. `agent list/show/start/stop/restart/run/status`
25. namespaced MCP routes
26. policy-enforced discovery and invocation

### P3 — Git isolation

27. read-only Git commands
28. guarded Git mutations
29. worktree service
30. `worktree *`
31. agent/session/worktree association

### P4 — Reversibility + observability

32. snapshots
33. provenance
34. audit events
35. logs
36. guarded restore/rollback

### P5 — Runtime state

37. sessions
38. tasks
39. context
40. memory
41. skills

### P6 — High-risk runtime

42. terminal execution
43. connector adapters/invocation
44. resource limits and audit validation

### P7 — Multi-agent/control UX

45. collaboration
46. Control API
47. TUI
48. ecosystem integrations
49. completion generation

## Security ordering

Do not expose a dangerous mutation before its enforcement layer exists.

```text
Capability
  ↓
Policy
  ↓
Session identity
  ↓
Audit
  ↓
Mutation
```

Applies especially to:

- `git push`
- `git reset`
- `git clean`
- `snapshot restore`
- `terminal run`
- `capability grant`
- `capability revoke`
- `connector invoke`

## Agent-specific MCP

Configured agents use:

```text
/{agent}/mcp
/{agent}/sse
```

For example:

```bash
awh agent start claude
```

activates only Claude's configured routes. `awh agent start --all` activates every enabled profile.

The route is never authorization. The request must still resolve:

```text
agent → session → workspace → capability → policy → tool → service
```

## Validation rule

Every final command must eventually pass:

```text
compile
→ unit tests
→ integration tests
→ real terminal test
→ failure/recovery test
→ documentation check
```

MCP commands also require real-client interoperability testing where practical.
