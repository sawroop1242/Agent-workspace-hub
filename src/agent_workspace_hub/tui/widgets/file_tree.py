"""File tree widget."""
from textual.widgets import Tree


class FileTree(Tree):
    """Display project file tree."""

    def __init__(self, label: str = "Files", **kwargs) -> None:
        super().__init__(label, **kwargs)
