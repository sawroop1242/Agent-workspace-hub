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


async def get_agent_handoff(project: str) -> dict[str, Any]:
    """Return a continuation brief that lets a new AI agent resume without a fresh prompt."""
    summary = _engine().get_summary(project)
    active_tasks = summary["active_tasks"]
    recent_memory = summary["recent_memory"]
    enabled_skills = summary["enabled_skills"]
    enabled_plugins = summary["enabled_plugins"]

    next_task = active_tasks[0] if active_tasks else None

    return {
        "project": summary["project"],
        "handoff_brief": {
            "purpose": (
                "Resume this project from persisted Agent Workspace Hub state: "
                "context, tasks, memory, files, skills, and Composio-backed plugins."
            ),
            "startup_steps": [
                "Read the project context and active tasks before editing files.",
                "Use recent memory to recover decisions, progress, and blockers.",
                "Read enabled skills before applying project-specific procedures.",
                "Use enabled plugins/connectors only when the current task needs external actions.",
                "Update context, memory, and tasks after meaningful progress so the next agent can continue.",
            ],
            "next_task": next_task,
            "counts": {
                "active_tasks": len(active_tasks),
                "recent_memory_entries": len(recent_memory),
                "enabled_skills": len(enabled_skills),
                "enabled_plugins": len(enabled_plugins),
            },
        },
        "context": summary["context"],
        "active_tasks": active_tasks,
        "recent_memory": recent_memory,
        "enabled_skills": enabled_skills,
        "enabled_plugins": enabled_plugins,
    }


async def delete_project(project: str, confirm: bool = False) -> dict[str, Any]:
    """Delete a project safely (archives by default)."""
    if not confirm:
        return {"success": False, "error": "Confirmation required"}
    _engine().delete_project(project, archive=True)
    return {"success": True}
