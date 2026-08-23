"""Diff viewer widget."""
from textual.widgets import TextArea


class DiffViewer(TextArea):
    """Display unified diff."""

    def __init__(self, **kwargs) -> None:
        super().__init__(read_only=True, **kwargs)

    def set_diff(self, diff_text: str) -> None:
        self.text = diff_text
