"""MCP file tools."""
from __future__ import annotations

from typing import Any

from ...config.settings import get_settings
from ...core import FileEngine


def _files(project: str) -> FileEngine:
    return FileEngine(get_settings().workspace_path, project)


async def list_files(project: str, directory: str = "") -> dict[str, Any]:
    """List files and folders in project."""
    return {"items": _files(project).list_files(directory)}


async def read_file(project: str, path: str) -> dict[str, Any]:
    """Read file content."""
    return {"content": _files(project).read_file(path)}


async def save_file(project: str, path: str, content: str) -> dict[str, Any]:
    """Create or update a file."""
    _files(project).save_file(path, content)
    return {"success": True}


async def create_folder(project: str, path: str) -> dict[str, Any]:
    """Create a directory."""
    _files(project).create_folder(path)
    return {"success": True}


async def rename_file(project: str, source: str, destination: str) -> dict[str, Any]:
    """Rename or move a file."""
    _files(project).rename_file(source, destination)
    return {"success": True}


async def delete_file(project: str, path: str, confirm: bool = False) -> dict[str, Any]:
    """Delete a file."""
    if not confirm:
        return {"success": False, "error": "Confirmation required"}
    _files(project).delete_file(path)
    return {"success": True}


async def search_files(project: str, query: str) -> dict[str, Any]:
    """Search file names and content."""
    return {"results": _files(project).search_files(query)}
