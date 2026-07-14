"""Thumbnail grid widget — icon-mode QListView with custom cell rendering."""

from __future__ import annotations

from PySide6.QtCore import (
    QEvent,
    QItemSelection,
    QModelIndex,
    QSize,
    Qt,
    Signal,
)
from PySide6.QtGui import (
    QAction,
    QContextMenuEvent,
    QMouseEvent,
)
from PySide6.QtWidgets import (
    QListView,
    QMenu,
    QWidget,
)

from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.widgets.thumbnail_animator import ThumbnailAnimator
from tarragon.theme.constants import GRID_GAP, THUMBNAIL_SIZE
from tarragon.widgets.thumbnail_delegate import (
    HOVER_MARGIN,
    TEXT_AREA_HEIGHT,
    ThumbnailDelegate,
)


class ThumbnailGrid(QListView):
    """Icon-mode list view configured as a thumbnail gallery with a custom delegate.

    Emits ``file_double_clicked(str)`` with the file path when an item is double-clicked
    (wired for external editor launch).
    Emits ``selection_changed(list)`` with selected path strings when selection changes.
    """

    selection_changed = Signal(list)  # list of selected path strings
    file_double_clicked = Signal(str)  # emits file path on double-click
    regenerate_requested = Signal(str)  # emits file path on "Regenerate Thumbnail" action

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setViewMode(QListView.ViewMode.IconMode)
        self.setIconSize(QSize(THUMBNAIL_SIZE, THUMBNAIL_SIZE))
        self.setGridSize(
            QSize(
                THUMBNAIL_SIZE + GRID_GAP * 2 + HOVER_MARGIN * 2,
                THUMBNAIL_SIZE + GRID_GAP * 2 + TEXT_AREA_HEIGHT + HOVER_MARGIN * 2,
            )
        )
        self.setWrapping(True)
        self.setResizeMode(QListView.ResizeMode.Adjust)
        self.setSelectionMode(QListView.SelectionMode.ExtendedSelection)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setSpacing(GRID_GAP)
        self.setUniformItemSizes(True)

        # Animation controller — drives hover-scale and fade-in effects
        self._animator = ThumbnailAnimator(self)

        self._delegate = ThumbnailDelegate(self, animator=self._animator)
        self.setItemDelegate(self._delegate)

        # Track hover for visual feedback
        self.setMouseTracking(True)

    def set_model(self, model: ThumbnailModel) -> None:
        """Convenience wrapper for setModel().

        Connects model signals so the animator can trigger fade-in for new rows
        and clear tracking on model reset.
        """
        # Disconnect previous model signals if a model was already set
        old_model = self.model()
        if old_model is not None:
            try:
                old_model.modelAboutToBeReset.disconnect(self._animator.notify_rows_about_to_reset)
                old_model.rowsInserted.disconnect(self._on_rows_inserted)
            except RuntimeError:
                pass  # Signal was not connected

        self.setModel(model)

        # Connect animator to model lifecycle signals
        model.modelAboutToBeReset.connect(self._animator.notify_rows_about_to_reset)
        model.rowsInserted.connect(self._on_rows_inserted)

    def _on_rows_inserted(self, parent: QModelIndex, first: int, last: int) -> None:
        """Handle new rows being added to the model — start fade-in animations."""
        # Only animate top-level rows (parent is invalid for flat list models)
        if not parent.isValid():
            self._animator.notify_rows_inserted(first, last)

    def mouseMoveEvent(self, event: QMouseEvent) -> None:  # noqa: N802
        """Track hovered item for visual feedback and hover-scale animation."""
        index = self.indexAt(event.position().toPoint())
        new_row = index.row() if index.isValid() else -1
        old_row = self._delegate._hovered_row
        if new_row != old_row:
            self._delegate.set_hovered_row(new_row)
            self._animator.start_hover(new_row, old_row)
            self.viewport().update()
        super().mouseMoveEvent(event)

    def leaveEvent(self, event: QEvent) -> None:  # noqa: N802
        """Reset hover state when mouse leaves the widget."""
        old_row = self._delegate._hovered_row
        self._delegate.set_hovered_row(-1)
        self._animator.start_hover(-1, old_row)
        self.viewport().update()
        super().leaveEvent(event)

    def mouseDoubleClickEvent(self, event: QMouseEvent) -> None:  # noqa: N802
        """Emit file_double_clicked signal with the path of the double-clicked item."""
        index = self.indexAt(event.position().toPoint())
        if index.isValid():
            path = index.data(ThumbnailModel.PathRole)
            if path:
                self.file_double_clicked.emit(path)
        super().mouseDoubleClickEvent(event)

    def contextMenuEvent(self, event: QContextMenuEvent) -> None:  # noqa: N802
        """Show a context menu with 'Regenerate Thumbnail' when right-clicking an item."""
        index = self.indexAt(event.pos())
        if not index.isValid():
            return
        path = index.data(ThumbnailModel.PathRole)
        if not path:
            return

        menu = QMenu(self)
        regenerate_action = QAction("Regenerate Thumbnail", self)
        regenerate_action.triggered.connect(lambda: self.regenerate_requested.emit(path))
        menu.addAction(regenerate_action)
        menu.exec(event.globalPos())

    def selectionChanged(  # noqa: N802
        self,
        selected: QItemSelection,
        deselected: QItemSelection,
    ) -> None:
        """Emit signal with currently selected paths when selection changes."""
        super().selectionChanged(selected, deselected)
        paths = []
        for index in self.selectedIndexes():
            path = index.data(ThumbnailModel.PathRole)
            if path:
                paths.append(path)
        self.selection_changed.emit(paths)
