"""MCP task tools."""
from __future__ import annotations

from typing import Any

from ...config.settings import get_settings
from ...core import TaskEngine
from ...models.project import Task


def _tasks(project: str) -> TaskEngine:
    return TaskEngine(get_settings().workspace_path, project)


async def create_task(project: str, id: str, title: str, description: str = "",
                      priority: int = 0, assigned_agent: str = "") -> dict[str, Any]:
    """Create a task."""
    task = Task(id=id, title=title, description=description, priority=priority, assigned_agent=assigned_agent)
    _tasks(project).create(task)
    return {"success": True, "task": task.model_dump()}


async def list_tasks(project: str, status: str | None = None,
                     assigned_agent: str | None = None) -> dict[str, Any]:
    """List tasks with optional filters."""
    return {"tasks": [t.model_dump() for t in _tasks(project).list_tasks(status, assigned_agent)]}


async def update_task(project: str, task_id: str, status: str | None = None,
                      notes: str | None = None) -> dict[str, Any]:
    """Update task status or add note."""
    task = _tasks(project).get_task(task_id)
    if status:
        task.status = status
    if notes:
        task.notes = notes
    _tasks(project).update_task(task)
    return {"success": True, "task": task.model_dump()}


async def complete_task(project: str, task_id: str) -> dict[str, Any]:
    """Mark task as done."""
    return await update_task(project, task_id, status="done")


async def assign_task(project: str, task_id: str, assigned_agent: str) -> dict[str, Any]:
    """Assign task to an agent."""
    task = _tasks(project).get_task(task_id)
    task.assigned_agent = assigned_agent
    _tasks(project).update_task(task)
    return {"success": True}
