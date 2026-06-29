"""Query service — SQL filter composition for thumbnail queries.

Combines filename text search, tag ID filters (AND semantics), and
color tag filters (OR semantics) into a single parameterized query.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from tarragon.db import Database


class QueryService:
    """Service-layer wrapper for composing SQL filter queries on thumbnails.

    Each filter (filename, tag ids, color tags) is a composable condition
    that gets combined into a single parameterised SQL statement.  The class
    owns no connection state — it delegates to the injected *Database* for all
    execution.
    """

    def __init__(self, db: Database) -> None:
        self._db = db

    def query(
        self,
        folder_path: str,
        filename_filter: str = "",
        tag_ids: set[int] | None = None,
        color_tags: set[str] | None = None,
    ) -> list[Path]:
        """Query thumbnails with optional filters.

        Parameters
        ----------
        folder_path:
            Root folder to scope the query.  Only thumbnails whose path
            starts with this string are considered.  When empty or ``None``,
            the query spans the **entire database** (global mode).
        filename_filter:
            If non-empty, only paths whose **filename** (basename) contains
            this string (case-insensitive) are returned.  The special LIKE
            characters ``%`` and ``_`` are escaped for safe literal matching.
        tag_ids:
            Set of manual tag IDs to require.  **AND** semantics — a file
            must have **all** of the specified tags.
        color_tags:
            Set of auto-color tag names.  **OR** semantics — a file needs
            **any** of the specified colour tags.

        Returns
        -------
        list[Path]
            Matching paths ordered alphabetically.
        """
        tag_ids = tag_ids or set()
        color_tags = color_tags or set()

        conditions: list[str] = []
        params: list[Any] = []

        # ── Folder scope (empty = global / entire DB) ──────────────
        if folder_path:
            conditions.append("path LIKE ?")
            params.append(f"{folder_path}%")

        # ── Filename filter (applied to basename via %/%) ──────────
        if filename_filter:
            escaped = filename_filter.replace("%", "\\%").replace("_", "\\_")
            conditions.append("path LIKE ? ESCAPE '\\'")
            params.append(f"%{escaped}%")

        # ── Tag ID filter (AND semantics) ──────────────────────────
        if tag_ids:
            placeholders = ",".join("?" * len(tag_ids))
            conditions.append(
                "path IN ("
                "SELECT ft.path FROM file_tags ft "
                f"WHERE ft.tag_id IN ({placeholders}) "
                "GROUP BY ft.path "
                "HAVING COUNT(DISTINCT ft.tag_id) = ?"
                ")"
            )
            params.extend(sorted(tag_ids))
            params.append(len(tag_ids))

        # ── Color tag filter (OR semantics) ────────────────────────
        if color_tags:
            placeholders = ",".join("?" * len(color_tags))
            conditions.append(
                "path IN ("
                "SELECT ft.path FROM file_tags ft "
                "JOIN tags t ON t.id = ft.tag_id "
                f"WHERE t.name IN ({placeholders})"
                ")"
            )
            params.extend(sorted(color_tags))

        if conditions:
            sql = f"SELECT path FROM thumbnails WHERE {' AND '.join(conditions)} ORDER BY path"
        else:
            sql = "SELECT path FROM thumbnails ORDER BY path"

        rows = self._db._execute(sql, tuple(params)).fetchall()
        return [Path(row["path"]) for row in rows]
