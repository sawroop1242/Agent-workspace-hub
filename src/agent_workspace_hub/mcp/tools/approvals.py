"""MCP approval tools."""
from __future__ import annotations

from typing import Any

from ...config.settings import get_settings
from ...core import ApprovalsEngine, Workspace


def _approvals() -> ApprovalsEngine:
    return ApprovalsEngine(get_settings().workspace_path)


async def list_pending_approvals() -> dict[str, Any]:
    """Show pending approvals."""
    return {"approvals": [a.model_dump() for a in _approvals().list_pending()]}


async def approve_action(approval_id: str) -> dict[str, Any]:
    """Approve a pending action."""
    req = _approvals().resolve(approval_id, "approved")
    return {"success": req is not None, "approval": req.model_dump() if req else None}


async def reject_action(approval_id: str) -> dict[str, Any]:
    """Reject a pending action."""
    req = _approvals().resolve(approval_id, "rejected")
    return {"success": req is not None, "approval": req.model_dump() if req else None}
