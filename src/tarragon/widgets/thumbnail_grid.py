"""Thumbnail grid widget — icon-mode QListView with custom cell rendering."""

from __future__ import annotations

from PySide6.QtCore import QEvent, QItemSelection, QModelIndex, QRect, QSize, Qt, Signal
from PySide6.QtGui import QColor, QHelpEvent, QMouseEvent, QPainter, QPen, QPixmap
from PySide6.QtWidgets import (
    QAbstractItemView,
    QListView,
    QStyle,
    QStyledItemDelegate,
    QStyleOptionViewItem,
    QToolTip,
)

from tarragon.models.thumbnail_model import ThumbnailModel

# Theme tokens (coral-amber dark palette)
CORAL_STRONG = QColor("#E76F51")
AMBER_ACCENT = QColor("#F4A261")
BG_PRIMARY = QColor("#16151A")
BG_SECONDARY = QColor("#242329")
TEXT_PRIMARY = QColor("#EAE8EB")
TEXT_SECONDARY = QColor("#A09CA3")
PSD_BADGE_COLOR = QColor("#E76F51")
THUMBNAIL_SIZE = 160
GRID_GAP = 8


class ThumbnailDelegate(QStyledItemDelegate):
    """Custom delegate that paints thumbnail images with file info overlays."""

    def __init__(self, parent: object | None = None) -> None:
        super().__init__(parent)
        self._hovered_row: int = -1

    def set_hovered_row(self, row: int) -> None:
        """Record which row is currently hovered for hover-effect painting."""
        self._hovered_row = row

    def paint(
        self,
        painter: QPainter,
        option: QStyleOptionViewItem,
        index: QModelIndex,
    ) -> None:
        """Paint the thumbnail cell: background, image, name, PSD badge, selection border."""
        painter.save()

        # ── Cell background ──────────────────────────────────────────
        is_hovered = index.row() == self._hovered_row
        is_selected = bool(option.state & QStyle.StateFlag.State_Selected)

        if is_selected:
            painter.fillRect(option.rect, BG_SECONDARY)
        elif is_hovered:
            painter.fillRect(option.rect, BG_SECONDARY.lighter(110))
        else:
            painter.fillRect(option.rect, BG_PRIMARY)

        # ── Compute image area ───────────────────────────────────────
        cell_rect = option.rect.adjusted(GRID_GAP, GRID_GAP, -GRID_GAP, -GRID_GAP)
        image_size = THUMBNAIL_SIZE
        image_rect = QRect(
            cell_rect.x(),
            cell_rect.y(),
            image_size,
            image_size,
        )

        # ── Draw thumbnail image ─────────────────────────────────────
        thumbnail_data = index.data(ThumbnailModel.PathRole)
        if thumbnail_data:
            pixmap = QPixmap(thumbnail_data)
            if not pixmap.isNull():
                scaled = pixmap.scaled(
                    image_size,
                    image_size,
                    Qt.AspectRatioMode.KeepAspectRatio,
                    Qt.TransformationMode.SmoothTransformation,
                )
                # Center the scaled image in the image rect
                x_offset = image_rect.x() + (image_size - scaled.width()) // 2
                y_offset = image_rect.y() + (image_size - scaled.height()) // 2
                painter.drawPixmap(x_offset, y_offset, scaled)

        # ── File basename text ───────────────────────────────────────
        name = index.data(Qt.ItemDataRole.DisplayRole) or ""
        painter.setPen(TEXT_PRIMARY)
        font = painter.font()
        font.setPointSize(10)
        painter.setFont(font)
        text_rect = QRect(
            cell_rect.x(),
            image_rect.bottom() + 4,
            image_size,
            20,
        )
        metrics = painter.fontMetrics()
        elided_name = metrics.elidedText(name, Qt.TextElideMode.ElideRight, image_size)
        painter.drawText(
            text_rect,
            Qt.AlignmentFlag.AlignHCenter | Qt.AlignmentFlag.AlignTop,
            elided_name,
        )

        # ── PSD/PSB badge ────────────────────────────────────────────
        path_str = index.data(ThumbnailModel.PathRole) or ""
        if path_str.lower().endswith((".psd", ".psb")):
            badge_rect = QRect(
                image_rect.right() - 30,
                image_rect.top() + 4,
                26,
                14,
            )
            painter.fillRect(badge_rect, PSD_BADGE_COLOR)
            painter.setPen(Qt.GlobalColor.white)
            small_font = painter.font()
            small_font.setPointSize(7)
            small_font.setBold(True)
            painter.setFont(small_font)
            ext = path_str.rsplit(".", 1)[-1].upper()  # "PSD" or "PSB"
            painter.drawText(badge_rect, Qt.AlignmentFlag.AlignCenter, ext)

        # ── Selection border (coral_strong, 1.5px) ───────────────────
        if is_selected:
            pen = QPen(CORAL_STRONG, 1.5)
            painter.setPen(pen)
            painter.setBrush(Qt.BrushStyle.NoBrush)
            border_rect = option.rect.adjusted(1, 1, -1, -1)
            painter.drawRect(border_rect)

        painter.restore()

    def sizeHint(  # noqa: N802
        self,
        option: QStyleOptionViewItem,
        index: QModelIndex,
    ) -> QSize:
        """Return the size of each grid cell."""
        return QSize(THUMBNAIL_SIZE + GRID_GAP * 2, THUMBNAIL_SIZE + GRID_GAP * 2 + 24)

    def helpEvent(  # noqa: N802
        self,
        event: QHelpEvent,
        view: QAbstractItemView,
        option: QStyleOptionViewItem,
        index: QModelIndex,
    ) -> bool:
        """Show tooltip with full filename on hover."""
        if event.type() == QEvent.Type.ToolTip:
            name = index.data(Qt.ItemDataRole.DisplayRole) or ""
            if name:
                QToolTip.showText(event.globalPos(), name, view)
            return True
        return super().helpEvent(event, view, option, index)


