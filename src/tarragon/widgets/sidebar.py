"""SidebarWidget — Favorites sidebar with model/view separation.

Provides a ``SidebarWidget`` that renders favorites with add/remove
controls and emits a signal when a favorite is clicked.

The sidebar also includes a navigable folder tree (``QTreeView`` backed by
``QFileSystemModel``) for browsing the local filesystem.

A custom :class:`SidebarItemDelegate` paints folder icons with selection-aware
colours (coral when selected, muted when not) to match the mockup aesthetic.

``FavoritesModel`` has been moved to :mod:`tarragon.models.favorites_model`
but is re-exported here for backward compatibility.
"""

from __future__ import annotations

from typing import override

from PySide6.QtCore import QDir, QModelIndex, QPersistentModelIndex, QSize, Qt, Signal
from PySide6.QtGui import QColor, QIcon, QPainter, QPixmap
from PySide6.QtWidgets import (
    QApplication,
    QFileSystemModel,
    QHBoxLayout,
    QLabel,
    QListView,
    QPushButton,
    QStyle,
    QStyledItemDelegate,
    QStyleOptionViewItem,
    QTreeView,
    QVBoxLayout,
    QWidget,
)

from tarragon.db.database import Database
from tarragon.models.favorites_model import FavoritesModel  # noqa: F401 — re-exported
from tarragon.theme.colors import CORAL_STRONG, TEXT_SECONDARY


class SidebarItemDelegate(QStyledItemDelegate):
    """Delegate that paints folder icons with selection-aware colours.

    Used in both the favorites list and folder tree to provide consistently
    coloured folder icons that change based on selection state — coral for
    selected items, muted for unselected.
    """

    def __init__(
        self,
        coral_color: QColor,
        muted_color: QColor,
        icon_size: int = 13,
        parent: QWidget | None = None,
    ) -> None:
        """Initialise with selection/unselection colours and icon size.

        Args:
            coral_color: Colour used for the folder icon when the item is selected.
            muted_color: Colour used for the folder icon when the item is not selected.
            icon_size: Pixel size for the folder icon (width and height).
            parent: Optional parent widget.
        """
        super().__init__(parent)
        self._icon_size = icon_size
        self._coral_pixmap = self._tinted_folder_pixmap(coral_color, icon_size)
        self._muted_pixmap = self._tinted_folder_pixmap(muted_color, icon_size)

    @staticmethod
    def _tinted_folder_pixmap(color: QColor, size: int) -> QPixmap:
        """Generate a folder icon pixmap tinted with *color*.

        Renders the platform's standard directory icon at *size* pixels, then
        overlays it with *color* using source-in compositing so only the opaque
        pixels of the original icon are tinted.

        Args:
            color: The tint colour to apply.
            size: Pixel dimensions (square) for the output pixmap.

        Returns:
            A new :class:`QPixmap` with the tinted folder icon.
        """
        app_style = QApplication.style()
        base_icon = app_style.standardIcon(QStyle.StandardPixmap.SP_DirIcon)
        base_pixmap = base_icon.pixmap(size, size)

        tinted = QPixmap(size, size)
        tinted.fill(QColor(0, 0, 0, 0))
        painter = QPainter(tinted)
        painter.drawPixmap(0, 0, base_pixmap)
        painter.setCompositionMode(QPainter.CompositionMode.CompositionMode_SourceIn)
        painter.fillRect(tinted.rect(), color)
        painter.end()
        return tinted

    @override
    def initStyleOption(  # noqa: N802
        self,
        option: QStyleOptionViewItem,
        index: QModelIndex | QPersistentModelIndex,
    ) -> None:
        """Override the item icon based on selection state.

        Sets a 13×13 folder icon coloured coral when selected, muted otherwise.

        Args:
            option: The style option to populate.
            index: The model index for the item.
        """
        super().initStyleOption(option, index)
        option.iconSize = QSize(self._icon_size, self._icon_size)  # type: ignore[attr-defined]
        if option.state & QStyle.StateFlag.State_Selected:
            option.icon = QIcon(self._coral_pixmap)
        else:
            option.icon = QIcon(self._muted_pixmap)


class SidebarWidget(QWidget):
    """A sidebar panel that shows a list of favorite folders and a navigable folder tree.

    Section order (top to bottom): Favorites, then Folders — matching the mockup.
    Emits ``favorite_clicked(str)`` when the user clicks a favorite row.
    Emits ``folder_navigated(str)`` when the user clicks a folder in the tree.
    """

    favorite_clicked = Signal(str)  # path string
    folder_navigated = Signal(str)  # path string

    def __init__(self, db: Database, parent: QWidget | None = None) -> None:
        """Build the sidebar layout with favorites list, folder tree, and action buttons.

        Section order matches the mockup: Favorites FIRST, then Folders.
        Both views use a :class:`SidebarItemDelegate` for selection-aware folder icons.
        """
        super().__init__(parent)
        self._db = db
        self._current_folder: str | None = None

        self._model = FavoritesModel(db, parent=self)

        # Colours for the sidebar icon delegate
        coral_color = CORAL_STRONG  # selected icon
        muted_color = TEXT_SECONDARY  # unselected icon

        # ── Layout ──────────────────────────────────────────────
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # ── Folder tree (SECOND section per mockup) ─────────────
        tree_header = QLabel("Folders")
        tree_header.setObjectName("sidebarSectionHeader")
        layout.addWidget(tree_header)

        self._folder_model = QFileSystemModel()
        self._folder_model.setFilter(QDir.Filter.Dirs | QDir.Filter.NoDotAndDotDot)
        self._folder_model.setRootPath("")

        self._folder_tree = QTreeView()
        self._folder_tree.setObjectName("sidebarFolderTree")
        self._folder_tree.setModel(self._folder_model)
        self._folder_tree.setHeaderHidden(True)
        # Hide all columns except the name column (column 0)
        for col in range(1, self._folder_model.columnCount()):
            self._folder_tree.hideColumn(col)
        self._folder_tree.setItemDelegate(
            SidebarItemDelegate(coral_color, muted_color, parent=self._folder_tree),
        )
        self._folder_tree.clicked.connect(self._on_folder_clicked)
        layout.addWidget(self._folder_tree, stretch=1)

        # Spacing between sections (Folders header needs 14px top margin)
        layout.addSpacing(14)

        # ── Favorites (FIRST section per mockup) ────────────────
        fav_header = QLabel("Favorites")
        fav_header.setObjectName("sidebarSectionHeader")
        layout.addWidget(fav_header)

        # List view
        self._list_view = QListView()
        self._list_view.setObjectName("sidebarFavorites")
        self._list_view.setModel(self._model)
        self._list_view.setItemDelegate(
            SidebarItemDelegate(coral_color, muted_color, parent=self._list_view),
        )
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
