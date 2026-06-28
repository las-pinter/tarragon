"""Thumbnail grid widget — icon-mode QListView with custom cell rendering."""

from __future__ import annotations

from PySide6.QtCore import QEvent, QItemSelection, QModelIndex, QRect, QSize, Qt, QTime, QTimer, Signal
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
CORAL_STRONG = QColor("#F0997B")
AMBER_ACCENT = QColor("#FAC775")
BG_PRIMARY = QColor("#16151A")
BG_SECONDARY = QColor("#1c1b22")
TEXT_PRIMARY = QColor("#ece9f2")
TEXT_SECONDARY = QColor("#A09CA3")
PSD_BADGE_COLOR = QColor("#F0997B")
THUMBNAIL_SIZE = 160
GRID_GAP = 8

# Animation tokens (from tokens.json motion values)
HOVER_SCALE_TARGET = 1.02
HOVER_DURATION_MS = 150  # duration_fast
FADE_DURATION_MS = 200  # fade_in_ms
ANIMATOR_INTERVAL_MS = 16  # ~60fps


def _ease_out(t: float) -> float:
    """Quadratic ease-out: fast start, slow finish."""
    return t * (2.0 - t)


class ThumbnailAnimator:
    """Drives hover-scale and fade-in animations for thumbnail grid cells.

    Since QStyledItemDelegate paints cells immediately (no per-cell QObjects),
    this controller maintains animation state per row and drives repaints via
    a single shared QTimer at ~60fps. The timer auto-stops when no animations
    are active to avoid unnecessary CPU usage.
    """

    def __init__(self, view: QListView) -> None:
        self._view = view
        self._timer = QTimer()
        self._timer.setInterval(ANIMATOR_INTERVAL_MS)
        self._timer.timeout.connect(self._tick)

        # Hover scale animations: row -> animation state dict
        # State: {"current": float, "target": float, "start_val": float, "start_time": QTime}
        self._hover_anims: dict[int, dict] = {}

        # Fade-in animations: row -> animation state dict
        # State: {"current": float, "start_time": QTime}
        self._fade_anims: dict[int, dict] = {}

        # Rows that have completed their fade-in (no animation needed)
        self._faded_in: set[int] = set()

    def start_hover(self, row: int, prev_row: int) -> None:
        """Begin scale-up animation for *row* and scale-down for *prev_row*."""
        now = QTime.currentTime()

        if row >= 0:
            existing = self._hover_anims.get(row)
            current_val = existing["current"] if existing else 1.0
            self._hover_anims[row] = {
                "current": current_val,
                "target": HOVER_SCALE_TARGET,
                "start_val": current_val,
                "start_time": now,
            }

        if prev_row >= 0 and prev_row != row:
            existing = self._hover_anims.get(prev_row)
            current_val = existing["current"] if existing else HOVER_SCALE_TARGET
            self._hover_anims[prev_row] = {
                "current": current_val,
                "target": 1.0,
                "start_val": current_val,
                "start_time": now,
            }

        self._ensure_running()

    def notify_rows_about_to_reset(self) -> None:
        """Clear all fade-in tracking when the model is about to reset."""
        self._fade_anims.clear()
        self._faded_in.clear()

    def notify_rows_inserted(self, first_row: int, last_row: int) -> None:
        """Start fade-in animation for newly inserted rows."""
        now = QTime.currentTime()
        for row in range(first_row, last_row + 1):
            if row not in self._faded_in:
                self._fade_anims[row] = {
                    "current": 0.0,
                    "start_time": now,
                }
        if first_row <= last_row:
            self._ensure_running()

    def get_scale(self, row: int) -> float:
        """Return the current animated scale factor for *row*."""
        anim = self._hover_anims.get(row)
        if anim is not None:
            return anim["current"]
        return 1.0

    def get_opacity(self, row: int) -> float:
        """Return the current animated opacity for *row*."""
        anim = self._fade_anims.get(row)
        if anim is not None:
            return anim["current"]
        return 1.0

    def is_animating(self) -> bool:
        """Return True if any animations are currently active."""
        return bool(self._hover_anims or self._fade_anims)

    def _ensure_running(self) -> None:
        """Start the timer if it isn't already running."""
        if not self._timer.isActive():
            self._timer.start()

    def _tick(self) -> None:
        """Advance all active animations by one frame and trigger a repaint."""
        now = QTime.currentTime()
        has_active = False

        # Advance hover scale animations
        finished_hover: list[int] = []
        for row, anim in self._hover_anims.items():
            elapsed = anim["start_time"].msecsTo(now)
            progress = min(elapsed / HOVER_DURATION_MS, 1.0)
            eased = _ease_out(progress)
            anim["current"] = anim["start_val"] + (anim["target"] - anim["start_val"]) * eased

            if progress >= 1.0:
                anim["current"] = anim["target"]
                if anim["target"] == 1.0:
                    # Fully scaled down — remove from tracking
                    finished_hover.append(row)
                # If target is HOVER_SCALE_TARGET, keep tracking so we can
                # animate back down when hover ends
            else:
                has_active = True

        for row in finished_hover:
            del self._hover_anims[row]

        # Advance fade-in animations
        finished_fade: list[int] = []
        for row, anim in self._fade_anims.items():
            elapsed = anim["start_time"].msecsTo(now)
            progress = min(elapsed / FADE_DURATION_MS, 1.0)
            eased = _ease_out(progress)
            anim["current"] = eased

            if progress >= 1.0:
                anim["current"] = 1.0
                self._faded_in.add(row)
                finished_fade.append(row)
            else:
                has_active = True

        for row in finished_fade:
            del self._fade_anims[row]

        # Trigger repaint of the viewport
        self._view.viewport().update()

        # Stop the timer if nothing is animating
        if not has_active:
            self._timer.stop()

    def shutdown(self) -> None:
        """Stop the timer and clear all state."""
        self._timer.stop()
        self._hover_anims.clear()
        self._fade_anims.clear()
        self._faded_in.clear()


