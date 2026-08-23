"""Safe file operations with path traversal guards."""
from __future__ import annotations

import shutil
from pathlib import Path

from ..utils.security import is_path_safe, is_secret_file


class FileEngine:
    def __init__(self, workspace_root: Path, project_name: str) -> None:
        self.project_path = workspace_root / "projects" / project_name

    def _safe_path(self, relative: str) -> Path:
        if not is_path_safe(self.project_path, relative):
            raise PermissionError(f"Path traversal blocked: {relative}")
        p = (self.project_path / relative).resolve()
        if is_secret_file(str(p)):
            raise PermissionError(f"Access to secret file blocked: {relative}")
        return p

    def list_files(self, relative_dir: str = "") -> list[dict]:
        base = self._safe_path(relative_dir) if relative_dir else self.project_path
        items = []
        for entry in sorted(base.iterdir()):
            rel = str(entry.relative_to(self.project_path))
            items.append({
                "name": entry.name,
                "path": rel,
                "is_dir": entry.is_dir(),
                "size": entry.stat().st_size if entry.is_file() else 0,
            })
        return items

    def read_file(self, relative: str) -> str:
        p = self._safe_path(relative)
        if not p.exists() or p.is_dir():
            raise FileNotFoundError(f"File not found: {relative}")
        return p.read_text()

    def save_file(self, relative: str, content: str) -> None:
        p = self._safe_path(relative)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content)

    def create_folder(self, relative: str) -> None:
        p = self._safe_path(relative)
        p.mkdir(parents=True, exist_ok=True)

    def rename_file(self, relative_src: str, relative_dst: str) -> None:
        src = self._safe_path(relative_src)
        dst = self._safe_path(relative_dst)
        src.rename(dst)

    def delete_file(self, relative: str, archive: bool = True) -> None:
        p = self._safe_path(relative)
        if p.is_dir():
            shutil.rmtree(p)
        else:
            p.unlink()

    def search_files(self, query: str) -> list[dict]:
        results = []
        for root, _dirs, files in __import__("os").walk(self.project_path):
            for f in files:
                fp = Path(root) / f
                try:
                    rel = str(fp.relative_to(self.project_path))
                    if query.lower() in f.lower():
                        results.append({"path": rel, "match": "filename"})
                    elif fp.stat().st_size < 1024 * 1024:  # Skip files > 1MB
                        content = fp.read_text(errors="ignore")
                        if query.lower() in content.lower():
                            results.append({"path": rel, "match": "content"})
                except Exception:
                    continue
                if len(results) >= 100:
                    break
            if len(results) >= 100:
                break
        return results
