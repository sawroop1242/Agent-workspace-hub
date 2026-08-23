"""Append-only memory.jsonl management."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from ..config.constants import AGENT_DIR, MEMORY_JSONL


class MemoryEngine:
    def __init__(self, workspace_root: Path, project_name: str) -> None:
        self.path = workspace_root / "projects" / project_name / AGENT_DIR / MEMORY_JSONL

    def append(self, message: str, type_: str, agent: str = "", metadata: dict[str, Any] | None = None) -> None:
        entry = {
            "timestamp": __import__("datetime").datetime.utcnow().isoformat(),
            "project": self.path.parent.parent.name,
            "agent": agent,
            "type": type_,
            "message": message,
            "metadata": metadata or {},
        }
        with open(self.path, "a") as f:
            f.write(json.dumps(entry) + "\n")

    def read(self, limit: int = 50, type_filter: str | None = None) -> list[dict[str, Any]]:
        if not self.path.exists():
            return []
        lines = self.path.read_text().strip().splitlines()
        entries = []
        for line in reversed(lines):
            if not line.strip():
                continue
            try:
                entry = json.loads(line)
                if type_filter and entry.get("type") != type_filter:
                    continue
                entries.append(entry)
                if len(entries) >= limit:
                    break
            except json.JSONDecodeError:
                continue
        return list(reversed(entries))

    def search(self, query: str, limit: int = 50) -> list[dict[str, Any]]:
        results = []
        for entry in self.read(limit=10000):
            if query.lower() in json.dumps(entry).lower():
                results.append(entry)
                if len(results) >= limit:
                    break
        return results
