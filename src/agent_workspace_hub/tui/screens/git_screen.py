"""Git backup screen with status, diff, and checkpoints."""
from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, DataTable, Header, Footer, Input, Static, TextArea

from ...config.settings import get_settings
from ...core import GitEngine, Workspace


class GitScreen(Screen):
    """Git status, diff viewer, and checkpoint manager."""

    BINDINGS = [
        ("escape", "app.pop_screen", "Back"),
        ("r", "refresh", "Refresh"),
    ]

    def __init__(self, project_name: str, **kwargs) -> None:
        super().__init__(**kwargs)
        self.project_name = project_name

    def compose(self) -> ComposeResult:
        yield Header()
        with Vertical():
            yield Static(f"Git: {self.project_name}", classes="title")
            with Horizontal():
                yield Button("Status", id="btn-status")
                yield Button("Checkpoint", id="btn-checkpoint", variant="success")
                yield Button("Push", id="btn-push")
                yield Button("Pull", id="btn-pull")
            yield Static("Changed Files:")
            yield DataTable(id="git-status-table")
            yield Static("Diff:")
            yield TextArea(id="diff-viewer", read_only=True)
            yield Static("Checkpoints:")
            yield DataTable(id="checkpoints-table")
        yield Footer()

    def on_mount(self) -> None:
        self.query_one("#git-status-table", DataTable).add_columns("File", "Status")
        self.query_one("#checkpoints-table", DataTable).add_columns("ID", "Message", "Agent", "Hash", "Date")
        self._load_status()
        self._load_checkpoints()

    def action_refresh(self) -> None:
        self._load_status()
        self._load_checkpoints()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-status":
            self._load_status()
        elif event.button.id == "btn-checkpoint":
            self.app.push_screen("checkpoint_create")
        elif event.button.id == "btn-push":
            self._do_push()
        elif event.button.id == "btn-pull":
            self._do_pull()

    def _load_status(self) -> None:
        t = self.query_one("#git-status-table", DataTable)
        t.clear()
        try:
            git = GitEngine(get_settings().workspace_path, self.project_name)
            status = git.status()
            for f in status.get("changed", []):
                t.add_row(f, "modified")
            for f in status.get("untracked", []):
                t.add_row(f, "untracked")
            for f in status.get("staged", []):
                t.add_row(f, "staged")
        except Exception as e:
            t.add_row(str(e), "error")

    def _load_checkpoints(self) -> None:
        t = self.query_one("#checkpoints-table", DataTable)
        t.clear()
        try:
            git = GitEngine(get_settings().workspace_path, self.project_name)
            for cp in git.list_checkpoints():
                t.add_row(cp.id, cp.message, cp.agent, cp.git_commit_hash[:8], cp.created_at[:10])
        except Exception as e:
            t.add_row("-", str(e), "-", "-", "-")

    def _do_push(self) -> None:
        try:
            git = GitEngine(get_settings().workspace_path, self.project_name)
            result = git.push()
            self.notify(result)
        except Exception as e:
            self.notify(f"Push failed: {e}", severity="error")

    def _do_pull(self) -> None:
        try:
            git = GitEngine(get_settings().workspace_path, self.project_name)
            result = git.pull()
            self.notify(result)
        except Exception as e:
            self.notify(f"Pull failed: {e}", severity="error")
