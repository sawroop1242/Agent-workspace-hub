"""Auto-scrolling log viewer widget."""
from __future__ import annotations

from textual.widgets import RichLog


class LogViewer(RichLog):
    """Displays live logs with auto-scroll."""

    def __init__(self, **kwargs) -> None:
        super().__init__(highlight=True, markup=True, **kwargs)

    def add_log(self, timestamp: str, level: str, category: str, message: str) -> None:
        """Add a formatted log line."""
        color = {
            "debug": "dim",
            "info": "cyan",
            "warn": "yellow",
            "error": "red bold",
        }.get(level, "white")
        line = f"[{timestamp}] [{color}]{level.upper()}[/{color}] [{category}] {message}"
        self.write(line)
        self.scroll_end(animate=False)
