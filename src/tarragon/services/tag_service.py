"""High-level operations for file tagging."""

from __future__ import annotations

import logging

from PySide6.QtCore import QObject, Signal

from tarragon.db.common.tag import Tag, TagSource
from tarragon.db.database import Database

logger = logging.getLogger(__name__)


class TagService(QObject):
    """Service-layer wrapper around Database.

    Provides API for managing tags attached to files, emitting
    ``tags_changed`` whenever the tag-state of any file is mutated.
    """

    tags_changed = Signal()

    def __init__(self, db: Database) -> None:
        super().__init__()
        self._db = db

    def create_tag(self, name: str, source: TagSource = TagSource.USER) -> Tag:
        """Create a tag with *name*
        Returns
        -------
        Tag
            A newly created Tag object
        """
        return self._db.ensure_tag(name, source)

    def add_tags_to_files_by_name(
        self, paths: list[str], tag_names: list[str], source: TagSource = TagSource.USER
    ) -> set[Tag]:
        """Add *tag_names* to every path in *paths*.

        Tags are created on-the-fly if they don't already exist.
        Emits ``tags_changed`` when done.
        """
        ensured_tags: set[Tag] = set()
        for tag_name in tag_names:
            tag = self._db.ensure_tag(tag_name, source)
            ensured_tags.add(tag)
            self._db.add_tag_to_files(paths, tag)
        logger.debug("Added tags %s to %s", tag_names, paths)
        self.tags_changed.emit()
        return ensured_tags

    def add_tags_to_file_by_name(self, path: str, tag_names: list[str], source: TagSource = TagSource.USER) -> set[Tag]:
        """Add *tag_names* to a *path*.

        Tags are created on-the-fly if they don't already exist.
        Emits ``tags_changed`` when done.
        """
        return self.add_tags_to_files_by_name([path], tag_names, source)

    def add_tags_to_files(self, paths: list[str], tags: set[Tag]) -> None:
        """Add *tags* to every path in *paths*."""
        for tag in tags:
            self._db.add_tag_to_files(paths, tag)
        logger.debug("Added tags %s to %s", tags, paths)
        self.tags_changed.emit()

    def add_tags_to_file(self, path: str, tags: set[Tag]) -> None:
        """Add *tags* to a *path*."""
        return self.add_tags_to_files([path], tags)

    def get_tags_for_file(self, path: str) -> set[Tag]:
        """Return all tags attached to *path*.

        Each entry: ``Tag``.
        """
        return self._db.get_tags_for_file(path)

    def get_tags_for_files(self, paths: list[str]) -> dict[str, set[Tag]]:
        """Fetch tags for multiple *paths* in a single query.

        Returns
        -------
        dict[str, set[Tag]]
            Mapping of path -> set of tags. Paths with no tags map to
            an empty set.
        """
        return self._db.get_tags_for_files(paths)

    def get_all_tags(self) -> list[Tag]:
        """Return every tag with its usage count.

        Returns
        -------
        list[Tag]
            Each entry: ``Tag``. Ordered alphabetically by name.
        """
        tags = self._db.get_all_tags()
        return sorted(tags)

    def remove_tags_from_files(self, paths: list[str], tags: set[Tag]) -> None:
        """Remove every tag in *tag_ids* from every path in *paths*.

        Emits ``tags_changed`` when done.
        """
        for tag in tags:
            self._db.remove_tag_from_files(paths, tag)
        logger.debug("Removed tags %s from %s", tags, paths)
        self.tags_changed.emit()

    def remove_tags_from_file(self, path: str, tags: set[Tag]) -> None:
        """Remove every tag in *tag_ids* from *path*.

        Emits ``tags_changed`` when done.
        """
        self.remove_tags_from_files([path], tags)

    def replace_auto_color_tags(self, path: str, tags: set[Tag]) -> None:
        """Replace the auto color tags for a file path."""
        self._db.replace_auto_color_tags(path, tags)
