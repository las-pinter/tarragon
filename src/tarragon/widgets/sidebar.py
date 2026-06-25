"""SidebarWidget — Favorites sidebar with model/view separation.

Provides a ``FavoritesModel`` (``QAbstractListModel``) backed by the database
favorites repository, and a ``SidebarWidget`` that renders it with add/remove
controls and emits a signal when a favorite is double-clicked.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, override

from PySide6.QtCore import QAbstractListModel, QModelIndex, Qt, Signal
from PySide6.QtWidgets import (
    QHBoxLayout,
    QLabel,
    QListView,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from tarragon.db import Database


class FavoritesModel(QAbstractListModel):
    """A list model backed by the database favorites table.

    Provides two roles:
        - ``DisplayRole``: the user-provided label (or file basename if unset)
        - ``UserRole``:    the full path string
    """

    def __init__(self, db: Database, parent: QWidget | None = None) -> None:
        """Initialise the model with a database reference and load existing data."""
        super().__init__(parent)
        self._db = db
        self._favorites: list[dict[str, Any]] = []
        self.load_from_db()

    @override
    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802
        """Return the number of favorite entries."""
        return len(self._favorites)

    @override
    def data(self, index: QModelIndex, role: int = Qt.DisplayRole) -> Any:  # noqa: N802
        """Return data for *index* according to *role*.

        * ``DisplayRole`` → user label, or file basename if label is ``None``
        * ``UserRole``    → full path string
        """
        if not index.isValid() or not (0 <= index.row() < len(self._favorites)):
            return None

        fav = self._favorites[index.row()]

        if role == Qt.DisplayRole:
            label = fav.get("label")
            if label:
                return label
            return Path(fav["path"]).name

        if role == Qt.UserRole:
            return fav["path"]

        return None

    # ── Mutators ────────────────────────────────────────────────

    def add_favorite(self, path: str, label: str | None = None) -> None:
        """Persist a new favorite via the database and refresh the model."""
        self._db.add_favorite(path, label=label)
        self.load_from_db()

    def remove_favorite(self, path: str) -> None:
        """Remove a favorite via the database and refresh the model."""
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


class SidebarWidget(QWidget):
    """A sidebar panel that shows a list of favorite folders.

    Emits ``favorite_clicked(str)`` when the user double-clicks a row.
    """

    favorite_clicked = Signal(str)  # path string

    def __init__(self, db: Database, parent: QWidget | None = None) -> None:
        """Build the sidebar layout with model, list view, and action buttons."""
        super().__init__(parent)
        self._db = db
        self._current_folder: str | None = None

        self._model = FavoritesModel(db, parent=self)

        # ── Layout ──────────────────────────────────────────────
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        # Header
        header = QLabel("Favorites")
        layout.addWidget(header)

        # List view
        self._list_view = QListView()
        self._list_view.setModel(self._model)
        self._list_view.doubleClicked.connect(self._on_favorite_double_clicked)
        layout.addWidget(self._list_view, stretch=1)

        # Buttons
        button_layout = QHBoxLayout()

        self._add_button = QPushButton("Add Current Folder")
        self._add_button.clicked.connect(self._on_add_clicked)
        button_layout.addWidget(self._add_button)

        self._remove_button = QPushButton("Remove")
        self._remove_button.clicked.connect(self._on_remove_clicked)
        button_layout.addWidget(self._remove_button)

        layout.addLayout(button_layout)

    # ── Public API ──────────────────────────────────────────────

    def set_current_folder(self, path: str) -> None:
        """Store the currently active folder path for the *Add* button."""
        self._current_folder = path

    # ── Slots ───────────────────────────────────────────────────

    def _on_add_clicked(self) -> None:
        """Add the current folder to favorites if one is set."""
        if self._current_folder is not None:
            self._model.add_favorite(self._current_folder)

    def _on_remove_clicked(self) -> None:
        """Remove the currently selected favorite from the list."""
        indexes = self._list_view.selectedIndexes()
        if indexes:
            path = indexes[0].data(Qt.UserRole)
            if path:
                self._model.remove_favorite(path)

    def _on_favorite_double_clicked(self, index: QModelIndex) -> None:
        """Emit ``favorite_clicked`` with the path of the double-clicked item."""
        if index.isValid():
            path = index.data(Qt.UserRole)
            if path:
                self.favorite_clicked.emit(path)
