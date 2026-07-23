"""High-level CRUD operations for file tagging.

Wraps Database tag operations into a proper service layer interface
with Qt signals for UI reactivity.
"""

from __future__ import annotations

import logging
from typing import Any

from PySide6.QtCore import QObject, Signal

from tarragon.db.database import Database

logger = logging.getLogger(__name__)


class TagService(QObject):
    """Service-layer wrapper around Database tag CRUD.

    Provides API for managing tags attached to files, emitting
    ``tags_changed`` whenever the tag-state of any file is mutated.
    """

    tags_changed = Signal()

    def __init__(self, db: Database) -> None:
        super().__init__()
        self._db = db

    def _get_or_create_tag(self, name: str) -> int:
        """Return the id of *name*, creating the tag if it doesn't exist."""
        return self._db.ensure_tag(name)

    def add_tags_to_files(self, paths: list[str], tag_names: list[str], source: str = "user") -> None:
        """Add *tag_names* to every path in *paths*.

        Tags are created on-the-fly if they don't already exist.
        Emits ``tags_changed`` when done.
        """
        for tag_name in tag_names:
            tag_id = self._get_or_create_tag(tag_name)
            self._db.add_file_tags(paths, tag_id, source=source)
        logger.debug("Added tags %s to %s", tag_names, paths)
        self.tags_changed.emit()

    def remove_tags_from_files(self, paths: list[str], tag_ids: set[int]) -> None:
        """Remove every tag in *tag_ids* from every path in *paths*.

        Emits ``tags_changed`` when done.
        """
        for tag_id in tag_ids:
            self._db.remove_file_tags(paths, tag_id)
        logger.debug("Removed tags %s from %s", tag_ids, paths)
        self.tags_changed.emit()

    def get_tag_name(self, tag_id: int) -> str | None:
        """Get tag name by ID. Returns None if the tag does not exist."""
        return self._db.get_tag_name(tag_id)

    def get_tags_for_file(self, path: str) -> list[dict[str, Any]]:
        """Return all tags attached to *path*.

        Each entry: ``{"id": int, "name": str, "source": str}``.
        """
        return self._db.get_tags_for_file(path)

    def get_file_tag_ids_batch(self, paths: list[str]) -> dict[str, set[int]]:
        """Fetch tag IDs for multiple paths in a single query.

        Returns
        -------
        dict[str, set[int]]
            Mapping of path → set of tag_ids. Paths with no tags map to
            an empty set.
        """
        return self._db.get_file_tag_ids_batch(paths)

    def get_all_tags(self, folder_path: str | None = None) -> list[dict[str, Any]]:
        """Return every tag with its usage count.

        Each entry: ``{"id": int, "name": str, "usage_count": int}``.
        Ordered alphabetically by name.

        Parameters
        ----------
        folder_path:
            When provided and non-empty, usage counts are scoped to files
            whose path starts with *folder_path* (local mode). When ``None``
            or empty, counts span the entire database (global mode).
        """
        return self._db.get_all_tags_with_counts(folder_path)
