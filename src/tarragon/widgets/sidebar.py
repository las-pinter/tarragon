"""SidebarWidget — Favorites sidebar with model/view separation.

Provides a ``FavoritesModel`` (``QAbstractListModel``) backed by the database
favorites repository, and a ``SidebarWidget`` that renders it with add/remove
controls and emits a signal when a favorite is clicked.

The sidebar also includes a navigable folder tree (``QTreeView`` backed by
``QFileSystemModel``) for browsing the local filesystem.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, override

from PySide6.QtCore import QAbstractListModel, QDir, QModelIndex, QPersistentModelIndex, Qt, Signal
from PySide6.QtWidgets import (
    QFileSystemModel,
    QHBoxLayout,
    QLabel,
    QListView,
    QPushButton,
    QTreeView,
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
    def rowCount(self, parent: QModelIndex | QPersistentModelIndex = QPersistentModelIndex()) -> int:  # noqa: N802
        """Return the number of favorite entries."""
        return len(self._favorites)

    @override
    def data(self, index: QModelIndex | QPersistentModelIndex, role: int = Qt.ItemDataRole.DisplayRole) -> Any:  # noqa: N802
        """Return data for *index* according to *role*.

        * ``DisplayRole`` → user label, or file basename if label is ``None``
        * ``UserRole``    → full path string
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
    """A sidebar panel that shows a folder tree and a list of favorite folders.

    Emits ``favorite_clicked(str)`` when the user clicks a favorite row.
    Emits ``folder_navigated(str)`` when the user clicks a folder in the tree.
    """

    favorite_clicked = Signal(str)  # path string
    folder_navigated = Signal(str)  # path string

    def __init__(self, db: Database, parent: QWidget | None = None) -> None:
        """Build the sidebar layout with folder tree, model, list view, and action buttons."""
        super().__init__(parent)
        self._db = db
        self._current_folder: str | None = None

        self._model = FavoritesModel(db, parent=self)

        # ── Layout ──────────────────────────────────────────────
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        # ── Folder tree ─────────────────────────────────────────
        tree_header = QLabel("Folders")
        layout.addWidget(tree_header)

        self._folder_model = QFileSystemModel()
        self._folder_model.setFilter(QDir.Filter.Dirs | QDir.Filter.NoDotAndDotDot)
        self._folder_model.setRootPath("")

        self._folder_tree = QTreeView()
        self._folder_tree.setModel(self._folder_model)
        self._folder_tree.setHeaderHidden(True)
        # Hide all columns except the name column (column 0)
        for col in range(1, self._folder_model.columnCount()):
            self._folder_tree.hideColumn(col)
        self._folder_tree.clicked.connect(self._on_folder_clicked)
        layout.addWidget(self._folder_tree, stretch=1)

        # ── Favorites ───────────────────────────────────────────
        header = QLabel("Favorites")
        layout.addWidget(header)

        # List view
        self._list_view = QListView()
        self._list_view.setModel(self._model)
        self._list_view.clicked.connect(self._on_favorite_clicked)
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
            path = indexes[0].data(Qt.ItemDataRole.UserRole)
            if path:
                self._model.remove_favorite(path)

    def _on_favorite_clicked(self, index: QModelIndex) -> None:
        """Emit ``favorite_clicked`` with the path of the clicked item."""
        if index.isValid():
            path = index.data(Qt.ItemDataRole.UserRole)
            if path:
                self.favorite_clicked.emit(path)

    def _on_folder_clicked(self, index: QModelIndex) -> None:
        """Emit ``folder_navigated`` with the path of the clicked folder."""
        if index.isValid():
            path = self._folder_model.filePath(index)
            if path:
                self.folder_navigated.emit(path)
