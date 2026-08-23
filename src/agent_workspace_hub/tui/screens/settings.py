"""Settings screen — Composio key, workspace root, server config."""
from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, Header, Footer, Input, Static, Switch

from ...config.settings import Settings, get_settings


class SettingsScreen(Screen):
    """Configure app settings."""

    BINDINGS = [("escape", "app.pop_screen", "Back"), ("s", "save", "Save")]

    def compose(self) -> ComposeResult:
        yield Header()
        with Vertical():
            yield Static("Settings", classes="title")

            yield Static("Workspace Root:")
            yield Input(id="input-workspace", placeholder="Path to workspace root")

            yield Static("Server Host:")
            yield Input(id="input-host", placeholder="127.0.0.1")

            yield Static("Server Port:")
            yield Input(id="input-port", placeholder="8765")

            yield Static("Composio API Key:")
            yield Input(id="input-composio", placeholder="Enter Composio API key", password=True)

            yield Static("Log Level:")
            yield Input(id="input-loglevel", placeholder="info | debug | warn | error")

            with Horizontal():
                yield Static("Auto-start server:")
                yield Switch(id="switch-autostart")

            with Horizontal():
                yield Static("Git auto-init:")
                yield Switch(id="switch-git")

            with Horizontal():
                yield Button("Save", id="btn-save", variant="success")
                yield Button("Clear Composio Key", id="btn-clear-composio", variant="warning")
        yield Footer()

    def on_mount(self) -> None:
        s = get_settings()
        self.query_one("#input-workspace", Input).value = s.workspace_root
        self.query_one("#input-host", Input).value = s.server_host
        self.query_one("#input-port", Input).value = str(s.server_port)
        self.query_one("#input-loglevel", Input).value = s.log_level
        self.query_one("#switch-autostart", Switch).value = s.auto_start_server
        self.query_one("#switch-git", Switch).value = s.git_auto_init

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-save":
            self._save_settings()
        elif event.button.id == "btn-clear-composio":
            get_settings().clear_composio_api_key()
            self.query_one("#input-composio", Input).value = ""
            self.notify("Composio API key cleared.")

    def _save_settings(self) -> None:
        s = get_settings()
        s.workspace_root = self.query_one("#input-workspace", Input).value
        s.server_host = self.query_one("#input-host", Input).value
        s.server_port = int(self.query_one("#input-port", Input).value or "8765")
        s.log_level = self.query_one("#input-loglevel", Input).value
        s.auto_start_server = self.query_one("#switch-autostart", Switch).value
        s.git_auto_init = self.query_one("#switch-git", Switch).value

        composio_key = self.query_one("#input-composio", Input).value
        if composio_key:
            s.set_composio_api_key(composio_key)

        s.save()
        self.notify("Settings saved successfully.")
