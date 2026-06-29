"""Tag service — high-level CRUD operations for file tagging.

Wraps Database tag operations into a proper service layer interface
with Qt signals for UI reactivity.
"""

from __future__ import annotations

from typing import Any

from PySide6.QtCore import QObject, Qt, Signal

from tarragon.db import Database


class TagService(QObject):
    """Service-layer wrapper around Database tag CRUD.

    Provides a clean API for managing tags attached to files, emitting
    ``tagsChanged`` whenever the tag-state of any file is mutated.
    """

    tagsChanged = Signal()  # noqa: N815 — Qt signal follows camelCase convention

    def __init__(self, db: Database) -> None:
        super().__init__()
        self._db = db

    # ── Tag CRUD ────────────────────────────────────────────────────────────

    def get_or_create_tag(self, name: str) -> int:
        """Return the id of *name*, creating the tag if it doesn't exist."""
        return self._db.ensure_tag(name)

    def add_tags_to_files(self, paths: list[str], tag_names: list[str]) -> None:
        """Add *tag_names* to every path in *paths*.

        Tags are created on-the-fly if they don't already exist.
        Emits ``tagsChanged`` when done.
        """
        for tag_name in tag_names:
            tag_id = self.get_or_create_tag(tag_name)
            self._db.add_file_tags(paths, tag_id, source="user")
        self.tagsChanged.emit()

    def remove_tags_from_files(self, paths: list[str], tag_ids: set[int]) -> None:
        """Remove every tag in *tag_ids* from every path in *paths*.

        Emits ``tagsChanged`` when done.
        """
        for tag_id in tag_ids:
            self._db.remove_file_tags(paths, tag_id)
        self.tagsChanged.emit()

    # ── Queries ─────────────────────────────────────────────────────────────

    def get_tags_for_file(self, path: str) -> list[dict[str, Any]]:
        """Return all tags attached to *path*.

        Each entry: ``{"id": int, "name": str, "source": str}``.
        """
        rows = self._db._execute(
            """SELECT t.id, t.name, ft.source
               FROM tags t
               JOIN file_tags ft ON ft.tag_id = t.id
               WHERE ft.path = ?""",
            (path,),
        ).fetchall()
        return [{"id": row["id"], "name": row["name"], "source": row["source"]} for row in rows]

    def resolve_tri_state(self, paths: list[str], tag_id: int) -> Qt.CheckState:
        """Determine the checked-state of *tag_id* across *paths*.

        Returns ``Qt.Checked`` when all files have the tag,
        ``Qt.PartiallyChecked`` when some do, and ``Qt.Unchecked``
        when none do.
        """
        if not paths:
            return Qt.CheckState.Unchecked

        tagged = 0
        for path in paths:
            if tag_id in self._db.get_file_tag_ids(path):
                tagged += 1

        if tagged == len(paths):
            return Qt.CheckState.Checked
        if tagged > 0:
            return Qt.CheckState.PartiallyChecked
        return Qt.CheckState.Unchecked

    def get_all_tags(self, folder_path: str | None = None) -> list[dict[str, Any]]:
        """Return every tag with its usage count.

        Each entry: ``{"id": int, "name": str, "usage_count": int}``.
        Ordered alphabetically by name.

        Parameters
        ----------
        folder_path:
            When provided and non-empty, usage counts are scoped to files
            whose path starts with *folder_path* (local mode).  When ``None``
            or empty, counts span the entire database (global mode).
        """
        if folder_path:
            rows = self._db._execute(
                """SELECT t.id, t.name, COUNT(ft.path) AS usage_count
                   FROM tags t
                   LEFT JOIN file_tags ft ON ft.tag_id = t.id
                     AND ft.path LIKE ?
                   GROUP BY t.id, t.name
                   ORDER BY t.name""",
                (f"{folder_path}%",),
            ).fetchall()
        else:
            rows = self._db._execute(
                """SELECT t.id, t.name, COUNT(ft.path) AS usage_count
                   FROM tags t
                   LEFT JOIN file_tags ft ON ft.tag_id = t.id
                   GROUP BY t.id, t.name
                   ORDER BY t.name""",
            ).fetchall()
        return [{"id": row["id"], "name": row["name"], "usage_count": row["usage_count"]} for row in rows]
