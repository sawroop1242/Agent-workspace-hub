"""Workspace root management — creates and validates the global directory structure."""
from __future__ import annotations

from pathlib import Path

from ..config.constants import (
    ARCHIVE_DIR,
    GLOBAL_LOGS_DIR,
    GLOBAL_PLUGINS_DIR,
    GLOBAL_REGISTRY_DIR,
    GLOBAL_SKILLS_DIR,
    GLOBAL_VAULT_DIR,
    PROJECTS_DIR,
)
from ..config.settings import get_settings


class Workspace:
    """Manages the workspace root directory and its substructure."""

    def __init__(self, root: Path | None = None) -> None:
        self.root = root or get_settings().workspace_path

    def ensure_structure(self) -> None:
        """Create all required global directories."""
        dirs = [
            self.root / GLOBAL_SKILLS_DIR,
            self.root / GLOBAL_PLUGINS_DIR,
            self.root / GLOBAL_REGISTRY_DIR,
            self.root / GLOBAL_LOGS_DIR,
            self.root / GLOBAL_VAULT_DIR,
            self.root / PROJECTS_DIR,
            self.root / ARCHIVE_DIR,
        ]
        for d in dirs:
            d.mkdir(parents=True, exist_ok=True)

        # Ensure registry files exist
        skills_registry = self.root / GLOBAL_REGISTRY_DIR / "skills.json"
        plugins_registry = self.root / GLOBAL_REGISTRY_DIR / "plugins.json"
        if not skills_registry.exists():
            skills_registry.write_text("[]")
        if not plugins_registry.exists():
            plugins_registry.write_text("[]")

    @property
    def projects_dir(self) -> Path:
        return self.root / PROJECTS_DIR

    @property
    def global_skills_dir(self) -> Path:
        return self.root / GLOBAL_SKILLS_DIR

    @property
    def global_plugins_dir(self) -> Path:
        return self.root / GLOBAL_PLUGINS_DIR

    def project_path(self, name: str) -> Path:
        return self.projects_dir / name
