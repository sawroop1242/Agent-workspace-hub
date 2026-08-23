"""Approval request models."""
from __future__ import annotations

from datetime import datetime

from pydantic import BaseModel, Field


class ApprovalRequest(BaseModel):
    """Pending approval for a dangerous action."""
    id: str = Field(...)
    project: str = Field(...)
    agent: str = Field(default="")
    plugin: str = Field(default="")
    action: str = Field(...)
    params_redacted: str = Field(default="")
    risk_level: str = Field(default="write", pattern="^(read|write|deploy|delete|admin)$")
    status: str = Field(default="pending", pattern="^(pending|approved|rejected)$")
    created_at: str = Field(default_factory=lambda: datetime.utcnow().isoformat())
    resolved_at: str | None = Field(default=None)
    resolved_by: str = Field(default="")
    reason: str = Field(default="")
