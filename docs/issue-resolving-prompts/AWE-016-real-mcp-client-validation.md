# AWE-016 / #37 — Real MCP Client Validation

## Task
Validate agent-grade editing against real MCP clients after the core service and MCP tools are complete.

## Target clients
OpenCode, OpenHands, Claude Code, Qwen Code, Codex, plus a generic MCP client where practical.

## Workflow
`connect → initialize → tools/list → read → minimal patch → result → verify → conflict → rollback`.

## Acceptance
- real connection and session initialization
- edit tools discovered correctly
- minimal patch works without full-file rewrite
- structured conflict/policy errors survive transport
- rollback works
- stale-read conflict is demonstrated
- Android/Termux compatibility is documented/tested where supported

## Constraint
Do not add client-specific logic to AWH to make a test pass. Interop failures should identify a protocol/schema/contract problem.
