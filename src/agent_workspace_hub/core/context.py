"""Context (context.md) management."""
from __future__ import annotations

from pathlib import Path

from ..config.constants import AGENT_DIR, CONTEXT_MD


class ContextEngine:
    def __init__(self, workspace_root: Path, project_name: str) -> None:
        self.path = workspace_root / "projects" / project_name / AGENT_DIR / CONTEXT_MD

    def read(self) -> str:
        if not self.path.exists():
            return ""
        return self.path.read_text()

    def update(self, content: str, mode: str = "replace") -> None:
        if mode == "append":
            current = self.read()
            content = current + "\n\n" + content
        self.path.write_text(content)
