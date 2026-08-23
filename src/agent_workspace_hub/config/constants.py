"""Application constants."""
import platformdirs
from pathlib import Path

APP_NAME = "agent-workspace-hub"
SCHEMA_VERSION = "1.0.0"
DEFAULT_SERVER_PORT = 8765
DEFAULT_SERVER_HOST = "127.0.0.1"

# Default workspace root
DEFAULT_WORKSPACE_ROOT = Path(platformdirs.user_data_dir(APP_NAME)) / "workspace"

# Subdirectories
GLOBAL_SKILLS_DIR = "global/skills"
GLOBAL_PLUGINS_DIR = "global/plugins"
GLOBAL_REGISTRY_DIR = "global/registry"
GLOBAL_LOGS_DIR = "global/logs"
GLOBAL_VAULT_DIR = "global/vault"
PROJECTS_DIR = "projects"
ARCHIVE_DIR = "archive"

# Project internal structure
AGENT_DIR = ".agent"
AGENT_MD = "AGENT.md"
README_MD = "README.md"
PROJECT_JSON = "project.json"
CONTEXT_MD = "context.md"
MEMORY_JSONL = "memory.jsonl"
SKILLS_INDEX_JSON = "skills-index.json"
PLUGINS_JSON = "plugins.json"
TASKS_DIR = "tasks"
DECISIONS_DIR = "decisions"
RULES_DIR = "rules"
LOGS_DIR = "logs"
CHECKPOINTS_DIR = "checkpoints"
SKILLS_DIR = "skills"

# Security
SECRET_PATTERNS = [
    ".env", ".env.local", ".env.*", "id_rsa", "id_dsa", "id_ecdsa", "id_ed25519",
    "*.pem", "*.key", "*.p12", "*.pfx", "credentials.json", "secrets.json",
    "token.json", "api_key.txt", "password.txt", "passwd", "shadow", ".htpasswd",
    "*.keystore", "*.jks",
]

# Enums
MEMORY_TYPES = ["note", "decision", "task_update", "file_change", "plugin_action", "checkpoint", "error", "approval"]
TASK_STATUSES = ["todo", "in_progress", "done", "blocked"]
RISK_LEVELS = ["read", "write", "deploy", "delete", "admin"]
LOG_CATEGORIES = ["server", "app", "agent", "plugin", "git", "file", "approval", "error"]
LOG_LEVELS = ["debug", "info", "warn", "error"]
