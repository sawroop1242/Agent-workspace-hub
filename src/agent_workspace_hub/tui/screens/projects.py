"""Projects list screen."""
from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, DataTable, Header, Footer, Input

from ...config.settings import get_settings
from ...core import ProjectEngine, Workspace


class ProjectsScreen(Screen):
    """Manage projects."""

    BINDINGS = [("escape", "app.pop_screen", "Back"), ("c", "create", "Create")]

    def compose(self) -> ComposeResult:
        yield Header()
        with Vertical():
            with Horizontal():
                yield Input(placeholder="Search projects...", id="search-input")
                yield Button("Create", id="btn-create", variant="success")
                yield Button("Delete", id="btn-delete", variant="error")
            yield DataTable(id="projects-table")
        yield Footer()

    def on_mount(self) -> None:
        table = self.query_one(DataTable)
        table.add_columns("Name", "Description", "Updated", "Git")
        self._load_projects()

    def _load_projects(self) -> None:
        table = self.query_one(DataTable)
        table.clear()
        engine = ProjectEngine(Workspace(get_settings().workspace_path))
        for p in engine.list_projects():
            table.add_row(p.name, p.description[:40], p.updated_at[:10], "Y" if p.git_enabled else "")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-create":
            self.app.push_screen("project_create")
        elif event.button.id == "btn-delete":
            self._delete_selected()

    def _delete_selected(self) -> None:
        table = self.query_one(DataTable)
        if table.cursor_row is not None:
            name = table.get_row_at(table.cursor_row)[0]
            engine = ProjectEngine(Workspace(get_settings().workspace_path))
            engine.delete_project(name, archive=True)
            self._load_projects()
