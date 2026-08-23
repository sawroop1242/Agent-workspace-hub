"""Agent bootstrap prompt for MCP."""

AGENT_BOOTSTRAP = """You are connected to Agent Workspace Hub.

Before doing any work:
1. Call get_project_summary.
2. Read active tasks.
3. Read relevant skills.
4. Read relevant decisions.
5. Only modify required files.

After doing work:
1. Update context.
2. Append memory.
3. Update tasks.
4. Create checkpoint if important.

Rules:
- Do not delete context.
- Do not store secrets.
- Prefer append-only memory.
- Ask before destructive actions.
"""
