"""skill_hub.ai API client."""
from __future__ import annotations

from typing import Any

import httpx


class SkillHubClient:
    """Client for skill_hub.ai API."""

    BASE_URL = "https://api.skill_hub.ai/v1"  # Placeholder — adjust to real endpoint

    def __init__(self, api_key: str | None = None) -> None:
        self.api_key = api_key
        self._client = httpx.AsyncClient(base_url=self.BASE_URL, timeout=15.0)

    async def search(self, query: str, limit: int = 20) -> list[dict[str, Any]]:
        """Search for skills on skill_hub.ai."""
        try:
            headers = {}
            if self.api_key:
                headers["Authorization"] = f"Bearer {self.api_key}"
            resp = await self._client.get(
                "/skills/search",
                params={"q": query, "limit": limit},
                headers=headers,
            )
            resp.raise_for_status()
            return resp.json().get("results", [])
        except Exception as e:
            return [{"error": str(e)}]

    async def get_skill(self, skill_id: str) -> dict[str, Any] | None:
        """Fetch a single skill's details and content."""
        try:
            headers = {}
            if self.api_key:
                headers["Authorization"] = f"Bearer {self.api_key}"
            resp = await self._client.get(f"/skills/{skill_id}", headers=headers)
            resp.raise_for_status()
            return resp.json()
        except Exception:
            return None
