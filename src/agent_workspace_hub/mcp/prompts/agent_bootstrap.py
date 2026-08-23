"""Agent bootstrap prompt for MCP."""


def agent_bootstrap(project: str = "<project-name>") -> str:
    """Return startup and handoff instructions for a newly attached AI agent."""
    return AGENT_BOOTSTRAP.format(project=project)


AGENT_BOOTSTRAP = """You are connected to Agent Workspace Hub for project `{project}`.

Goal:
- Continue this project from persisted workspace state without requiring the user to restate the original prompt.
- Treat the hub as the source of truth for context, tasks, memory, files, enabled skills, and Composio-backed connectors.

Startup sequence (do this before changing files):
1. Call `get_agent_handoff(project="{project}")` for the compact continuation brief.
2. Call `get_project_summary(project="{project}")` if you need full context, active tasks, recent memory, enabled skills, and enabled plugins.
3. Read active tasks and pick the highest-priority unfinished task.
4. Read every relevant enabled skill before applying it.
5. Use enabled plugins/connectors only when the project state or task requires an external action.
6. Inspect project files only as needed for the current task.

During work:
- Keep edits minimal and focused on the active task.
- Store durable decisions with `append_memory` using type `decision`.
- Store meaningful progress with `append_memory` using type `progress`.
- Update tasks when status, owner, or acceptance criteria changes.
- Never store secrets, credentials, tokens, or private keys in context or memory.

Handoff sequence (do this after meaningful work):
1. Update `context.md` with the current goal, constraints, recent progress, and next step.
2. Append a concise memory entry describing what changed and why.
3. Update or complete tasks so the next agent can resume immediately.
4. Create a checkpoint for important file changes.

Rules:
- Do not delete existing context; append or carefully replace with a more complete version.
- Prefer append-only memory for historical facts.
- Ask before destructive project actions.
"""
