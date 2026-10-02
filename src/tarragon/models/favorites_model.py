"""QAbstractListModel backed by the database favorites table.

Provides a model for displaying and managing the user's favorite folders.
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path
from typing import Any, override

from PySide6.QtCore import (
    QAbstractListModel,
    QMimeData,
    QModelIndex,
    QPersistentModelIndex,
    Qt,
)

from tarragon.db._base import normalize_path
from tarragon.db.database import Database

# Custom mime type carrying the dragged favorite's path so internal reorders
# can be decoded without depending on Qt's binary item-data serialization.
FAVORITE_MIME_TYPE = "application/x-tarragon-favorite-path"


class FavoritesModel(QAbstractListModel):
    """A list model backed by the database favorites table.

    Provides two roles:
        - ``DisplayRole``: the user-provided label (or file basename if unset)
        - ``UserRole``:    the full path string

    Single-row mutations (add/remove/move) announce row-level model signals
    instead of a full reset; drag-and-drop reorder is supported through the
    ``FAVORITE_MIME_TYPE`` mime payload.
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
    # Drag and drop
    # -------------------------------------------------------------------------

    @override
    def flags(self, index: QModelIndex | QPersistentModelIndex) -> Qt.ItemFlag:
        """Enable drag and drop so rows can be reordered in the view."""
        item_flags = super().flags(index)
        item_flags |= Qt.ItemFlag.ItemIsDragEnabled
        item_flags |= Qt.ItemFlag.ItemIsDropEnabled
        return item_flags

    @override
    def supportedDropActions(self) -> Qt.DropAction:
        """Only internal moves are supported: reordering, not copying."""
        return Qt.DropAction.MoveAction

    @override
    def mimeTypes(self) -> list[str]:
        """Return the custom favorite-path mime type used for reorder drags."""
        return [FAVORITE_MIME_TYPE]

    @override
    def mimeData(self, indexes: Sequence[QModelIndex]) -> QMimeData:
        """Encode the dragged favorite's path into a mime payload.

        Only the first index is encoded: the sidebar list view runs in the
        default single-selection mode, so a drag always carries exactly one
        row (``len(indexes) == 1``).
        """
        mime = QMimeData()
        if indexes:
            index = indexes[0]
            if index.isValid() and 0 <= index.row() < len(self._favorites):
                mime.setData(FAVORITE_MIME_TYPE, self._favorites[index.row()]["path"].encode("utf-8"))
        return mime

    @override
    def canDropMimeData(
        self,
        data: QMimeData,
        action: Qt.DropAction,
        row: int,
        column: int,
        parent: QModelIndex | QPersistentModelIndex,
    ) -> bool:
        """Accept only internal move drops carrying a favorite path."""
        del action, row, column, parent
        return data.hasFormat(FAVORITE_MIME_TYPE)

    @override
    def dropMimeData(
        self,
        data: QMimeData,
        action: Qt.DropAction,
        row: int,
        column: int,
        parent: QModelIndex | QPersistentModelIndex,
    ) -> bool:
        """Translate an internal drop into ``move_favorite``.

        QAbstractItemView reports the drop as an insertion row in the current
        list (``row``, or the item's row when dropped onto it); the final
        index shifts down by one when the row moves past its own position.
        Drop positions that cannot map to a real insertion point (``row``
        beyond the list, or a negative row with no item parent) are rejected.
        """
        if action == Qt.DropAction.IgnoreAction:
            return True
        if column > 0 or not data.hasFormat(FAVORITE_MIME_TYPE):
            return False

        path = bytes(data.data(FAVORITE_MIME_TYPE).data()).decode("utf-8")
        from_row = next(
            (i for i, fav in enumerate(self._favorites) if fav["path"] == path),
            -1,
        )
        if from_row < 0:
            return False

        row_count = self.rowCount(QModelIndex())
        # Reject positions that cannot map to an insertion point before
        # converting them: a row beyond the end of the list, or a negative
        # row reported without an item parent. Accepting these would clamp
        # them into a phantom "last" position and reorder the model even
        # though no view-visible drop occurred.
        if row > row_count or (row < 0 and not parent.isValid()):
            return False

        if parent.isValid():
            dest_row = parent.row()
        else:
            dest_row = row

        if from_row < dest_row:
            final_row = dest_row - 1
        else:
            final_row = dest_row
        final_row = min(max(final_row, 0), row_count - 1)

        self.move_favorite(from_row, final_row)
        return True

    # -------------------------------------------------------------------------
    # Mutators
    # -------------------------------------------------------------------------

    def add_favorite(self, path: str, label: str | None = None) -> None:
        """Append a new favorite at the end of the list.

        The database-level ``INSERT OR IGNORE`` semantics are mirrored here:
        adding a path that is already present is a no-op so the view and the
        database cannot diverge.

        Args:
            path: Filesystem path to add as a favorite.
            label: Optional human-readable label. Falls back to basename.
        """
        path = normalize_path(path)
        if any(fav["path"] == path for fav in self._favorites):
            return

        # First append lands at sort_order 1; reorder_favorites later
        # normalizes to 0..n-1 — do not "fix" the default to 0, that would
        # make consecutive appends collide on the same order.
        next_order = max((fav["sort_order"] for fav in self._favorites), default=0) + 1
        row = len(self._favorites)

        # Persist first: if the database insert fails the exception
        # propagates and the model is left untouched (no phantom row, no
        # row-level signals for an uncommitted append).
        self._db.add_favorite(path, label=label, sort_order=next_order)

        self.beginInsertRows(QModelIndex(), row, row)
        self._favorites.append({"path": path, "label": label, "sort_order": next_order})
        self.endInsertRows()

    def remove_favorite(self, row: int) -> None:
        """Remove the favorite at *row* via row-level signals.

        Args:
            row: Index of the favorite to remove.
        """
        if not (0 <= row < len(self._favorites)):
            return

        path = self._favorites[row]["path"]
        # Persist first: if the database delete fails the exception
        # propagates and the row stays visible until the delete commits.
        self._db.remove_favorite(path)

        self.beginRemoveRows(QModelIndex(), row, row)
        del self._favorites[row]
        self.endRemoveRows()

    def move_favorite(self, from_row: int, to_row: int) -> None:
        """Move a favorite to *to_row* and persist the reordered list.

        *to_row* is the final index of the moved row in the resulting list.
        The reordered path list is persisted through ``reorder_favorites``,
        which reindexes sort orders 0..n-1 in a single batched update.

        Args:
            from_row: Current index of the favorite to move.
            to_row: Desired final index for the moved favorite.
        """
        row_count = len(self._favorites)
        if not (0 <= from_row < row_count) or not (0 <= to_row < row_count):
            return
        if from_row == to_row:
            return

        # Persist the reindexed order first: if the database write fails the
        # exception propagates and the in-memory list is untouched (no
        # moved-but-unpersisted state). Once committed, the row-level signals
        # below cannot fail for the already-validated from_row/to_row range.
        reordered = list(self._favorites)
        favorite = reordered.pop(from_row)
        reordered.insert(to_row, favorite)
        self._db.reorder_favorites([fav["path"] for fav in reordered])

        # Qt's beginMoveRows announces an insertion point in the current
        # list; a downward move must account for the removed row (+1).
        if from_row < to_row:
            dest_child = to_row + 1
        else:
            dest_child = to_row

        if not self.beginMoveRows(QModelIndex(), from_row, from_row, QModelIndex(), dest_child):
            return

        favorite = self._favorites.pop(from_row)
        insert_at = dest_child - 1 if from_row < dest_child else dest_child
        self._favorites.insert(insert_at, favorite)
        self.endMoveRows()

    def load_from_db(self) -> None:
        """Reload all favorites from the database into the internal list."""
        self.beginResetModel()
        self._favorites = list(self._db.list_favorites())
        self.endResetModel()

    def favorite_paths(self) -> list[str]:
        """Return a list of all stored favorite paths."""
        return [fav["path"] for fav in self._favorites]
