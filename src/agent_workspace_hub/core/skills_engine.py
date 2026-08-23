"""Global and project-local skill management."""
from __future__ import annotations

import json
import shutil
from pathlib import Path

from ..config.constants import AGENT_DIR, GLOBAL_SKILLS_DIR, SKILLS_DIR, SKILLS_INDEX_JSON
from ..models.skill import SkillManifest


class SkillsEngine:
    def __init__(self, workspace_root: Path) -> None:
        self.root = workspace_root
        self.global_skills = self.root / GLOBAL_SKILLS_DIR

    def list_global_skills(self) -> list[SkillManifest]:
        skills = []
        if not self.global_skills.exists():
            return skills
        for entry in self.global_skills.iterdir():
            manifest = entry / "manifest.json"
            if manifest.exists():
                try:
                    skills.append(SkillManifest(**json.loads(manifest.read_text())))
                except Exception:
                    continue
        return skills

    def list_project_skills(self, project_name: str) -> list[SkillManifest]:
        agent_dir = self.root / "projects" / project_name / AGENT_DIR
        index_file = agent_dir / SKILLS_INDEX_JSON
        enabled = json.loads(index_file.read_text()).get("enabled", []) if index_file.exists() else []

        skills = []
        for ref in enabled:
            if ref.startswith("global://"):
                skill_id = ref.replace("global://skills/", "")
                manifest = self.global_skills / skill_id / "manifest.json"
            elif ref.startswith("local://"):
                rel = ref.replace("local://", "")
                manifest = agent_dir / rel
            else:
                continue

            if manifest.exists():
                try:
                    skills.append(SkillManifest(**json.loads(manifest.read_text())))
                except Exception:
                    continue
        return skills

    def read_skill(self, skill_id: str, source: str = "global") -> str:
        if source == "global":
            entry_file = self.global_skills / skill_id / "skill.md"
        else:
            entry_file = self.root / "projects" / source / AGENT_DIR / SKILLS_DIR / f"{skill_id}.md"
        if not entry_file.exists():
            raise FileNotFoundError(f"Skill {skill_id} not found")
        return entry_file.read_text()

    def install_global_skill(self, manifest: SkillManifest, content: str) -> None:
        dest = self.global_skills / manifest.id
        dest.mkdir(parents=True, exist_ok=True)
        (dest / "manifest.json").write_text(manifest.model_dump_json(indent=2))
        (dest / manifest.entry_file).write_text(content)

    def uninstall_global_skill(self, skill_id: str) -> None:
        dest = self.global_skills / skill_id
        if dest.exists():
            shutil.rmtree(dest)

    def enable_project_skill(self, project_name: str, skill_id: str, source: str = "global") -> None:
        agent_dir = self.root / "projects" / project_name / AGENT_DIR
        index_file = agent_dir / SKILLS_INDEX_JSON
        index = json.loads(index_file.read_text()) if index_file.exists() else {"enabled": []}
        ref = f"global://skills/{skill_id}" if source == "global" else f"local://.agent/skills/{skill_id}.md"
        if ref not in index["enabled"]:
            index["enabled"].append(ref)
        index_file.write_text(json.dumps(index, indent=2))

    def disable_project_skill(self, project_name: str, skill_id: str, source: str = "global") -> None:
        agent_dir = self.root / "projects" / project_name / AGENT_DIR
        index_file = agent_dir / SKILLS_INDEX_JSON
        if not index_file.exists():
            return
        index = json.loads(index_file.read_text())
        ref = f"global://skills/{skill_id}" if source == "global" else f"local://.agent/skills/{skill_id}.md"
        if ref in index.get("enabled", []):
            index["enabled"].remove(ref)
        index_file.write_text(json.dumps(index, indent=2))
