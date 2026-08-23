"""Plugin models."""
from __future__ import annotations

from datetime import datetime
from typing import Any

from pydantic import BaseModel, Field


class PluginAction(BaseModel):
    """Individual plugin action definition."""
    id: str = Field(...)
    name: str = Field(...)
    description: str = Field(default="")
    risk_level: str = Field(default="read", pattern="^(read|write|deploy|delete|admin)$")
    input_schema: dict[str, Any] = Field(default_factory=dict)
    output_schema: dict[str, Any] = Field(default_factory=dict)
    requires_approval: bool = Field(default=False)


class PluginManifest(BaseModel):
    """Plugin manifest metadata."""
    id: str = Field(...)
    name: str = Field(...)
    version: str = Field(default="1.0.0")
    description: str = Field(default="")
    auth_type: str = Field(default="token", pattern="^(token|oauth|api_key|none)$")
    required_credentials: list[str] = Field(default_factory=list)
    actions: list[PluginAction] = Field(default_factory=list)
    permissions: list[str] = Field(default_factory=list)
    config_schema: dict[str, Any] = Field(default_factory=dict)
    risk_level: str = Field(default="low", pattern="^(low|medium|high)$")
    updated_at: str = Field(default_factory=lambda: datetime.utcnow().isoformat())
    source: str = Field(default="")
