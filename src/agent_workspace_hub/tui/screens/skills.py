"""Skills screen — installed + skill_hub.ai search."""
from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, DataTable, Header, Footer, Input, TabbedContent, TabPane

from ...config.settings import get_settings
from ...core import SkillsEngine, Workspace
from ...skill_hub.search import search_skills


class SkillsScreen(Screen):
    """Manage skills and search skill_hub.ai."""

    BINDINGS = [("escape", "app.pop_screen", "Back")]

    def compose(self) -> ComposeResult:
        yield Header()
        with TabbedContent():
            with TabPane("Installed", id="tab-installed"):
                yield DataTable(id="installed-table")
            with TabPane("skill_hub.ai", id="tab-online"):
                with Vertical():
                    with Horizontal():
                        yield Input(placeholder="Search skill_hub.ai...", id="search-input")
                        yield Button("Search", id="btn-search")
                    yield DataTable(id="online-table")
        yield Footer()

    def on_mount(self) -> None:
        it = self.query_one("#installed-table",DataTable)
        it.add_columns("ID", "Name", "Version", "Risk")
        se = SkillsEngine(get_settings().workspace_path)
        for s in se.list_global_skills():
            it.add_row(s.id, s.name, s.version, s.risk_level)

        ot = self.query_one("#online-table",DataTable)
        ot.add_columns("ID", "Name", "Description", "Install")

    async def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-search":
            query = self.query_one("#search-input",Input).value
            if query:
                results = await search_skills(query)
                ot = self.query_one("#online-table",DataTable)
                ot.clear()
                for r in results:
                    if "error" in r:
                        continue
                    ot.add_row(r.get("id", ""), r.get("name", ""), r.get("description", "")[:40], "Install")
