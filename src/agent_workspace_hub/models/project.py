"""Project, Task, and Checkpoint models."""
from __future__ import annotations

from datetime import datetime
from typing import Any

from pydantic import BaseModel, Field


class Project(BaseModel):
    """Project metadata."""
    id: str = Field(..., description="Unique project ID")
    name: str = Field(..., description="Project name")
    description: str = Field(default="", description="Project description")
    created_at: str = Field(default_factory=lambda: datetime.utcnow().isoformat())
    updated_at: str = Field(default_factory=lambda: datetime.utcnow().isoformat())
    path: str = Field(..., description="Relative path within workspace")
    git_enabled: bool = Field(default=False)
    tags: list[str] = Field(default_factory=list)
    schema_version: str = Field(default="1.0.0")


class Task(BaseModel):
    """Project task."""
    id: str = Field(...)
    title: str = Field(...)
    description: str = Field(default="")
    status: str = Field(default="todo", pattern="^(todo|in_progress|done|blocked)$")
    priority: int = Field(default=0, ge=0, le=5)
    created_at: str = Field(default_factory=lambda: datetime.utcnow().isoformat())
    updated_at: str = Field(default_factory=lambda: datetime.utcnow().isoformat())
    created_by: str = Field(default="")
    assigned_agent: str = Field(default="")
    notes: str = Field(default="")


class Checkpoint(BaseModel):
    """Git checkpoint metadata."""
    id: str = Field(...)
    project: str = Field(...)
    message: str = Field(...)
    agent: str = Field(default="")
    reason: str = Field(default="")
    created_at: str = Field(default_factory=lambda: datetime.utcnow().isoformat())
    git_commit_hash: str = Field(default="")
