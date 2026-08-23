"""MCP plugin tools — routes through Composio."""
from __future__ import annotations

from typing import Any

from ...composio_integration.client import ComposioClient
from ...config.settings import get_settings
from ...core import PluginsEngine
from ...utils.security import redact_secrets


def _plugins() -> PluginsEngine:
    return PluginsEngine(get_settings().workspace_path)


async def list_plugins(project: str) -> dict[str, Any]:
    """List installed and available plugins."""
    pe = _plugins()
    return {
        "global": [p.model_dump() for p in pe.list_global_plugins()],
        "project": [p.model_dump() for p in pe.list_project_plugins(project)],
    }


async def invoke_plugin_action(project: str, plugin_id: str, action: str,
                               params: dict[str, Any]) -> dict[str, Any]:
    """Execute a plugin action via Composio."""
    # Check if plugin is enabled for project
    pe = _plugins()
    enabled = [p.id for p in pe.list_project_plugins(project)]
    if plugin_id not in enabled:
        return {"success": False, "error": f"Plugin {plugin_id} not enabled for project"}

    client = ComposioClient()
    if not client.is_configured():
        return {"success": False, "error": "Composio API key not configured. Add it in Settings."}

    result = await client.execute_action(action, params)
    return {"success": True, "result": result}
