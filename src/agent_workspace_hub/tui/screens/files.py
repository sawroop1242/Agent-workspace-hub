"""File manager screen with tree and editor."""
from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, DirectoryTree, Header, Footer, Input, Static, TextArea

from ...config.settings import get_settings
from ...core import FileEngine


class FilesScreen(Screen):
    """File tree and editor for a project."""

    BINDINGS = [
        ("escape", "app.pop_screen", "Back"),
        ("s", "save", "Save"),
    ]

    def __init__(self, project_name: str, **kwargs) -> None:
        super().__init__(**kwargs)
        self.project_name = project_name
        self.current_file: str | None = None

    def compose(self) -> ComposeResult:
        yield Header()
        with Vertical():
            yield Static(f"Files: {self.project_name}", classes="title")
            with Horizontal():
                yield Input(placeholder="File path...", id="file-path")
                yield Button("Open", id="btn-open")
                yield Button("Save", id="btn-save", variant="success")
                yield Button("New File", id="btn-new")
                yield Button("Delete", id="btn-delete", variant="error")
            with Horizontal():
                with Vertical():
                    yield Static("Files")
                    yield TextArea(id="file-list", read_only=True, show_line_numbers=False)
                with Vertical():
                    yield Static("Editor")
                    yield TextArea(id="file-editor")
        yield Footer()

    def on_mount(self) -> None:
        self._load_file_list()

    def _load_file_list(self) -> None:
        try:
            fe = FileEngine(get_settings().workspace_path, self.project_name)
            items = fe.list_files("")
            lines = []
            for item in items:
                prefix = "📁" if item["is_dir"] else "📄"
                lines.append(f"{prefix} {item['name']}")
            self.query_one("#file-list", TextArea).text = "\n".join(lines)
        except Exception as e:
            self.query_one("#file-list", TextArea).text = f"Error: {e}"

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-open":
            self._open_file()
        elif event.button.id == "btn-save":
            self._save_file()
        elif event.button.id == "btn-new":
            self._new_file()
        elif event.button.id == "btn-delete":
            self._delete_file()

    def _open_file(self) -> None:
        path = self.query_one("#file-path", Input).value
        if not path:
            return
        try:
            fe = FileEngine(get_settings().workspace_path, self.project_name)
            content = fe.read_file(path)
            self.query_one("#file-editor", TextArea).text = content
            self.current_file = path
        except Exception as e:
            self.notify(f"Error: {e}", severity="error")

    def _save_file(self) -> None:
        path = self.query_one("#file-path", Input).value
        if not path:
            return
        try:
            fe = FileEngine(get_settings().workspace_path, self.project_name)
            content = self.query_one("#file-editor", TextArea).text
            fe.save_file(path, content)
            self.notify("File saved.")
            self._load_file_list()
        except Exception as e:
            self.notify(f"Error: {e}", severity="error")

    def _new_file(self) -> None:
        path = self.query_one("#file-path", Input).value
        if not path:
            return
        try:
            fe = FileEngine(get_settings().workspace_path, self.project_name)
            fe.save_file(path, "")
            self.query_one("#file-editor", TextArea).text = ""
            self.current_file = path
            self._load_file_list()
            self.notify("File created.")
        except Exception as e:
            self.notify(f"Error: {e}", severity="error")

    def _delete_file(self) -> None:
        path = self.query_one("#file-path", Input).value
        if not path:
            return
        try:
            fe = FileEngine(get_settings().workspace_path, self.project_name)
            fe.delete_file(path)
            self.query_one("#file-editor", TextArea).text = ""
            self.current_file = None
            self._load_file_list()
            self.notify("File deleted.")
        except Exception as e:
            self.notify(f"Error: {e}", severity="error")

    def action_save(self) -> None:
        self._save_file()
