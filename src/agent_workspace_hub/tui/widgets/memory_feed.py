"""Memory timeline widget."""
from textual.widgets import RichLog


class MemoryFeed(RichLog):
    """Display memory entries as a timeline."""

    def __init__(self, **kwargs) -> None:
        super().__init__(highlight=True, markup=True, **kwargs)

    def add_entry(self, timestamp: str, type_: str, agent: str, message: str) -> None:
        color = {
            "note": "cyan",
            "decision": "yellow",
            "task_update": "green",
            "file_change": "blue",
            "plugin_action": "magenta",
            "checkpoint": "green bold",
            "error": "red bold",
            "approval": "orange",
        }.get(type_, "white")
        self.write(f"[{timestamp}] [{color}]{type_}[/{color}] {agent or 'system'}: {message}")
