"""Service layer — orchestration, validation, and CRUD for the Tarragon application."""

from tarragon.services.color_tagger import extract_dominant_color_tags
from tarragon.services.editors import launch_editor, resolve_editor_command, substitute_file_path
from tarragon.services.query_service import QueryService
from tarragon.services.settings_service import SettingsService
from tarragon.services.tag_service import TagService
from tarragon.services.thumbnail_service import ThumbnailService

__all__ = [
    "QueryService",
    "SettingsService",
    "TagService",
    "ThumbnailService",
    "extract_dominant_color_tags",
    "launch_editor",
    "resolve_editor_command",
    "substitute_file_path",
]
