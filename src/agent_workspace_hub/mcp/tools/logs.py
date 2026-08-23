"""MCP log tools."""
from __future__ import annotations

from typing import Any

from ...config.settings import get_settings
from ...core import LogsEngine, Workspace


def _logs() -> LogsEngine:
    return LogsEngine(get_settings().workspace_path)


async def read_logs(limit: int = 100, category: str | None = None,
                    level: str | None = None, project: str | None = None) -> dict[str, Any]:
    """Read structured logs."""
    return {"entries": [e.model_dump() for e in _logs().read_logs(limit, category, level, project)]}
