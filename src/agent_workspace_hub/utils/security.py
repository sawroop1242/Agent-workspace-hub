"""Security utilities: path traversal prevention, secret detection."""
from __future__ import annotations

import fnmatch
import re
from pathlib import Path

from ..config.constants import SECRET_PATTERNS


def is_path_safe(project_path: Path, requested_path: str) -> bool:
    """Ensure requested path stays within project boundary."""
    try:
        resolved = (project_path / requested_path).resolve()
        resolved.relative_to(project_path.resolve())
        return True
    except (ValueError, RuntimeError):
        return False


def is_secret_file(filename: str) -> bool:
    """Check if filename matches secret patterns."""
    name = Path(filename).name
    for pattern in SECRET_PATTERNS:
        if fnmatch.fnmatch(name, pattern):
            return True
    return False


def redact_secrets(text: str) -> str:
    """Redact common secret patterns from text."""
    # API keys, tokens, passwords
    patterns = [
        (r'[Aa][Pp][Ii][_-]?[Kk][Ee][Yy]\s*[:=]\s*["\']?[A-Za-z0-9_\-]{16,}["\']?', 'API_KEY=***'),
        (r'[Tt][Oo][Kk][Ee][Nn]\s*[:=]\s*["\']?[A-Za-z0-9_\-]{16,}["\']?', 'TOKEN=***'),
        (r'[Pp][Aa][Ss][Ss][Ww][Oo][Rr][Dd]\s*[:=]\s*["\']?[^\s"\']+["\']?', 'PASSWORD=***'),
        (r'sk-[A-Za-z0-9]{20,}', 'sk-***'),
        (r'ghp_[A-Za-z0-9]{36}', 'ghp_***'),
        (r'gho_[A-Za-z0-9]{36}', 'gho_***'),
    ]
    for pattern, replacement in patterns:
        text = re.sub(pattern, replacement, text)
    return text


def sanitize_filename(name: str) -> str:
    """Remove dangerous characters from filenames."""
    return re.sub(r'[<>:"/\\|?*\x00-\x1f]', '_', name)
