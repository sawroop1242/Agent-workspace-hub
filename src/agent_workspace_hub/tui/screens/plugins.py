"""Plugins screen — Composio tools manager."""
from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, DataTable, Header, Footer, Static

from ...composio_integration.client import ComposioClient
from ...config.settings import get_settings


class PluginsScreen(Screen):
    """Manage Composio plugins/tools."""

    BINDINGS = [("escape", "app.pop_screen", "Back")]

    def compose(self) -> ComposeResult:
        yield Header()
        with Vertical():
            yield Static("Composio Plugin Manager", classes="title")
            with Horizontal():
                yield Button("Refresh", id="btn-refresh")
                yield Button("Settings", id="btn-settings")
            yield DataTable(id="plugins-table")
        yield Footer()

    def on_mount(self) -> None:
        t = self.query_one(DataTable)
        t.add_columns("Tool Name", "Description", "Status")
        self._load_tools()

    async def _load_tools(self) -> None:
        t = self.query_one(DataTable)
        t.clear()
        client = ComposioClient()
        if not client.is_configured():
            t.add_row("Not configured", "Add Composio API key in Settings", "X")
            return
        tools = await client.list_tools()
        for tool in tools:
            if "error" in tool:
                continue
            t.add_row(tool.get("name", ""), tool.get("description", "")[:50], "OK")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-refresh":
            self._load_tools()
        elif event.button.id == "btn-settings":
            self.app.push_screen("settings")
