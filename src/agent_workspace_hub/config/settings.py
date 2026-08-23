"""Application settings with vault-backed credential storage."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import keyring
from pydantic import BaseModel, Field

from .constants import APP_NAME, DEFAULT_SERVER_HOST, DEFAULT_SERVER_PORT, DEFAULT_WORKSPACE_ROOT


class Settings(BaseModel):
    """Application settings persisted to JSON."""

    workspace_root: str = Field(default=str(DEFAULT_WORKSPACE_ROOT))
    server_host: str = Field(default=DEFAULT_SERVER_HOST)
    server_port: int = Field(default=DEFAULT_SERVER_PORT)
    log_level: str = Field(default="info")
    theme: str = Field(default="dark")
    auto_start_server: bool = Field(default=False)
    git_auto_init: bool = Field(default=True)
    approval_required_for: list[str] = Field(default_factory=lambda: ["deploy", "delete", "admin"])
    max_log_history: int = Field(default=10000)
    composio_api_key_ref: str | None = Field(default=None)
    skill_hub_enabled: bool = Field(default=True)

    @property
    def workspace_path(self) -> Path:
        return Path(self.workspace_root).expanduser().resolve()

    def save(self) -> None:
        path = self._settings_path()
        path.parent.mkdir(parents=True, exist_ok=True)
        data = self.model_dump()
        path.write_text(json.dumps(data, indent=2))

    @classmethod
    def load(cls) -> Settings:
        path = cls._settings_path()
        if path.exists():
            return cls(**json.loads(path.read_text()))
        return cls()

    @staticmethod
    def _settings_path() -> Path:
        app_dir = Path.home() / ".config" / APP_NAME
        app_dir.mkdir(parents=True, exist_ok=True)
        return app_dir / "settings.json"

    def set_composio_api_key(self, api_key: str) -> None:
        keyring.set_password(APP_NAME, "composio_api_key", api_key)
        self.composio_api_key_ref = "keyring:composio_api_key"
        self.save()

    def get_composio_api_key(self) -> str | None:
        if self.composio_api_key_ref != "keyring:composio_api_key":
            return None
        return keyring.get_password(APP_NAME, "composio_api_key")

    def clear_composio_api_key(self) -> None:
        try:
            keyring.delete_password(APP_NAME, "composio_api_key")
        except keyring.errors.PasswordDeleteError:
            pass
        self.composio_api_key_ref = None
        self.save()

    def set_credential(self, name: str, value: str) -> None:
        keyring.set_password(APP_NAME, name, value)

    def get_credential(self, name: str) -> str | None:
        return keyring.get_password(APP_NAME, name)

    def delete_credential(self, name: str) -> None:
        try:
            keyring.delete_password(APP_NAME, name)
        except keyring.errors.PasswordDeleteError:
            pass


_settings_instance: Settings | None = None


def get_settings() -> Settings:
    global _settings_instance
    if _settings_instance is None:
        _settings_instance = Settings.load()
    return _settings_instance
