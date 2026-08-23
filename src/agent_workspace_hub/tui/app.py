"""Main Textual TUI application."""
from __future__ import annotations

from textual.app import App
from textual.binding import Binding

from ...config.settings import get_settings
from ...mcp.server import MCPServer
from .screens.approvals import ApprovalsScreen
from .screens.files import FilesScreen
from .screens.git_screen import GitScreen
from .screens.home import HomeScreen
from .screens.logs_screen import LogsScreen
from .screens.plugins import PluginsScreen
from .screens.project_detail import ProjectDetailScreen
from .screens.projects import ProjectsScreen
from .screens.settings import SettingsScreen
from .screens.skills import SkillsScreen


class AgentWorkspaceHubApp(App):
    """Agent Workspace Hub TUI Application."""

    CSS_PATH = "styles/app.tcss"
    BINDINGS = [
        Binding("ctrl+c", "quit", "Quit", show=True),
        Binding("ctrl+d", "toggle_dark", "Dark Mode"),
    ]

    SCREENS = {
        "home": HomeScreen,
        "projects": ProjectsScreen,
        "skills": SkillsScreen,
        "plugins": PluginsScreen,
        "approvals": ApprovalsScreen,
        "settings": SettingsScreen,
        "logs": LogsScreen,
    }

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        settings = get_settings()
        self.server = MCPServer(
            host=settings.server_host,
            port=settings.server_port,
        )

    def on_mount(self) -> None:
        self.push_screen(HomeScreen(self.server))

    def action_toggle_dark(self) -> None:
        self.dark = not self.dark

    def get_server(self) -> MCPServer:
        return self.server
