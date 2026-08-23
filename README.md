# Agent-workspace-hub

## One-line install

Install Agent Workspace Hub with one command:

```bash
curl -fsSL https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/main/scripts/install.sh | bash
```

Install directly from Git source instead of the latest release wheel:

```bash
curl -fsSL https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/main/scripts/install.sh | bash -s -- --source source
```

Install with optional Composio connector dependencies:

```bash
curl -fsSL https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/main/scripts/install.sh | bash -s -- --with-composio
```

The installer requires Python 3.11+, prefers `pipx` for an isolated CLI install when
available, falls back to `python -m pip install --user`, and prints PATH setup guidance
after installation.



## Agent handoff workflow

Agent Workspace Hub is designed as an MCP server that preserves enough project state
for a different AI agent to continue work without a new bootstrap prompt. Each project
keeps durable state in `.agent/context.md`, `.agent/memory.jsonl`, task files, enabled
skills, and enabled Composio-backed plugins.

Recommended new-agent startup:

1. Call `get_agent_handoff(project)` to receive the compact continuation brief.
2. Call `get_project_summary(project)` when full context, active tasks, recent memory,
   enabled skills, and enabled plugins are needed.
3. Read relevant skills before applying project-specific procedures.
4. Use Composio plugin actions only when the enabled connector is required by the
   active task.
5. After meaningful work, update context, append memory, update tasks, and create a
   checkpoint when appropriate.

This loop makes context transfer explicit: the outgoing agent records decisions and
progress, and the incoming agent starts from the persisted handoff state instead of
asking the user to repeat the project idea.

| File                                   | Purpose                                     |
| -------------------------------------- | ------------------------------------------- |
| `pyproject.toml`                       | Package config, deps, CLI entry point `awh` |
| `requirements.txt`                     | Core dependencies                           |
| `.gitignore`                           | Git ignore rules                            |
| `config/constants.py`                  | App constants, paths, security patterns     |
| `config/settings.py`                   | Settings with OS keychain vault             |
| `models/*.py`                          | Pydantic models for all entities            |
| `core/workspace.py`                    | Workspace root management                   |
| `core/project.py`                      | Project CRUD + AGENT.md template            |
| `core/context.py`                      | context.md read/update                      |
| `core/memory.py`                       | Append-only memory.jsonl                    |
| `core/files.py`                        | Safe file ops with path guards              |
| `core/tasks.py`                        | Task CRUD                                   |
| `core/skills_engine.py`                | Global + project skill management           |
| `core/plugins_engine.py`               | Plugin enable/disable                       |
| `core/git_engine.py`                   | Git init, commit, diff, checkpoints         |
| `core/approvals.py`                    | Approval queue                              |
| `core/logs.py`                         | Structured logging with streaming           |
| `composio_integration/client.py`       | Composio API wrapper                        |
| `composio_integration/tools_mapper.py` | Composio → MCP mapping                      |
| `skill_hub/client.py`                  | skill\_hub.ai API client                    |
| `skill_hub/search.py`                  | Search helper                               |
| `mcp/server.py`                        | FastMCP server + lifecycle                  |
| `mcp/tools/*.py`                       | MCP tools, including project handoff state  |
| `mcp/prompts/agent_bootstrap.py`       | Agent bootstrap prompt                      |
| `tui/app.py`                           | Main Textual App                            |
| `tui/screens/home.py`                  | Start/Stop + live logs                      |
| `tui/screens/projects.py`              | Project list                                |
| `tui/screens/project_detail.py`        | Context, tasks, memory tabs                 |
| `tui/screens/files.py`                 | File tree + editor                          |
| `tui/screens/skills.py`                | Installed + skill\_hub.ai search            |
| `tui/screens/plugins.py`               | Composio tools manager                      |
| `tui/screens/git_screen.py`            | Git status, diff, checkpoints               |
| `tui/screens/approvals.py`             | Approval center                             |
| `tui/screens/logs_screen.py`           | Log viewer with filters                     |
| `tui/screens/settings.py`              | Composio key, workspace, config             |
| `tui/widgets/*.py`                     | Reusable widgets                            |
| `tui/styles/app.tcss`                  | Textual CSS                                 |
| `__main__.py`                          | Entry point                                 |
| `scripts/install.sh`                   | Installation script                         |
| `README.md`                            | Full documentation                          |
| `docs/USAGE.md`                        | Usage guide                                 |
