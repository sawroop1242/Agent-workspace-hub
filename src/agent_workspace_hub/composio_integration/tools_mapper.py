"""Maps Composio tools to MCP-compatible tool definitions."""
from __future__ import annotations

from typing import Any


class ToolsMapper:
    """Converts Composio tool schemas to FastMCP tool signatures."""

    @staticmethod
    def to_mcp_tool_def(tool: dict[str, Any]) -> dict[str, Any]:
        """Convert Composio tool to MCP tool definition."""
        return {
            "name": f"composio_{tool.get('name', 'unknown')}",
            "description": tool.get("description", ""),
            "inputSchema": tool.get("input", {"type": "object", "properties": {}}),
        }

    @staticmethod
    def extract_action_name(mcp_tool_name: str) -> str:
        """Strip composio_ prefix to get original action name."""
        if mcp_tool_name.startswith("composio_"):
            return mcp_tool_name[9:]
        return mcp_tool_name
