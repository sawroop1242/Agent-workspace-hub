"""Task management within a project."""
from __future__ import annotations

import json
from pathlib import Path

from ..config.constants import AGENT_DIR, TASKS_DIR
from ..models.project import Task


class TaskEngine:
    def __init__(self, workspace_root: Path, project_name: str) -> None:
        self.tasks_dir = workspace_root / "projects" / project_name / AGENT_DIR / TASKS_DIR
        self.tasks_dir.mkdir(parents=True, exist_ok=True)

    def _task_path(self, task_id: str) -> Path:
        return self.tasks_dir / f"{task_id}.json"

    def create(self, task: Task) -> Task:
        self._task_path(task.id).write_text(task.model_dump_json(indent=2))
        return task

    def list_tasks(self, status: str | None = None, assigned_agent: str | None = None) -> list[Task]:
        tasks = []
        for f in self.tasks_dir.glob("*.json"):
            try:
                t = Task(**json.loads(f.read_text()))
                if status and t.status != status:
                    continue
                if assigned_agent and t.assigned_agent != assigned_agent:
                    continue
                tasks.append(t)
            except Exception:
                continue
        return tasks

    def get_task(self, task_id: str) -> Task:
        p = self._task_path(task_id)
        if not p.exists():
            raise FileNotFoundError(f"Task {task_id} not found")
        return Task(**json.loads(p.read_text()))

    def update_task(self, task: Task) -> Task:
        from datetime import datetime
        task.updated_at = datetime.utcnow().isoformat()
        self._task_path(task.id).write_text(task.model_dump_json(indent=2))
        return task

    def delete_task(self, task_id: str) -> None:
        p = self._task_path(task_id)
        if p.exists():
            p.unlink()
