"""Home screen — Start/Stop server + live logs."""
from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, Header, Footer, Static

from ...config.settings import get_settings
from ...mcp.server import MCPServer
from ..widgets.log_viewer import LogViewer
from ..widgets.server_status import ServerStatus


class HomeScreen(Screen):
    """Main home screen with server controls and log stream."""

    BINDINGS = [
        ("p", "push_screen('projects')", "Projects"),
        ("s", "push_screen('skills')", "Skills"),
        ("l", "push_screen('plugins')", "Plugins"),
        ("a", "push_screen('approvals')", "Approvals"),
        ("q", "quit", "Quit"),
    ]

    def __init__(self, server: MCPServer, **kwargs) -> None:
        super().__init__(**kwargs)
        self.server = server
        self._log_callback = None

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)
        with Vertical(id="home-container"):
            with Horizontal(id="server-panel"):
                yield Static("MCP Server Control", classes="title")
                yield Button("Start", id="btn-start", variant="success")
                yield Button("Stop", id="btn-stop", variant="error")
                yield Button("Restart", id="btn-restart", variant="primary")
                yield ServerStatus(id="server-status")

            with Vertical(id="log-panel"):
                yield Static("Live Logs", classes="title")
                yield LogViewer(id="log-viewer")
        yield Footer()

    def on_mount(self) -> None:
        self._update_status()
        lv = self.query_one(LogViewer)
        self._log_callback = lambda e: self.app.call_from_thread(
            lv.add_log, e.timestamp, e.level, e.category, e.message
        )
        self.server.logs_engine.subscribe(self._log_callback)
        if get_settings().auto_start_server:
            self._start_server()

    def on_unmount(self) -> None:
        if self._log_callback:
            self.server.logs_engine.unsubscribe(self._log_callback)

    def on_button_pressed(self, event: Button.Pressed) -> None:
        btn_id = event.button.id
        if btn_id == "btn-start":
            self._start_server()
        elif btn_id == "btn-stop":
            self._stop_server()
        elif btn_id == "btn-restart":
            self._restart_server()

    async def _start_server(self) -> None:
        self.query_one("#btn-start",Button).disabled = True
        await self.server.start()
        self._update_status()
        self.query_one("#btn-stop", Button).disabled = False

    async def _stop_server(self) -> None:
        await self.server.stop()
        self._update_status()
        self.query_one("#btn-start", Button).disabled = False
        self.query_one("#btn-stop", Button).disabled = True

    async def _restart_server(self) -> None:
        await self.server.stop()
        await self.server.start()
        self._update_status()

    def _update_status(self) -> None:
        status = self.query_one(ServerStatus)
        s = self.server.get_status()
        status.update_status(s["running"], s["host"], s["port"])
        self.query_one("#btn-start", Button).disabled = s["running"]
        self.query_one("#btn-stop", Button).disabled = not s["running"]
