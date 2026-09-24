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
│   ├── create
│   ├── list
│   ├── show
│   ├── start [--all | ids]
│   ├── stop
│   ├── restart
│   ├── run
│   ├── enable
│   ├── disable
│   ├── status
│   └── session
│       ├── open
│       ├── list [--agent]
│       ├── show
│       ├── resolve
│       ├── pause
│       ├── resume
│       └── stop
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

## `awh init`

Initializes a directory as an AWH workspace. Implemented on the `rust`
branch (TW-001).

```text
awh init [--path <dir>]   # default --path .
```

Behavior:

- fresh root: creates `.agent/workspace.json` (the durable workspace
  manifest: `version`, `workspace_id`, `workspace_root`, `created_at`) and
  initializes `.agent/policy.json` in its valid empty state; prints
  `initialized workspace <canonical-root>` and `workspace id: <workspace id>`;
- already initialized: loads and reports the existing manifest unchanged
  (`workspace already initialized —`, same workspace id). Re-init is
  idempotent: no new identity, no rewritten manifest, no reset of agents,
  grants, policy rules, or other persisted state;
- no implicit authority: init never creates agent records, never activates
  an agent, and never grants capabilities. Activation and grants remain
  explicit (`awh agent —`);
- fails closed: a root that is a file, a corrupt manifest, an unsupported
  manifest version, or a manifest recorded for a different root are
  structured errors (non-zero exit), and persisted bytes are left
  untouched — never silently re-initialized;
- concurrency: the manifest write is atomic (temp file + fsync + rename)
  and guarded by the same cross-process `StoreLock` mechanism as the other
  `.agent` stores, so two concurrent `awh init` invocations converge on one
  identity instead of racing;
- the `--path` root is canonicalized before use, so `awh init --path ws`
  creates state only under `ws/` and never in the process CWD.

## `awh agent` — agent runtime identity (TW-002)

Registers and manages agent profiles and their AWH-native runtime
sessions. Implemented on the `rust` branch (TW-002). Every surface (CLI,
MCP, TUI, Control API) must use the shared `AgentRuntimeService` — the
CLI handlers are thin; no duplicate lifecycle logic exists in the CLI.

```text
awh agent create <id> <name> --role <role>
awh agent list
awh agent show <id>
awh agent start <ids> | --all      # ids and --all are mutually exclusive
awh agent stop <id>
awh agent restart <id>
awh agent enable <id>
awh agent disable <id>
awh agent status

awh agent session open <agent-id>
awh agent session list [--agent <id>]
awh agent session show <session-id>
awh agent session resolve <agent-id> <session-id>
awh agent session pause <agent-id> <session-id>
awh agent session resume <agent-id> <session-id>
awh agent session stop <agent-id> <session-id>
```

### Profile rules

- `create` registers a fresh profile (id, name, role validated and
  bounded; the profile carries no capabilities); a duplicate id is an
  explicit error — a registry never overwrites an existing identity;
- `start` activates a profile (`created|stopped|paused|failed → active`);
  `--all` starts every *enabled* profile instead of named ones, and ids
  plus `--all` in one invocation is a usage error (the operator's intent
  must not be silently broadened). Disabled profiles fail closed: an
  inactive or disabled agent never silently becomes active;
- `stop`/`restart` are deliberate operator actions; `restart` is stop +
  start and always results in an `active` profile. Stopping or disabling
  an agent does not mutate or unload its session records — it only makes
  session resolution fail closed;
- `enable`/`disable` toggle the profile's resolution switch;
- `status` reports per-agent lifecycle (`status:`, `enabled:`) and how
  many of its sessions are currently usable (active or paused).

### Session rules (TW-002 §7–§10)

- a session is the AWH-native runtime identity. It is NOT the MCP
  protocol session (transport lifecycle) and the two are never conflated;
- `open` requires an initialized workspace and an enabled, active agent;
  the session binds one agent to exactly one workspace. Session ids are
  unique per open (`sess-<nanos>-<pid>-<seq>`; the agent/workspace binding
  lives in the session record, not the id);
- `resolve` answers "which caller is this?" only — it never grants
  capabilities and is never authorization. It re-validates everything on
  every call: session exists, belongs to the claiming agent, is usable
  (active or paused), is bound to the current workspace, and the agent
  profile is still enabled and active. A persisted record is never
  trusted merely because it exists;
- lifecycle transitions go through the validated table:

```text
Active  → Paused, Stopped, Failed
Paused  → Active (resume), Stopped, Failed
Stopped → (terminal: no transitions)
Failed  → (terminal: no transitions)
```

- `stopped` and `failed` are terminal: no pause, no resume, no resolve.
  Reopening a stopped or failed caller requires opening a new session —
  nothing is ever silently reactivated;
- terminal transitions (`stop`, and `Failed` via the runtime) validate
  ownership and workspace binding but do not require an active profile:
  a stopped or disabled agent can still *retire* its sessions. Gating
  teardown on an active profile would make cleanup unreachable after
  `agent stop` / `agent disable`. Extending *use* (resume) still requires
  the full resolution contract;
- session records persist under `.agent/sessions/` as per-session JSON
  (atomic tempfile + fsync + rename under the cross-process `StoreLock`),
  for correlation and audit. Listing is best-effort: a record that fails
  to parse is skipped so one torn file cannot brick `agent status`,
  `session list`, or `session show` workspace-wide; targeted access
  (`session show <id>`, resolve) fails loudly, naming the damaged file.

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
| `agent session *` (TW-002 subset) | 9 | `AgentRuntimeService`, session store, workspace manifest |
| `session *` (full) | 9 | agent + workspace + policy |
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
