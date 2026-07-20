"""Tag and file-tag CRUD operations mixed into the Database class."""

from __future__ import annotations

import logging
from typing import Any

from tarragon.db._base import MixinBase, normalize_path

logger = logging.getLogger(__name__)


class TagsMixin(MixinBase):
    """Create, query, and delete tags and their file associations."""

    def ensure_tag(self, name: str) -> int:
        """Insert a tag if it doesn't exist; always returns the tag id."""
        logger.debug("ensure_tag: name=%s", name)
        cursor = self._execute(
            "INSERT INTO tags (name) VALUES (?) ON CONFLICT(name) DO UPDATE SET name=name RETURNING id",
            (name,),
        )
        return int(cursor.fetchone()["id"])

    def add_file_tags(self, paths: list[str], tag_id: int, source: str = "user") -> None:
        """Associate one or more file paths with a given tag."""
        logger.debug(
            "add_file_tags: %d paths, tag_id=%d, source=%s",
            len(paths),
            tag_id,
            source,
        )
        self._executemany(
            "INSERT OR IGNORE INTO file_tags (path, tag_id, source) VALUES (?, ?, ?)",
            [(normalize_path(p), tag_id, source) for p in paths],
        )
        self._commit()

    def remove_file_tags(self, paths: list[str], tag_id: int) -> None:
        """Remove file-tag associations for the given paths and tag."""
        logger.debug("remove_file_tags: %d paths, tag_id=%d", len(paths), tag_id)
        normalized = [normalize_path(p) for p in paths]
        placeholders = ",".join("?" * len(normalized))
        self._execute(
            f"DELETE FROM file_tags WHERE path IN ({placeholders}) AND tag_id = ?",
            (*normalized, tag_id),
        )
        self._commit()

    def get_file_tag_ids(self, path: str) -> set[int]:
        """Return the set of tag ids associated with a given path."""
        path = normalize_path(path)
        logger.debug("get_file_tag_ids: path=%s", path)
        cursor = self._execute("SELECT tag_id FROM file_tags WHERE path = ?", (path,))
        return {row["tag_id"] for row in cursor.fetchall()}

    def get_tags_for_file(self, path: str) -> list[dict[str, Any]]:
        """Return all tags attached to a specific file.

        Parameters
        ----------
        path:
            The file path to look up tags for.

        Returns
        -------
        list[dict[str, Any]]
            List of dicts with keys: `id`, `name`, `source`.
        """
        logger.debug("get_tags_for_file: path=%s", path)
        path = normalize_path(path)
        return self.fetch_all(
            """SELECT t.id, t.name, ft.source
               FROM tags t
               JOIN file_tags ft ON ft.tag_id = t.id
               WHERE ft.path = ?""",
            (path,),
        )

    def get_file_tag_ids_batch(self, paths: list[str]) -> dict[str, set[int]]:
        """Batch lookup of tag IDs for multiple file paths.

        More efficient than calling :meth:`get_file_tag_ids` in a loop.

        Parameters
        ----------
        paths:
            List of file paths to look up.

        Returns
        -------
        dict[str, set[int]]
            Mapping of path -> set of tag_ids.  Paths with no tags map
            to an empty set.
        """
        logger.debug("get_file_tag_ids_batch: %d paths", len(paths))
        if not paths:
            return {}

        normalized = [normalize_path(p) for p in paths]
        placeholders = ", ".join("?" * len(normalized))
        rows = self.fetch_all(
            f"SELECT path, tag_id FROM file_tags WHERE path IN ({placeholders})",
            tuple(normalized),
        )

        result: dict[str, set[int]] = {path: set() for path in normalized}
        for row in rows:
            result[row["path"]].add(row["tag_id"])
        return result

    def delete_tag(self, tag_id: int) -> None:
        """Delete a tag from the database. Also removes all file-tag associations.

        Manually deletes file_tags rows since SQLite requires
        ``PRAGMA foreign_keys = ON`` for CASCADE to take effect, and
        Tarragon does not enable that pragma globally.
        """
        logger.debug("delete_tag: tag_id=%d", tag_id)
        self._execute("DELETE FROM file_tags WHERE tag_id = ?", (tag_id,))
        self._execute("DELETE FROM tags WHERE id = ?", (tag_id,))
        self._commit()

    def get_tag_name(self, tag_id: int) -> str | None:
        """Return the name of a tag by its id, or None if not found."""
        logger.debug("get_tag_name: tag_id=%d", tag_id)
        row = self._execute("SELECT name FROM tags WHERE id = ?", (tag_id,)).fetchone()
        return row["name"] if row else None

    def replace_auto_color_tags(self, path: str, tags: list[str]) -> None:
        """Delete old auto_color tags for a path and insert new ones."""
        path = normalize_path(path)
        logger.debug("replace_auto_color_tags: path=%s, tags=%d", path, len(tags))
        self._execute(
            "DELETE FROM file_tags WHERE path = ? AND source = 'auto_color'",
            (path,),
        )
        if tags:
            tag_ids: list[int] = []
            for name in tags:
                cursor = self._execute(
                    "INSERT INTO tags (name) VALUES (?) ON CONFLICT(name) DO UPDATE SET name=name RETURNING id",
                    (name,),
                )
                tag_ids.append(cursor.fetchone()["id"])

            self._executemany(
                "INSERT OR IGNORE INTO file_tags (path, tag_id, source) VALUES (?, ?, ?)",
                [(path, tid, "auto_color") for tid in tag_ids],
            )
        self._commit()

    def get_all_tags_with_counts(self, folder_path: str | None = None) -> list[dict[str, Any]]:
        """Return all tags with their usage counts.

        Parameters
        ----------
        folder_path:
            When provided and non-empty, usage counts are scoped to files
            whose path starts with *folder_path* (local mode).  When ``None``
            or empty, counts span the entire database (global mode).

        Returns
        -------
        list[dict[str, Any]]
            List of dicts with keys: `id`, `name`, `usage_count`.
            Ordered alphabetically by name.
        """
        logger.debug("get_all_tags_with_counts: folder_path=%s", folder_path)
        if folder_path:
            folder_path = normalize_path(folder_path)
            return self.fetch_all(
                """SELECT t.id, t.name, COUNT(ft.path) AS usage_count
                   FROM tags t
                   LEFT JOIN file_tags ft ON ft.tag_id = t.id
                     AND ft.path LIKE ?
                   GROUP BY t.id, t.name
                   ORDER BY t.name""",
                (f"{folder_path.rstrip('/')}/%",),
            )
        return self.fetch_all(
            """SELECT t.id, t.name, COUNT(ft.path) AS usage_count
               FROM tags t
               LEFT JOIN file_tags ft ON ft.tag_id = t.id
               GROUP BY t.id, t.name
               ORDER BY t.name""",
        )