class ThumbnailGrid(QListView):
    """Icon-mode list view configured as a thumbnail gallery with a custom delegate.

    Emits ``file_double_clicked(str)`` with the file path when an item is double-clicked
    (wired for external editor launch).
    Emits ``selection_changed(list)`` with selected path strings when selection changes.
    """

    selection_changed = Signal(list)  # list of selected path strings
    file_double_clicked = Signal(str)  # emits file path on double-click

    def __init__(self, parent: object | None = None) -> None:
        super().__init__(parent)
        self.setViewMode(QListView.ViewMode.IconMode)
        self.setIconSize(QSize(THUMBNAIL_SIZE, THUMBNAIL_SIZE))
        self.setGridSize(QSize(THUMBNAIL_SIZE + GRID_GAP * 2, THUMBNAIL_SIZE + GRID_GAP * 2 + 24))
        self.setWrapping(True)
        self.setResizeMode(QListView.ResizeMode.Adjust)
        self.setSelectionMode(QListView.SelectionMode.ExtendedSelection)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setSpacing(0)
        self.setUniformItemSizes(True)

        self._delegate = ThumbnailDelegate(self)
        self.setItemDelegate(self._delegate)

        # Track hover for visual feedback
        self.setMouseTracking(True)

    def set_model(self, model: ThumbnailModel) -> None:
        """Convenience wrapper for setModel()."""
        self.setModel(model)

    def mouseMoveEvent(self, event: QMouseEvent) -> None:  # noqa: N802
        """Track hovered item for visual feedback."""
        index = self.indexAt(event.position().toPoint())
        new_row = index.row() if index.isValid() else -1
        if new_row != self._delegate._hovered_row:
            self._delegate.set_hovered_row(new_row)
            self.viewport().update()
        super().mouseMoveEvent(event)

    def leaveEvent(self, event: QEvent) -> None:  # noqa: N802
        """Reset hover state when mouse leaves the widget."""
        self._delegate.set_hovered_row(-1)
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
