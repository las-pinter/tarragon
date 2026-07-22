"""Query service — SQL filter composition for thumbnail queries.

Combines filename text search, tag ID filters (AND semantics),
color tag filters (AND semantics), and folder filters (OR semantics)
into a single parameterized query.
"""

from __future__ import annotations

import logging
import time
from pathlib import Path
from typing import Any

from tarragon.db._base import normalize_path
from tarragon.db.database import Database

logger = logging.getLogger(__name__)


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
        folder_filters: set[str] | None = None,
        filename_filter: str = "",
        tag_ids: set[int] | None = None,
        color_tags: set[str] | None = None,
    ) -> list[Path]:
        """Query thumbnails with optional filters.

        Parameters
        ----------
        folder_filters:
            Set of folder paths to scope the query.  Thumbnails whose path
            starts with **any** of these strings are included (OR semantics).
            When empty or ``None``, the query spans the **entire database**
            (global mode).
        filename_filter:
            If non-empty, only paths whose **filename** (basename) contains
            this string (case-insensitive) are returned.  The special LIKE
            characters ``%`` and ``_`` are escaped for safe literal matching.
        tag_ids:
            Set of manual tag IDs to require.  **AND** semantics — a file
            must have **all** of the specified tags.
        color_tags:
            Set of auto-color tag names.  **AND** semantics — a file must
            have **all** of the specified color tags.

        Returns
        -------
        list[Path]
            Matching paths ordered alphabetically.
        """
        start = time.perf_counter()
        folder_filters = folder_filters or set()
        logger.debug(
            "Query start: folders=%s, filename_filter=%s, color_tags=%s, tag_ids=%s",
            folder_filters,
            filename_filter,
            color_tags,
            tag_ids,
        )
        tag_ids = tag_ids or set()
        color_tags = color_tags or set()

        conditions: list[str] = []
        params: list[Any] = []

        # ── Folder scope (empty = global / entire DB) ──────────────
        if folder_filters:
            folder_conds: list[str] = []
            for folder in sorted(folder_filters):
                # Normalize to forward slashes so the LIKE pattern matches
                # paths stored in the database (also normalized to '/').
                normalized = normalize_path(folder)
                folder_conds.append("path LIKE ?")
                params.append(f"{normalized.rstrip('/')}/%")
            conditions.append(f"({' OR '.join(folder_conds)})")

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

        # ── Color tag filter (AND semantics) ───────────────────────
        if color_tags:
            placeholders = ",".join("?" * len(color_tags))
            conditions.append(
                "path IN ("
                "SELECT ft.path FROM file_tags ft "
                "JOIN tags t ON t.id = ft.tag_id "
                f"WHERE t.name IN ({placeholders}) "
                "GROUP BY ft.path "
                "HAVING COUNT(DISTINCT t.name) = ?"
                ")"
            )
            params.extend(sorted(color_tags))
            params.append(len(color_tags))

        if conditions:
            sql = f"SELECT path FROM thumbnails WHERE {' AND '.join(conditions)} ORDER BY path"
        else:
            sql = "SELECT path FROM thumbnails ORDER BY path"

        rows = self._db.fetch_all(sql, tuple(params))
        elapsed = time.perf_counter() - start
        results = [Path(row["path"]) for row in rows]
        logger.debug("Query returned %d results in %.3fs", len(results), elapsed)
        return results
