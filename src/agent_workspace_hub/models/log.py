"""Log entry models."""
from __future__ import annotations

from datetime import datetime
from typing import Any

from pydantic import BaseModel, Field


class LogEntry(BaseModel):
    """Structured log entry."""
    timestamp: str = Field(default_factory=lambda: datetime.utcnow().isoformat())
    level: str = Field(default="info", pattern="^(debug|info|warn|error)$")
    category: str = Field(default="server")
    project: str | None = Field(default=None)
    agent: str | None = Field(default=None)
    message: str = Field(...)
    metadata: dict[str, Any] = Field(default_factory=dict)
