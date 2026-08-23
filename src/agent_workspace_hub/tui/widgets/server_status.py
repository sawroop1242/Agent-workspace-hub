"""Server status indicator widget."""
from textual.widgets import Static


class ServerStatus(Static):
    """Shows MCP server running state."""

    def update_status(self, running: bool, host: str = "127.0.0.1", port: int = 8765) -> None:
        if running:
            self.update(f"RUNNING @ http://{host}:{port}/sse")
        else:
            self.update("STOPPED")
