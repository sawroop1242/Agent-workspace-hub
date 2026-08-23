"""Approval queue for dangerous actions."""
from __future__ import annotations

import uuid
from datetime import datetime
from pathlib import Path
from typing import Any

from ..models.approval import ApprovalRequest


class ApprovalsEngine:
    def __init__(self, workspace_root: Path) -> None:
        self.pending_path = workspace_root / "global" / "pending_approvals.jsonl"
        self.pending_path.parent.mkdir(parents=True, exist_ok=True)

    def request(self, project: str, action: str, risk_level: str, agent: str = "",
                plugin: str = "", params: dict[str, Any] | None = None) -> ApprovalRequest:
        req = ApprovalRequest(
            id=str(uuid.uuid4())[:8],
            project=project,
            agent=agent,
            plugin=plugin,
            action=action,
            params_redacted=str(params) if params else "",
            risk_level=risk_level,
            status="pending",
        )
        with open(self.pending_path, "a") as f:
            f.write(req.model_dump_json() + "\n")
        return req

    def list_pending(self) -> list[ApprovalRequest]:
        if not self.pending_path.exists():
            return []
        pending = []
        for line in self.pending_path.read_text().strip().splitlines():
            try:
                req = ApprovalRequest(**__import__("json").loads(line))
                if req.status == "pending":
                    pending.append(req)
            except Exception:
                continue
        return pending

    def resolve(self, approval_id: str, status: str, resolved_by: str = "user") -> ApprovalRequest | None:
        if not self.pending_path.exists():
            return None
        lines = self.pending_path.read_text().strip().splitlines()
        updated = []
        result = None
        for line in lines:
            try:
                req = ApprovalRequest(**__import__("json").loads(line))
                if req.id == approval_id and req.status == "pending":
                    req.status = status
                    req.resolved_at = datetime.utcnow().isoformat()
                    req.resolved_by = resolved_by
                    result = req
                updated.append(req.model_dump_json())
            except Exception:
                updated.append(line)
        self.pending_path.write_text("\n".join(updated) + "\n")
        return result
