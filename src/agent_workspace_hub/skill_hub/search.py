"""Search helper for skill_hub.ai."""
from __future__ import annotations

from typing import Any

from .client import SkillHubClient


async def search_skills(query: str, limit: int = 20, api_key: str | None = None) -> list[dict[str, Any]]:
    """Convenience function to search skills."""
    client = SkillHubClient(api_key=api_key)
    return await client.search(query, limit)
