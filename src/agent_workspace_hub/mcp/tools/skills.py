"""MCP skill tools."""
from __future__ import annotations

from typing import Any

from ...config.settings import get_settings
from ...core import SkillsEngine, Workspace
from ...models.skill import SkillManifest


def _skills() -> SkillsEngine:
    return SkillsEngine(get_settings().workspace_path)


async def list_skills(project: str) -> dict[str, Any]:
    """List available skills for a project."""
    se = _skills()
    return {
        "global": [s.model_dump() for s in se.list_global_skills()],
        "project": [s.model_dump() for s in se.list_project_skills(project)],
    }


async def read_skill(skill_id: str, source: str = "global") -> dict[str, Any]:
    """Read skill content."""
    return {"content": _skills().read_skill(skill_id, source)}


async def install_skill(source: str, skill_id: str, name: str, description: str = "",
                        content: str = "", version: str = "1.0.0") -> dict[str, Any]:
    """Install a skill into the global library."""
    manifest = SkillManifest(id=skill_id, name=name, description=description, version=version)
    _skills().install_global_skill(manifest, content)
    return {"success": True}


async def uninstall_skill(skill_id: str) -> dict[str, Any]:
    """Remove a global skill."""
    _skills().uninstall_global_skill(skill_id)
    return {"success": True}


async def enable_project_skill(project: str, skill_id: str, source: str = "global") -> dict[str, Any]:
    """Enable a skill for a project."""
    _skills().enable_project_skill(project, skill_id, source)
    return {"success": True}


async def disable_project_skill(project: str, skill_id: str, source: str = "global") -> dict[str, Any]:
    """Disable a skill for a project."""
    _skills().disable_project_skill(project, skill_id, source)
    return {"success": True}
