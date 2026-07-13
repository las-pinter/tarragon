"""Favorites CRUD operations — mixed into the Database class."""

from __future__ import annotations

import logging
from typing import Any

from tarragon.db._base import _MixinBase, _normalize_path, _row_to_dict

logger = logging.getLogger(__name__)


class FavoritesMixin(_MixinBase):
    """Add, remove, and list favorite records."""

    def add_favorite(
        self,
        path: str,
        label: str | None = None,
        sort_order: int = 0,
    ) -> None:
        """Add a file to favorites."""
        path = _normalize_path(path)
        logger.debug("add_favorite: path=%s, label=%s", path, label)
        self._execute(
            "INSERT OR IGNORE INTO favorites (path, label, sort_order) VALUES (?, ?, ?)",
            (path, label, sort_order),
        )
        self._commit()

    def remove_favorite(self, path: str) -> None:
        """Remove a file from favorites."""
        path = _normalize_path(path)
        logger.debug("remove_favorite: path=%s", path)
        self._execute("DELETE FROM favorites WHERE path = ?", (path,))
        self._commit()

    def list_favorites(self) -> list[dict[str, Any]]:
        """Return all favorite records ordered by sort_order then path."""
        logger.debug("list_favorites")
        cursor = self._execute(
            "SELECT * FROM favorites ORDER BY sort_order, path"
        )
        return [_row_to_dict(row) for row in cursor.fetchall()]
