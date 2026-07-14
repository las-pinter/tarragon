"""Custom Qt delegate for painting thumbnail cells."""

from __future__ import annotations

from PySide6.QtCore import (
    QEvent,
    QModelIndex,
    QObject,
    QPersistentModelIndex,
    QRect,
    QSize,
    Qt,
)
from PySide6.QtGui import (
    QHelpEvent,
    QPainter,
    QPainterPath,
    QPen,
    QPixmap,
)
from PySide6.QtWidgets import (
    QAbstractItemView,
    QStyle,
    QStyledItemDelegate,
    QStyleOptionViewItem,
    QToolTip,
)

from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.theme.colors import (
    BG_PRIMARY,
    BG_SECONDARY,
    BORDER_SUBTLE,
    CORAL_MUTED,
    TEXT_MUTED,
)
from tarragon.theme.file_type_badge import get_badge_colors
from tarragon.theme.constants import GRID_GAP, THUMBNAIL_SIZE
from tarragon.theme.typography import SMALL_SIZE
from tarragon.widgets.thumbnail_animator import ThumbnailAnimator

# ── File extension badge layout constants ─────────────────────────────────────
BADGE_MARGIN = 4  # offset from top-left corner of thumbnail image
BADGE_PAD_X = 5  # horizontal padding inside badge
BADGE_PAD_Y = 1  # vertical padding inside badge
BADGE_RADIUS = 4  # border-radius for badge rounded rect
BADGE_FONT_SIZE = 9  # badge font size in points

# ── Text area height in grid cell sizeHint ────────────────────────────────────
TEXT_AREA_HEIGHT = 24

# ── Thumbnail border-radius (from mockup spec) ────────────────────────────────
THUMBNAIL_RADIUS = 8

# Extra pixels per cell to accommodate hover-scale growth without overlapping neighbors
HOVER_MARGIN = 4


class ThumbnailDelegate(QStyledItemDelegate):
    """Custom delegate that paints thumbnail images with file info overlays.

    Uses a :class:`ThumbnailAnimator` to apply hover-scale and fade-in
    animations during immediate-mode painting.
    """

    def __init__(
        self,
        parent: QObject | None = None,
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
        index: QModelIndex | QPersistentModelIndex,
    ) -> None:
        """Paint the thumbnail cell: background, image, name, extension badge, selection border."""
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

        # ── Cell background (inset by HOVER_MARGIN for visual gap) ───
        is_hovered = row == self._hovered_row
        is_selected = bool(option.state & QStyle.StateFlag.State_Selected)

        container_rect = option.rect.adjusted(HOVER_MARGIN, HOVER_MARGIN, -HOVER_MARGIN, -HOVER_MARGIN)

        # Build rounded-rect path for clipping (8px border-radius)
        clip_path = QPainterPath()
        clip_path.addRoundedRect(
            float(container_rect.x()),
            float(container_rect.y()),
            float(container_rect.width()),
            float(container_rect.height()),
            float(THUMBNAIL_RADIUS),
            float(THUMBNAIL_RADIUS),
        )

        # Clip to rounded rect so background + image respect border-radius
        painter.setClipPath(clip_path)

        if is_selected:
            painter.fillRect(container_rect, BG_SECONDARY)
        elif is_hovered:
            painter.fillRect(container_rect, BG_SECONDARY.lighter(110))
        else:
            painter.fillRect(container_rect, BG_PRIMARY)

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

        # Reset clipping so borders and text aren't clipped to rounded rect
        painter.setClipping(False)

        # ── File basename text ───────────────────────────────────────
        name = index.data(Qt.ItemDataRole.DisplayRole) or ""
        painter.setPen(TEXT_MUTED)
        font = painter.font()
        font.setPointSize(SMALL_SIZE)
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

        # ── File extension badge (top-left corner) ───────────────────
        path_str = index.data(ThumbnailModel.PathRole) or ""
        if path_str:
            ext = path_str.rsplit(".", 1)[-1] if "." in path_str else ""
            if ext:
                bg_color, text_color = get_badge_colors(ext)
                badge_text = ext.upper()

                # Set up badge font: 9px bold
                badge_font = painter.font()
                badge_font.setPointSize(BADGE_FONT_SIZE)
                badge_font.setBold(True)
                painter.setFont(badge_font)

                # Measure text and compute badge rect with padding
                fm = painter.fontMetrics()
                text_width = fm.horizontalAdvance(badge_text)
                text_height = fm.height()

                badge_w = text_width + BADGE_PAD_X * 2
                badge_h = text_height + BADGE_PAD_Y * 2

                # Position: top-LEFT corner of image_rect
                badge_x = image_rect.left() + BADGE_MARGIN
                badge_y = image_rect.top() + BADGE_MARGIN
                badge_rect = QRect(badge_x, badge_y, badge_w, badge_h)

                # Draw rounded background
                painter.setPen(Qt.PenStyle.NoPen)
                painter.setBrush(bg_color)
                painter.drawRoundedRect(
                    badge_rect,
                    float(BADGE_RADIUS),
                    float(BADGE_RADIUS),
                )

                # Draw extension text centered in badge
                painter.setPen(text_color)
                painter.drawText(
                    badge_rect,
                    Qt.AlignmentFlag.AlignCenter,
                    badge_text,
                )

        # ── Borders (rounded, 8px radius) ────────────────────────────
        border_rect = container_rect.adjusted(1, 1, -1, -1)
        if is_selected:
            # Selection border: 1.5px CORAL_MUTED
            pen = QPen(CORAL_MUTED, 1.5)
            painter.setPen(pen)
            painter.setBrush(Qt.BrushStyle.NoBrush)
            painter.drawRoundedRect(border_rect, THUMBNAIL_RADIUS, THUMBNAIL_RADIUS)
        else:
            # Unselected border: 1px BORDER_SUBTLE
            pen = QPen(BORDER_SUBTLE, 1.0)
            painter.setPen(pen)
            painter.setBrush(Qt.BrushStyle.NoBrush)
            painter.drawRoundedRect(container_rect, THUMBNAIL_RADIUS, THUMBNAIL_RADIUS)

        painter.restore()

    def sizeHint(  # noqa: N802
        self,
        option: QStyleOptionViewItem,
        index: QModelIndex | QPersistentModelIndex,
    ) -> QSize:
        """Return the size of each grid cell.

        Includes HOVER_MARGIN so scaled-up hover painting stays within cell bounds.
        """
        return QSize(
            THUMBNAIL_SIZE + GRID_GAP * 2 + HOVER_MARGIN * 2,
            THUMBNAIL_SIZE + GRID_GAP * 2 + TEXT_AREA_HEIGHT + HOVER_MARGIN * 2,
        )

    def helpEvent(  # noqa: N802
        self,
        event: QHelpEvent,
        view: QAbstractItemView,
        option: QStyleOptionViewItem,
        index: QModelIndex | QPersistentModelIndex,
    ) -> bool:
        """Show tooltip with full filename on hover."""
        if event.type() == QEvent.Type.ToolTip:
            name = index.data(Qt.ItemDataRole.DisplayRole) or ""
            if name:
                QToolTip.showText(event.globalPos(), name, view)
            return True
        return super().helpEvent(event, view, option, index)
