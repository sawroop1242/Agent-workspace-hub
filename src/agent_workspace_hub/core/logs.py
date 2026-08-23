"""Structured logging with live streaming support."""
from __future__ import annotations

import json
import threading
from collections.abc import Callable
from datetime import datetime
from pathlib import Path

from ..config.constants import GLOBAL_LOGS_DIR
from ..models.log import LogEntry


class LogsEngine:
    def __init__(self, workspace_root: Path) -> None:
        self.logs_dir = workspace_root / GLOBAL_LOGS_DIR
        self.logs_dir.mkdir(parents=True, exist_ok=True)
        self.log_file = self.logs_dir / "server.jsonl"
        self._callbacks: list[Callable[[LogEntry], None]] = []
        self._lock = threading.Lock()

    def subscribe(self, callback: Callable[[LogEntry], None]) -> None:
        with self._lock:
            self._callbacks.append(callback)

    def unsubscribe(self, callback: Callable[[LogEntry], None]) -> None:
        with self._lock:
            if callback in self._callbacks:
                self._callbacks.remove(callback)

    def log(self, message: str, level: str = "info", category: str = "server",
            project: str | None = None, agent: str | None = None,
            metadata: dict | None = None) -> None:
        entry = LogEntry(
            timestamp=datetime.utcnow().isoformat(),
            level=level,
            category=category,
            project=project,
            agent=agent,
            message=message,
            metadata=metadata or {},
        )
        with open(self.log_file, "a") as f:
            f.write(entry.model_dump_json() + "\n")

        with self._lock:
            for cb in list(self._callbacks):
                try:
                    cb(entry)
                except Exception:
                    pass

    def read_logs(self, limit: int = 200, category: str | None = None,
                  level: str | None = None, project: str | None = None) -> list[LogEntry]:
        if not self.log_file.exists():
            return []
        entries = []
        for line in reversed(self.log_file.read_text().strip().splitlines()):
            if not line.strip():
                continue
            try:
                entry = LogEntry(**json.loads(line))
                if category and entry.category != category:
                    continue
                if level and entry.level != level:
                    continue
                if project and entry.project != project:
                    continue
                entries.append(entry)
                if len(entries) >= limit:
                    break
            except Exception:
                continue
        return list(reversed(entries))

    def clear(self) -> None:
        self.log_file.write_text("")
