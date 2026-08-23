"""MCP memory tools."""
from __future__ import annotations

from typing import Any

from ...config.settings import get_settings
from ...core import MemoryEngine


def _mem(project: str) -> MemoryEngine:
    return MemoryEngine(get_settings().workspace_path, project)


async def append_memory(project: str, message: str, type: str, agent: str = "",
                        metadata: dict[str, Any] | None = None) -> dict[str, Any]:
    """Add an event to memory.jsonl."""
    _mem(project).append(message, type, agent, metadata)
    return {"success": True}


async def read_memory(project: str, limit: int = 50, type_filter: str | None = None) -> dict[str, Any]:
    """Read recent memory events."""
    return {"entries": _mem(project).read(limit, type_filter)}


async def search_memory(project: str, query: str, limit: int = 50) -> dict[str, Any]:
    """Search memory events."""
    return {"entries": _mem(project).search(query, limit)}
