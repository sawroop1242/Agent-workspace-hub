"""MCP Git tools."""
from __future__ import annotations

from typing import Any

from ...config.settings import get_settings
from ...core import GitEngine


def _git(project: str) -> GitEngine:
    return GitEngine(get_settings().workspace_path, project)


async def git_status(project: str) -> dict[str, Any]:
    """Show changed files."""
    return _git(project).status()


async def create_checkpoint(project: str, message: str, agent: str = "",
                            reason: str = "") -> dict[str, Any]:
    """Create Git commit / snapshot."""
    cp = _git(project).create_checkpoint(message, agent, reason)
    return {"success": True, "checkpoint": cp.model_dump()}


async def list_checkpoints(project: str) -> dict[str, Any]:
    """List checkpoints."""
    return {"checkpoints": [c.model_dump() for c in _git(project).list_checkpoints()]}


async def restore_checkpoint(project: str, commit_hash: str, confirm: bool = False) -> dict[str, Any]:
    """Restore project to previous checkpoint."""
    if not confirm:
        return {"success": False, "error": "Confirmation required"}
    _git(project).restore_checkpoint(commit_hash)
    return {"success": True}
