"""Full log viewer screen with filters."""
from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, DataTable, Header, Footer, Input, Select, Static

from ...config.settings import get_settings
from ...core import LogsEngine, Workspace


class LogsScreen(Screen):
    """Structured log viewer with search and filters."""

    BINDINGS = [
        ("escape", "app.pop_screen", "Back"),
        ("r", "refresh", "Refresh"),
        ("c", "clear", "Clear"),
    ]

    def compose(self) -> ComposeResult:
        yield Header()
        with Vertical():
            yield Static("Log Viewer", classes="title")
            with Horizontal():
                yield Input(placeholder="Search logs...", id="search-input")
                yield Select(
                    [("All", None), ("Server", "server"), ("Agent", "agent"),
                     ("Plugin", "plugin"), ("Git", "git"), ("File", "file"),
                     ("Approval", "approval"), ("Error", "error")],
                    id="category-select",
                    prompt="Category",
                )
                yield Select(
                    [("All", None), ("Debug", "debug"), ("Info", "info"),
                     ("Warn", "warn"), ("Error", "error")],
                    id="level-select",
                    prompt="Level",
                )
                yield Button("Refresh", id="btn-refresh")
                yield Button("Clear", id="btn-clear", variant="error")
                yield Button("Export", id="btn-export")
            yield DataTable(id="logs-table")
        yield Footer()

    def on_mount(self) -> None:
        t = self.query_one(DataTable)
        t.add_columns("Timestamp", "Level", "Category", "Project", "Agent", "Message")
        self._load_logs()

    def action_refresh(self) -> None:
        self._load_logs()

    def action_clear(self) -> None:
        engine = LogsEngine(get_settings().workspace_path)
        engine.clear()
        self._load_logs()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-refresh":
            self._load_logs()
        elif event.button.id == "btn-clear":
            self.action_clear()
        elif event.button.id == "btn-export":
            self._export_logs()

    def _load_logs(self) -> None:
        t = self.query_one(DataTable)
        t.clear()

        category = self.query_one("#category-select", Select).value
        level = self.query_one("#level-select", Select).value
        search = self.query_one("#search-input", Input).value

        engine = LogsEngine(get_settings().workspace_path)
        entries = engine.read_logs(
            limit=500,
            category=category if category != Select.BLANK else None,
            level=level if level != Select.BLANK else None,
        )

        for entry in entries:
            msg = entry.message
            if search and search.lower() not in msg.lower():
                continue
            t.add_row(
                entry.timestamp[:19],
                entry.level.upper(),
                entry.category,
                entry.project or "-",
                entry.agent or "-",
                msg[:80],
            )

    def _export_logs(self) -> None:
        import datetime
        dest = get_settings().workspace_path / "global" / "logs" / f"export_{datetime.datetime.now().strftime('%Y%m%d_%H%M%S')}.json"
        engine = LogsEngine(get_settings().workspace_path)
        entries = engine.read_logs(limit=10000)
        import json
        with open(dest, "w") as f:
            for e in entries:
                f.write(json.dumps(e.model_dump()) + "\n")
        self.notify(f"Exported to {dest}")
