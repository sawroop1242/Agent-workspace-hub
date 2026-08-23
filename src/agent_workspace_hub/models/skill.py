"""Skill models."""
from __future__ import annotations

from datetime import datetime

from pydantic import BaseModel, Field


class SkillManifest(BaseModel):
    """Skill manifest metadata."""
    id: str = Field(...)
    name: str = Field(...)
    version: str = Field(default="1.0.0")
    description: str = Field(default="")
    author: str = Field(default="")
    tags: list[str] = Field(default_factory=list)
    required_plugins: list[str] = Field(default_factory=list)
    risk_level: str = Field(default="low", pattern="^(low|medium|high)$")
    entry_file: str = Field(default="skill.md")
    updated_at: str = Field(default_factory=lambda: datetime.utcnow().isoformat())
    source: str = Field(default="", description="URL or path source")
