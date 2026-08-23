"""Project detail screen — context, tasks, memory, files overview."""
from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, DataTable, Header, Footer, Static, TabbedContent, TabPane, TextArea

from ...config.settings import get_settings
from ...core import ContextEngine, MemoryEngine, ProjectEngine, TaskEngine, Workspace


class ProjectDetailScreen(Screen):
    """Detailed project view with tabs."""

    BINDINGS = [
        ("escape", "app.pop_screen", "Back"),
        ("g", "git", "Git"),
        ("f", "files", "Files"),
    ]

    def __init__(self, project_name: str, **kwargs) -> None:
        super().__init__(**kwargs)
        self.project_name = project_name

    def compose(self) -> ComposeResult:
        yield Header()
        with Vertical():
            yield Static(f"Project: {self.project_name}", classes="title")
            with Horizontal():
                yield Button("Git", id="btn-git")
                yield Button("Files", id="btn-files")
                yield Button("Refresh", id="btn-refresh")

            with TabbedContent():
                with TabPane("Context", id="tab-context"):
                    yield TextArea(id="context-editor")
                    yield Button("Save Context", id="btn-save-context", variant="success")
                with TabPane("Tasks", id="tab-tasks"):
                    yield DataTable(id="tasks-table")
                    yield Button("New Task", id="btn-new-task")
                with TabPane("Memory", id="tab-memory"):
                    yield DataTable(id="memory-table")
                    yield Button("Add Memory", id="btn-add-memory")
        yield Footer()

    def on_mount(self) -> None:
        self.query_one("#tasks-table",DataTable).add_columns("ID", "Title", "Status", "Priority", "Assigned")
        self.query_one("#memory-table",DataTable).add_columns("Time", "Type", "Agent", "Message")
        self._load_all()

    def _load_all(self) -> None:
        self._load_context()
        self._load_tasks()
        self._load_memory()

    def _load_context(self) -> None:
        try:
            ctx = ContextEngine(get_settings().workspace_path, self.project_name)
            self.query_one("#context-editor", TextArea).text = ctx.read()
        except Exception:
            pass

    def _load_tasks(self) -> None:
        t = self.query_one("#tasks-table",DataTable)
        t.clear()
        try:
            te = TaskEngine(get_settings().workspace_path, self.project_name)
            for task in te.list_tasks():
                t.add_row(task.id, task.title, task.status, str(task.priority), task.assigned_agent)
        except Exception:
            pass

    def _load_memory(self) -> None:
        t = self.query_one("#memory-table",DataTable)
        t.clear()
        try:
            mem = MemoryEngine(get_settings().workspace_path, self.project_name)
            for entry in mem.read(limit=50):
                t.add_row(
                    entry.get("timestamp", "")[:19],
                    entry.get("type", ""),
                    entry.get("agent", "") or "-",
                    entry.get("message", "")[:50],
                )
        except Exception:
            pass

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-git":
            self.app.push_screen(GitScreen(self.project_name))
        elif event.button.id == "btn-files":
            self.app.push_screen(FilesScreen(self.project_name))
        elif event.button.id == "btn-refresh":
            self._load_all()
        elif event.button.id == "btn-save-context":
            text = self.query_one("#context-editor", TextArea).text
            ctx = ContextEngine(get_settings().workspace_path, self.project_name)
            ctx.update(text)
            self.notify("Context saved.")

    def action_git(self) -> None:
        self.app.push_screen(GitScreen(self.project_name))

    def action_files(self) -> None:
        self.app.push_screen(FilesScreen(self.project_name))
