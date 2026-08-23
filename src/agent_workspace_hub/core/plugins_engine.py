"""Plugin engine — routes to Composio or local plugins."""
from __future__ import annotations

import json
from pathlib import Path

from ..config.constants import AGENT_DIR, GLOBAL_PLUGINS_DIR, PLUGINS_JSON
from ..models.plugin import PluginManifest


class PluginsEngine:
    def __init__(self, workspace_root: Path) -> None:
        self.root = workspace_root
        self.global_plugins = self.root / GLOBAL_PLUGINS_DIR

    def list_global_plugins(self) -> list[PluginManifest]:
        plugins = []
        if not self.global_plugins.exists():
            return plugins
        for entry in self.global_plugins.iterdir():
            manifest = entry / "manifest.json"
            if manifest.exists():
                try:
                    plugins.append(PluginManifest(**json.loads(manifest.read_text())))
                except Exception:
                    continue
        return plugins

    def list_project_plugins(self, project_name: str) -> list[PluginManifest]:
        agent_dir = self.root / "projects" / project_name / AGENT_DIR
        index_file = agent_dir / PLUGINS_JSON
        enabled = json.loads(index_file.read_text()).get("enabled", []) if index_file.exists() else []

        plugins = []
        for plugin_id in enabled:
            manifest = self.global_plugins / plugin_id / "manifest.json"
            if manifest.exists():
                try:
                    plugins.append(PluginManifest(**json.loads(manifest.read_text())))
                except Exception:
                    continue
        return plugins

    def enable_project_plugin(self, project_name: str, plugin_id: str) -> None:
        agent_dir = self.root / "projects" / project_name / AGENT_DIR
        index_file = agent_dir / PLUGINS_JSON
        index = json.loads(index_file.read_text()) if index_file.exists() else {"enabled": []}
        if plugin_id not in index["enabled"]:
            index["enabled"].append(plugin_id)
        index_file.write_text(json.dumps(index, indent=2))

    def disable_project_plugin(self, project_name: str, plugin_id: str) -> None:
        agent_dir = self.root / "projects" / project_name / AGENT_DIR
        index_file = agent_dir / PLUGINS_JSON
        if not index_file.exists():
            return
        index = json.loads(index_file.read_text())
        if plugin_id in index.get("enabled", []):
            index["enabled"].remove(plugin_id)
        index_file.write_text(json.dumps(index, indent=2))
