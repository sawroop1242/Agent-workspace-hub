"""Skill display card."""
from textual.widgets import Static


class SkillCard(Static):
    """Display a skill card."""

    def __init__(self, skill_id: str, name: str, version: str = "", description: str = "", **kwargs) -> None:
        super().__init__(**kwargs)
        self.skill_id = skill_id
        self.name = name
        self.version = version
        self.description = description

    def compose(self):
        from textual.app import ComposeResult
        yield Static(f"[bold]{self.name}[/bold] [dim]v{self.version}[/dim]")
        yield Static(f"[dim]{self.description[:60]}[/dim]")
