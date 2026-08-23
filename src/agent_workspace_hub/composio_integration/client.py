"""Composio API client wrapper."""
from __future__ import annotations

from typing import Any

import httpx

from ..config.settings import get_settings


class ComposioClient:
    """Wraps Composio API to list and execute tools."""

    BASE_URL = "https://backend.composio.dev/api/v1"

    def __init__(self, api_key: str | None = None) -> None:
        self.api_key = api_key or get_settings().get_composio_api_key()
        self._client = httpx.AsyncClient(
            base_url=self.BASE_URL,
            headers={"x-api-key": self.api_key or ""},
            timeout=30.0,
        )

    async def list_tools(self) -> list[dict[str, Any]]:
        """Fetch available tools from Composio."""
        if not self.api_key:
            return []
        try:
            resp = await self._client.get("/tools")
            resp.raise_for_status()
            data = resp.json()
            return data.get("items", [])
        except Exception as e:
            return [{"error": str(e)}]

    async def execute_action(self, action: str, params: dict[str, Any]) -> dict[str, Any]:
        """Execute a Composio action."""
        if not self.api_key:
            return {"error": "Composio API key not configured"}
        try:
            resp = await self._client.post(
                "/actions/execute",
                json={"action": action, "params": params},
            )
            resp.raise_for_status()
            return resp.json()
        except Exception as e:
            return {"error": str(e)}

    def is_configured(self) -> bool:
        return self.api_key is not None and len(self.api_key) > 0
