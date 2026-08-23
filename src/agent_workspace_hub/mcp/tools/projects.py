"""MCP project tools."""
from __future__ import annotations

from typing import Any

from ...config.settings import get_settings
from ...core import ProjectEngine, Workspace
from ...models.project import Project


def _engine() -> ProjectEngine:
    return ProjectEngine(Workspace(get_settings().workspace_path))


async def create_project(name: str, description: str = "", template: str | None = None) -> dict[str, Any]:
    """Create a new project workspace."""
    project = _engine().create(name, description)
    return {"success": True, "project": project.model_dump()}


async def list_projects() -> dict[str, Any]:
    """List all available projects."""
    projects = _engine().list_projects()
    return {"projects": [p.model_dump() for p in projects]}


async def get_project_summary(project: str) -> dict[str, Any]:
    """Get compact project summary for AI agents. Call this first."""
    return _engine().get_summary(project)


async def open_project(project: str) -> dict[str, Any]:
    """Return detailed metadata for a selected project."""
    p = _engine().get_project(project)
    summary = _engine().get_summary(project)
    return {"project": p.model_dump(), "summary": summary}


async def delete_project(project: str, confirm: bool = False) -> dict[str, Any]:
    """Delete a project safely (archives by default)."""
    if not confirm:
        return {"success": False, "error": "Confirmation required"}
    _engine().delete_project(project, archive=True)
    return {"success": True}
