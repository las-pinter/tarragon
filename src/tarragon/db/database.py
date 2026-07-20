"""Final Database class composing all mixin capabilities."""

from __future__ import annotations

from tarragon.db._base import Base
from tarragon.db._editors import EditorsMixin
from tarragon.db._favorites import FavoritesMixin
from tarragon.db._folder_cache import FolderCacheMixin
from tarragon.db._settings import SettingsMixin
from tarragon.db._tags import TagsMixin
from tarragon.db._thumbnails import ThumbnailsMixin


class Database(
    Base,
    ThumbnailsMixin,
    TagsMixin,
    FavoritesMixin,
    SettingsMixin,
    EditorsMixin,
    FolderCacheMixin,
):
    """SQLite-backed repository for Tarragon's catalog data.

    Manages schema initialization and provides CRUD operations for thumbnails,
    tags, favorites, settings, and editor associations.

    Usage as context manager is supported; the caller owns connection lifecycle.
    """
