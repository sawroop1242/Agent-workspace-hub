"""Plugin/Composio tool card."""
from textual.widgets import Static


class PluginCard(Static):
    """Display a plugin/tool card."""

    def __init__(self, name: str, description: str = "", status: str = "", **kwargs) -> None:
        super().__init__(**kwargs)
        self.plugin_name = name
        self.description = description
        self.status = status

    def compose(self):
        from textual.app import ComposeResult
        yield Static(f"[bold]{self.plugin_name}[/bold] [{self.status}]")
        yield Static(f"[dim]{self.description[:60]}[/dim]")
