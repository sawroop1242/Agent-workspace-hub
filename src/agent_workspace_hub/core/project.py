"""Project CRUD and workspace generation."""
from __future__ import annotations

import json
import shutil
from datetime import datetime
from pathlib import Path
from typing import Any

from ..config.constants import (
    AGENT_DIR,
    AGENT_MD,
    CHECKPOINTS_DIR,
    CONTEXT_MD,
    DECISIONS_DIR,
    LOGS_DIR,
    MEMORY_JSONL,
    PLUGINS_JSON,
    PROJECT_JSON,
    README_MD,
    RULES_DIR,
    SKILLS_DIR,
    SKILLS_INDEX_JSON,
    TASKS_DIR,
)
from ..models.project import Project
from .workspace import Workspace


AGENT_MD_TEMPLATE = """# Agent Workspace Hub — Project Instructions

You are connected to Agent Workspace Hub.

Before doing any work:
1. Call `get_project_summary`.
2. Read active tasks.
3. Read relevant skills.
4. Read relevant decisions.
5. Only modify required files.

After doing work:
1. Update context.
2. Append memory.
3. Update tasks.
4. Create checkpoint if important.

Rules:
- Do not delete context.
- Do not store secrets.
- Prefer append-only memory.
- Ask before destructive actions.
"""


class ProjectEngine:
    """Handles project creation, deletion, and metadata."""

    def __init__(self, workspace: Workspace | None = None) -> None:
        self.workspace = workspace or Workspace()
        self.workspace.ensure_structure()

    def create(self, name: str, description: str = "", git_init: bool = True) -> Project:
        """Create a new project with full .agent structure."""
        project_path = self.workspace.project_path(name)
        if project_path.exists():
            raise FileExistsError(f"Project '{name}' already exists")

        agent_dir = project_path / AGENT_DIR

        # Create directories
        for subdir in [
            agent_dir,
            agent_dir / SKILLS_DIR,
            agent_dir / TASKS_DIR,
            agent_dir / DECISIONS_DIR,
            agent_dir / RULES_DIR,
            agent_dir / LOGS_DIR,
            agent_dir / CHECKPOINTS_DIR,
            project_path / "files",
            project_path / "src",
            project_path / "docs",
            project_path / "tests",
        ]:
            subdir.mkdir(parents=True)

        # Write AGENT.md
        (project_path / AGENT_MD).write_text(AGENT_MD_TEMPLATE)

        # Write README.md
        (project_path / README_MD).write_text(f"# {name}\n\n{description}\n")

        # Write project.json
        project = Project(
            id=name.lower().replace(" ", "-"),
            name=name,
            description=description,
            path=str(project_path.relative_to(self.workspace.root)),
            git_enabled=git_init,
        )
        (agent_dir / PROJECT_JSON).write_text(project.model_dump_json(indent=2))

        # Write context.md
        (agent_dir / CONTEXT_MD).write_text(
            f"# Context: {name}\n\n## Goal\n{description}\n\n## Current Task\n\n## Tech Stack\n\n## Constraints\n\n## Recent Progress\n\n## Important Notes\n"
        )

        # Write memory.jsonl (empty)
        (agent_dir / MEMORY_JSONL).write_text("")

        # Write skills-index.json
        (agent_dir / SKILLS_INDEX_JSON).write_text(json.dumps({"enabled": []}, indent=2))

        # Write plugins.json
        (agent_dir / PLUGINS_JSON).write_text(json.dumps({"enabled": []}, indent=2))

        # Git init
        if git_init:
            import git
            git.Repo.init(project_path)

        return project

    def list_projects(self) -> list[Project]:
        """List all projects."""
        projects = []
        if not self.workspace.projects_dir.exists():
            return projects
        for entry in self.workspace.projects_dir.iterdir():
            if entry.is_dir():
                pj = entry / AGENT_DIR / PROJECT_JSON
                if pj.exists():
                    try:
                        projects.append(Project(**json.loads(pj.read_text())))
                    except Exception:
                        continue
        return sorted(projects, key=lambda p: p.updated_at, reverse=True)

    def get_project(self, name: str) -> Project:
        """Load project metadata."""
        pj = self.workspace.project_path(name) / AGENT_DIR / PROJECT_JSON
        if not pj.exists():
            raise FileNotFoundError(f"Project '{name}' not found")
        return Project(**json.loads(pj.read_text()))

    def update_project(self, project: Project) -> None:
        """Save updated project metadata."""
        project.updated_at = datetime.utcnow().isoformat()
        pj = self.workspace.project_path(project.name) / AGENT_DIR / PROJECT_JSON
        pj.write_text(project.model_dump_json(indent=2))

    def delete_project(self, name: str, archive: bool = True) -> None:
        """Delete or archive a project."""
        src = self.workspace.project_path(name)
        if not src.exists():
            raise FileNotFoundError(f"Project '{name}' not found")
        if archive:
            dst = self.workspace.root / "archive" / f"{name}_{datetime.utcnow().isoformat()}"
            shutil.move(str(src), str(dst))
        else:
            shutil.rmtree(src)

    def get_summary(self, name: str) -> dict[str, Any]:
        """Compact project summary for AI agents."""
        project = self.get_project(name)
        agent_dir = self.workspace.project_path(name) / AGENT_DIR

        context = ""
        if (agent_dir / CONTEXT_MD).exists():
            context = (agent_dir / CONTEXT_MD).read_text()

        # Read active tasks
        tasks = []
        tasks_dir = agent_dir / TASKS_DIR
        if tasks_dir.exists():
            for tf in sorted(tasks_dir.glob("*.json")):
                tasks.append(json.loads(tf.read_text()))

        # Read recent memory (last 20 lines)
        memory = []
        mem_file = agent_dir / MEMORY_JSONL
        if mem_file.exists():
            lines = mem_file.read_text().strip().splitlines()
            for line in lines[-20:]:
                if line.strip():
                    memory.append(json.loads(line))

        # Read enabled skills
        skills = []
        si = agent_dir / SKILLS_INDEX_JSON
        if si.exists():
            skills = json.loads(si.read_text()).get("enabled", [])

        # Read enabled plugins
        plugins = []
        pi = agent_dir / PLUGINS_JSON
        if pi.exists():
            plugins = json.loads(pi.read_text()).get("enabled", [])

        return {
            "project": project.model_dump(),
            "context": context,
            "active_tasks": [t for t in tasks if t.get("status") != "done"],
            "recent_memory": memory,
            "enabled_skills": skills,
            "enabled_plugins": plugins,
  }
