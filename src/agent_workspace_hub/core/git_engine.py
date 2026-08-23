"""Git operations and checkpoint management."""
from __future__ import annotations

import json
from pathlib import Path

from git import Repo

from ..config.constants import CHECKPOINTS_DIR
from ..models.project import Checkpoint


class GitEngine:
    def __init__(self, workspace_root: Path, project_name: str) -> None:
        self.project_path = workspace_root / "projects" / project_name
        self.checkpoints_dir = self.project_path / ".agent" / CHECKPOINTS_DIR

    def _get_repo(self) -> Repo:
        if not (self.project_path / ".git").exists():
            raise RuntimeError("Git not initialized for this project")
        return Repo(self.project_path)

    def init(self) -> None:
        if not (self.project_path / ".git").exists():
            Repo.init(self.project_path)

    def status(self) -> dict:
        try:
            repo = self._get_repo()
            return {
                "changed": [item.a_path for item in repo.index.diff(None)],
                "untracked": repo.untracked_files,
                "staged": [item.a_path for item in repo.index.diff("HEAD")],
            }
        except Exception as e:
            return {"error": str(e), "changed": [], "untracked": [], "staged": []}

    def diff(self, file_path: str | None = None) -> str:
        repo = self._get_repo()
        if file_path:
            return repo.git.diff(file_path)
        return repo.git.diff()

    def create_checkpoint(self, message: str, agent: str = "", reason: str = "") -> Checkpoint:
        repo = self._get_repo()
        repo.git.add("--all")
        commit = repo.index.commit(message)
        cp = Checkpoint(
            id=f"cp-{commit.hexsha[:8]}",
            project=self.project_path.name,
            message=message,
            agent=agent,
            reason=reason,
            git_commit_hash=commit.hexsha,
        )
        self.checkpoints_dir.mkdir(parents=True, exist_ok=True)
        (self.checkpoints_dir / f"{cp.id}.json").write_text(cp.model_dump_json(indent=2))
        return cp

    def list_checkpoints(self) -> list[Checkpoint]:
        cps = []
        if self.checkpoints_dir.exists():
            for f in sorted(self.checkpoints_dir.glob("*.json")):
                try:
                    cps.append(Checkpoint(**json.loads(f.read_text())))
                except Exception:
                    continue
        return cps

    def restore_checkpoint(self, commit_hash: str) -> None:
        repo = self._get_repo()
        repo.git.stash("push", "-m", "auto-backup-before-restore")
        repo.git.checkout(commit_hash, "--", ".")

    def push(self, remote: str = "origin", branch: str = "main") -> str:
        repo = self._get_repo()
        origin = repo.remote(remote)
        origin.push(branch)
        return f"Pushed to {remote}/{branch}"

    def pull(self, remote: str = "origin", branch: str = "main") -> str:
        repo = self._get_repo()
        origin = repo.remote(remote)
        origin.pull(branch)
        return f"Pulled from {remote}/{branch}"
