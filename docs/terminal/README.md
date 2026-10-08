# Terminal prompt

This directory is reserved for the dedicated implementation prompt(s) for the terminal prompt feature family.

Prompt scope and implementation contracts will be added here separately.

## Lifecycle decision: ephemeral (TRM-001 §6)

AWH terminal execution is **bounded and synchronous**: `terminal run` (CLI,
MCP `terminal.run`, Control API `/terminal/run`, TUI terminal screen)
spawns one argv-only child, waits under a wall-clock timeout (default 30 s,
CLI cap 600 s), captures at most 256 KiB per stream, and returns an
`ExecOutcome`. There is no background-process plane, so AWH tracks **no
long-lived OS processes** and no durable terminal metadata is required.

Consequences, per the TRM-001 §6 allowance for an ephemeral model:

- `awh terminal list` reports an empty live list (`[]`) with an explanatory
  note — it only covers the current process lifetime, which tracks nothing.
- `awh terminal kill <id>` fails deterministically with `unknown execution
  id` for any id — a kill never signals a raw PID, and unknown or
  already-finished ids have stable behavior.
- No restart reconciliation exists because no terminal state survives the
  command; audit history (which IS durable) is owned by the canonical
  `AuditLog`, not a terminal store.

A future background-process plane would need its own prompt-scope decision
registry, execution ids, reconciliation, and retention — none of that is
part of the current contract.

## Security posture

- argv-only: a program name containing whitespace is rejected before
  spawn; arguments are never shell-interpreted.
- Authorization happens before spawn on the agent plane (MCP builtin-trust
  gate + workspace-local deny policy on the program name + capability
  gate); the operator planes (CLI, Control API, TUI) sit behind their
  existing operator trust boundaries (bearer token / local operator).
- Audit records the program name and exit status only — never arguments
  or captured output, which can carry secrets.
- Known limitation: there is no OS-level sandbox around terminal children
  (the MCP sandbox wraps custom MCP server spawns only); the enforced
  controls are authorization, policy, argv validation, timeout, capture
  caps, and audit.