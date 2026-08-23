# Agent-workspace-hub

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
| `mcp/tools/*.py`                       | All MCP tool implementations                |
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
