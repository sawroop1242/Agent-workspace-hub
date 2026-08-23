"""skill_hub.ai integration for online skill discovery."""
from .client import SkillHubClient
from .search import search_skills

__all__ = ["SkillHubClient", "search_skills"]
