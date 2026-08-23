"""MCP context tools."""
from __future__ import annotations

from typing import Any

from ...config.settings import get_settings
from ...core import ContextEngine


def _ctx(project: str) -> ContextEngine:
    return ContextEngine(get_settings().workspace_path, project)


async def read_context(project: str) -> dict[str, Any]:
    """Read current project context."""
    return {"context": _ctx(project).read()}


async def update_context(project: str, content: str, mode: str = "replace") -> dict[str, Any]:
    """Update project context (replace or append)."""
    _ctx(project).update(content, mode)
    return {"success": True}
