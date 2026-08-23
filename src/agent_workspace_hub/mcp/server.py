"""FastMCP 2.0 server setup and lifecycle."""
from __future__ import annotations

import asyncio
from pathlib import Path

from fastmcp import FastMCP

from ..config.settings import get_settings
from ..core import (
    ApprovalsEngine,
    ContextEngine,
    FileEngine,
    GitEngine,
    LogsEngine,
    MemoryEngine,
    PluginsEngine,
    ProjectEngine,
    SkillsEngine,
    TaskEngine,
    Workspace,
)
from ..models.approval import ApprovalRequest
from ..models.project import Checkpoint, Project, Task
from ..models.skill import SkillManifest
from .prompts.agent_bootstrap import agent_bootstrap
from .tools import (
    approvals,
    context,
    files,
    git_tools,
    logs,
    memory,
    plugins,
    projects,
    skills,
    tasks,
)

mcp = FastMCP("AgentWorkspaceHub")


# --- Register prompts and tools ---

mcp.prompt()(agent_bootstrap)


# Projects
mcp.tool()(projects.create_project)
mcp.tool()(projects.list_projects)
mcp.tool()(projects.get_project_summary)
mcp.tool()(projects.get_agent_handoff)
mcp.tool()(projects.open_project)
mcp.tool()(projects.delete_project)

# Context
mcp.tool()(context.read_context)
mcp.tool()(context.update_context)

# Memory
mcp.tool()(memory.append_memory)
mcp.tool()(memory.read_memory)
mcp.tool()(memory.search_memory)

# Files
mcp.tool()(files.list_files)
mcp.tool()(files.read_file)
mcp.tool()(files.save_file)
mcp.tool()(files.create_folder)
mcp.tool()(files.rename_file)
mcp.tool()(files.delete_file)
mcp.tool()(files.search_files)

# Tasks
mcp.tool()(tasks.create_task)
mcp.tool()(tasks.list_tasks)
mcp.tool()(tasks.update_task)
mcp.tool()(tasks.complete_task)
mcp.tool()(tasks.assign_task)

# Skills
mcp.tool()(skills.list_skills)
mcp.tool()(skills.read_skill)
mcp.tool()(skills.install_skill)
mcp.tool()(skills.uninstall_skill)
mcp.tool()(skills.enable_project_skill)
mcp.tool()(skills.disable_project_skill)

# Plugins (Composio)
mcp.tool()(plugins.list_plugins)
mcp.tool()(plugins.invoke_plugin_action)

# Git
mcp.tool()(git_tools.git_status)
mcp.tool()(git_tools.create_checkpoint)
mcp.tool()(git_tools.list_checkpoints)
mcp.tool()(git_tools.restore_checkpoint)

# Approvals
mcp.tool()(approvals.list_pending_approvals)
mcp.tool()(approvals.approve_action)
mcp.tool()(approvals.reject_action)

# Logs
mcp.tool()(logs.read_logs)


class MCPServer:
    """Manages the FastMCP server lifecycle."""

    def __init__(self, host: str = "127.0.0.1", port: int = 8765) -> None:
        self.host = host
        self.port = port
        self._task: asyncio.Task | None = None
        self._workspace = Workspace()
        self._workspace.ensure_structure()
        self.logs_engine = LogsEngine(self._workspace.root)

    async def start(self) -> None:
        """Start the MCP server via SSE transport."""
        self.logs_engine.log("Starting FastMCP server", level="info", category="server")
        # FastMCP 2.0 uses sse_server() for async serving
        self._task = asyncio.create_task(mcp.sse_server(host=self.host, port=self.port))

    async def stop(self) -> None:
        """Stop the MCP server."""
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass
            self._task = None
        self.logs_engine.log("FastMCP server stopped", level="info", category="server")

    @property
    def is_running(self) -> bool:
        return self._task is not None and not self._task.done()

    def get_status(self) -> dict:
        return {
            "running": self.is_running,
            "host": self.host,
            "port": self.port,
            "url": f"http://{self.host}:{self.port}/sse" if self.is_running else None,
        }
