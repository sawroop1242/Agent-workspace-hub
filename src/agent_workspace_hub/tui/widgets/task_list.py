"""Task list widget."""
from textual.widgets import DataTable


class TaskList(DataTable):
    """Display project tasks."""

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self.add_columns("ID", "Title", "Status", "Priority", "Assigned")