class ThumbnailDelegate(QStyledItemDelegate):
    """Custom delegate that paints thumbnail images with file info overlays.

    Uses a :class:`ThumbnailAnimator` to apply hover-scale and fade-in
    animations during immediate-mode painting.
    """

    def __init__(
        self,
        parent: object | None = None,
        animator: ThumbnailAnimator | None = None,
    ) -> None:
        super().__init__(parent)
        self._animator = animator
        self._hovered_row: int = -1

    def set_animator(self, animator: ThumbnailAnimator) -> None:
        """Attach the animation controller (called after construction)."""
        self._animator = animator

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
        row = index.row()
        painter.save()

        # ── Fade-in opacity ──────────────────────────────────────────
        if self._animator is not None:
            opacity = self._animator.get_opacity(row)
            if opacity < 1.0:
                painter.setOpacity(opacity)

        # ── Hover scale transform ────────────────────────────────────
        if self._animator is not None:
            scale = self._animator.get_scale(row)
            if scale != 1.0:
                cx = option.rect.center().x()
                cy = option.rect.center().y()
                painter.translate(cx, cy)
                painter.scale(scale, scale)
                painter.translate(-cx, -cy)

        # ── Cell background ──────────────────────────────────────────
        is_hovered = row == self._hovered_row
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
        # Try cached 256px thumbnail first (fast for grid scrolling)
        cache_path = index.data(ThumbnailModel.ThumbnailRole256)
        if cache_path:
            pixmap = QPixmap(cache_path)
        else:
            # Fallback: load source file directly (works for JPEG/PNG/WebP, blank for PSD)
            source_path = index.data(ThumbnailModel.PathRole)
            pixmap = QPixmap(source_path) if source_path else QPixmap()

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
                old_model.modelAboutToBeReset.disconnect(  # type: ignore[union-attr]
                    self._animator.notify_rows_about_to_reset
                )
                old_model.rowsInserted.disconnect(  # type: ignore[union-attr]
                    self._on_rows_inserted
                )
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
