"""Project summary card widget."""
from textual.widgets import Static


class ProjectCard(Static):
    """Display a project summary card."""

    def __init__(self, name: str, description: str = "", updated: str = "", **kwargs) -> None:
        super().__init__(**kwargs)
        self.name = name
        self.description = description
        self.updated = updated

    def compose(self):
        from textual.app import ComposeResult
        yield Static(f"[bold]{self.name}[/bold]")
        yield Static(f"[dim]{self.description[:60]}[/dim]")
        yield Static(f"[dim]Updated: {self.updated[:10]}[/dim]")
