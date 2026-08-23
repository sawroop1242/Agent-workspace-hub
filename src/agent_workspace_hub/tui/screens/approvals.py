"""Approvals center screen."""
from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, Header, Footer, Static

from ...config.settings import get_settings
from ...core import ApprovalsEngine, Workspace
from ...models.approval import ApprovalRequest


class ApprovalCard(Static):
    """Single approval request card."""

    def __init__(self, req: ApprovalRequest, **kwargs) -> None:
        super().__init__(**kwargs)
        self.req = req

    def compose(self) -> ComposeResult:
        with Vertical(classes=f"approval-card approval-risk-{self.req.risk_level}"):
            yield Static(f"[bold]Agent:[/bold] {self.req.agent or 'Unknown'}")
            yield Static(f"[bold]Project:[/bold] {self.req.project}")
            yield Static(f"[bold]Action:[/bold] {self.req.action}")
            yield Static(f"[bold]Risk:[/bold] {self.req.risk_level.upper()}")
            if self.req.params_redacted:
                yield Static(f"[dim]Params: {self.req.params_redacted[:100]}[/dim]")
            with Horizontal():
                yield Button("Approve", id=f"approve-{self.req.id}", variant="success")
                yield Button("Reject", id=f"reject-{self.req.id}", variant="error")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        btn_id = event.button.id
        engine = ApprovalsEngine(get_settings().workspace_path)
        if btn_id and btn_id.startswith("approve-"):
            approval_id = btn_id.replace("approve-", "")
            engine.resolve(approval_id, "approved")
            self.remove()
        elif btn_id and btn_id.startswith("reject-"):
            approval_id = btn_id.replace("reject-", "")
            engine.resolve(approval_id, "rejected")
            self.remove()


class ApprovalsScreen(Screen):
    """Pending approvals center."""

    BINDINGS = [("escape", "app.pop_screen", "Back"), ("r", "refresh", "Refresh")]

    def compose(self) -> ComposeResult:
        yield Header()
        with Vertical(id="approvals-container"):
            yield Static("Pending Approvals", classes="title")
        yield Footer()

    def on_mount(self) -> None:
        self._load_approvals()

    def action_refresh(self) -> None:
        self._load_approvals()

    def _load_approvals(self) -> None:
        container = self.query_one("#approvals-container")
        for child in list(container.children):
            if isinstance(child, ApprovalCard):
                child.remove()

        engine = ApprovalsEngine(get_settings().workspace_path)
        pending = engine.list_pending()
        if not pending:
            container.mount(Static("[dim]No pending approvals.[/dim]"))
            return

        for req in pending:
            container.mount(ApprovalCard(req))
