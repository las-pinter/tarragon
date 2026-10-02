"""Favorites CRUD operations mixed into the Database class."""

from __future__ import annotations

import logging
from typing import Any

from tarragon.db._base import MixinBase, normalize_path

logger = logging.getLogger(__name__)


class FavoritesMixin(MixinBase):
    """Add, remove, and list favorite records."""

    def add_favorite(
        self,
        path: str,
        label: str | None = None,
        sort_order: int = 0,
    ) -> None:
        """Add a file to favorites."""
        path = normalize_path(path)
        logger.debug("Called - path: %s, label: %s", path, label)
        self._execute(
            "INSERT OR IGNORE INTO favorites (path, label, sort_order) VALUES (?, ?, ?)",
            (path, label, sort_order),
        )
        self._commit()

    def remove_favorite(self, path: str) -> None:
        """Remove a file from favorites."""
        path = normalize_path(path)
        logger.debug("Called - path: %s", path)
        self._execute("DELETE FROM favorites WHERE path = ?", (path,))
        self._commit()

    def update_favorite_sort_order(self, path: str, sort_order: int) -> None:
        """Set the sort order for a single favorite record.

        Single-row counterpart to :meth:`reorder_favorites` for callers that
        must not reindex (and re-commit) the whole list.
        """
        path = normalize_path(path)
        logger.debug("Called - path: %s, sort_order: %d", path, sort_order)
        self._execute(
            "UPDATE favorites SET sort_order = ? WHERE path = ?",
            (sort_order, path),
        )
        self._commit()

    def reorder_favorites(self, paths: list[str]) -> None:
        """Reindex favorite sort orders to 0..n-1 in the given display order.

        Args:
            paths: Favorite paths in the desired display order. Each listed
                path gets a sort_order matching its index, so a full reorder
                persists as one batched ``UPDATE`` plus a single commit.
        """
        logger.debug("Called - paths: %s", paths)
        self._executemany(
            "UPDATE favorites SET sort_order = ? WHERE path = ?",
            [(index, normalize_path(path)) for index, path in enumerate(paths)],
        )
        self._commit()

    def list_favorites(self) -> list[dict[str, Any]]:
        """Return all favorite records ordered by sort_order then path."""
        logger.debug("Called")
        return self._fetch_all_locked("SELECT * FROM favorites ORDER BY sort_order, path")
