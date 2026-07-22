"""QAbstractListModel backed by the database favorites table.

Provides a model for displaying and managing the user's favorite folders.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, override

from PySide6.QtCore import QAbstractListModel, QModelIndex, QPersistentModelIndex, Qt

from tarragon.db.database import Database


class FavoritesModel(QAbstractListModel):
    """A list model backed by the database favorites table.

    Provides two roles:
        - ``DisplayRole``: the user-provided label (or file basename if unset)
        - ``UserRole``:    the full path string
    """

    def __init__(self, db: Database, parent: Any = None) -> None:
        """Initialise the model with a database reference and load existing data.

        Args:
            db: The database connection to read/write favorites.
            parent: Optional Qt parent object.
        """
        super().__init__(parent)
        self._db = db
        self._favorites: list[dict[str, Any]] = []
        self.load_from_db()

    @override
    def rowCount(self, parent: QModelIndex | QPersistentModelIndex = QPersistentModelIndex()) -> int:
        """Return the number of favorite entries."""
        return len(self._favorites)

    @override
    def data(self, index: QModelIndex | QPersistentModelIndex, role: int = Qt.ItemDataRole.DisplayRole) -> Any:
        """Return data for *index* according to *role*.

        * ``DisplayRole``: user label, or file basename if label is ``None``
        * ``UserRole``:    full path string
        """
        if not index.isValid() or not (0 <= index.row() < len(self._favorites)):
            return None

        fav = self._favorites[index.row()]

        if role == Qt.ItemDataRole.DisplayRole:
            label = fav.get("label")
            if label:
                return label
            return Path(fav["path"]).name

        if role == Qt.ItemDataRole.UserRole:
            return fav["path"]

        return None

    # -------------------------------------------------------------------------
    # Mutators
    # -------------------------------------------------------------------------

    def add_favorite(self, path: str, label: str | None = None) -> None:
        """Persist a new favorite via the database and refresh the model.

        Args:
            path: Filesystem path to add as a favorite.
            label: Optional human-readable label. Falls back to basename.
        """
        self._db.add_favorite(path, label=label)
        self.load_from_db()

    def remove_favorite(self, path: str) -> None:
        """Remove a favorite via the database and refresh the model.

        Args:
            path: Filesystem path of the favorite to remove.
        """
        self._db.remove_favorite(path)
        self.load_from_db()

    def load_from_db(self) -> None:
        """Reload all favorites from the database into the internal list."""
        self.beginResetModel()
        self._favorites = list(self._db.list_favorites())
        self.endResetModel()

    def favorite_paths(self) -> list[str]:
        """Return a list of all stored favorite paths."""
        return [fav["path"] for fav in self._favorites]
