"""FlowLayout — a wrapping layout for Qt widgets.

Arranges child items left-to-right, wrapping to the next line when the
available width is exceeded.  Similar to how text flows in a paragraph.

This is a Python/PySide6 implementation of the well-known Qt FlowLayout
example, adapted for use in the tarragon gallery filter bar.
"""

from __future__ import annotations

from PySide6.QtCore import QMargins, QPoint, QRect, QSize, Qt
from PySide6.QtWidgets import QLayout, QLayoutItem, QWidget


class FlowLayout(QLayout):
    """A layout that wraps its items to the next line when width is exceeded.

    Items are placed left-to-right.  When an item would extend beyond the
    right edge of the available rectangle, it wraps to the beginning of the
    next line.

    Args:
        parent: Optional parent layout.
        margin: Contents margin in pixels.
        spacing: Spacing between items in pixels.  Defaults to 6 when
            not explicitly provided (or when a negative value is passed).
    """

    def __init__(
        self,
        parent: QWidget | None = None,
        margin: int = 0,
        spacing: int = -1,
    ) -> None:
        super().__init__(parent)
        self._item_list: list[QLayoutItem] = []
        self._margin = margin
        self._spacing = spacing if spacing >= 0 else 6

    # ── QLayout interface ───────────────────────────────────────────────

    def addItem(self, item: QLayoutItem) -> None:  # noqa: N802
        """Append *item* to the layout."""
        self._item_list.append(item)

    def count(self) -> int:
        """Return the number of items in the layout."""
        return len(self._item_list)

    def itemAt(self, index: int) -> QLayoutItem | None:  # noqa: N802
        """Return the item at *index*, or ``None`` if out of range."""
        if 0 <= index < len(self._item_list):
            return self._item_list[index]
        return None

    def takeAt(self, index: int) -> QLayoutItem | None:  # noqa: N802
        """Remove and return the item at *index*, or ``None`` if out of range."""
        if 0 <= index < len(self._item_list):
            return self._item_list.pop(index)
        return None

    def expandingDirections(self) -> Qt.Orientation:  # noqa: N802
        """FlowLayout does not expand in either direction."""
        return Qt.Orientation(0)

    def hasHeightForWidth(self) -> bool:  # noqa: N802
        """Yes — the height depends on the available width."""
        return True

    def heightForWidth(self, width: int) -> int:  # noqa: N802
        """Calculate the height needed for the given *width*."""
        return self._do_layout(QRect(0, 0, width, 0), test_only=True)

    def setGeometry(self, rect: QRect) -> None:  # noqa: N802
        """Position all items within *rect*."""
        super().setGeometry(rect)
        self._do_layout(rect, test_only=False)

    def sizeHint(self) -> QSize:  # noqa: N802
        """Return the preferred size of the layout."""
        return self.minimumSize()

    def minimumSize(self) -> QSize:  # noqa: N802
        """Return the minimum size — the largest single item plus margins."""
        size = QSize()
        for item in self._item_list:
            size = size.expandedTo(item.minimumSize())
        m = self._margin
        size += QSize(2 * m, 2 * m)
        return size

    # ── Internal ────────────────────────────────────────────────────────

    def _do_layout(self, rect: QRect, *, test_only: bool) -> int:
        """Arrange items within *rect*.

        Items are placed left-to-right and **vertically centered** within
        each line so that widgets of different heights align neatly.

        When *test_only* is ``True``, no geometry is applied — the method
        merely calculates and returns the total height required.

        Returns:
            The total height consumed by all lines of items.
        """
        effective_rect = rect.marginsRemoved(QMargins(self._margin, self._margin, self._margin, self._margin))

        x = effective_rect.x()
        y = effective_rect.y()
        line_height = 0

        # Two-pass approach: collect items per line, then position them
        # with vertical centering.  This avoids the misalignment that
        # occurs when items of different heights share a top-aligned line.
        line_items: list[tuple[QLayoutItem, QSize]] = []

        for item in self._item_list:
            item_size = item.sizeHint()
            next_x = x + item_size.width() + self._spacing

            # Wrap to next line if this item would overflow
            if next_x - self._spacing > effective_rect.right() and line_height > 0:
                # Position the completed line
                if not test_only:
                    self._position_line(line_items, effective_rect.x(), y, line_height)
                x = effective_rect.x()
                y = y + line_height + self._spacing
                next_x = x + item_size.width() + self._spacing
                line_height = 0
                line_items = []

            x = next_x
            line_height = max(line_height, item_size.height())
            line_items.append((item, item_size))

        # Position the final line
        if not test_only and line_items:
            self._position_line(line_items, effective_rect.x(), y, line_height)

        return y + line_height - rect.y() + self._margin

    @staticmethod
    def _position_line(
        items: list[tuple[QLayoutItem, QSize]],
        line_x: int,
        line_y: int,
        line_height: int,
    ) -> None:
        """Place all *items* on a single line, vertically centered.

        Args:
            items: Layout items and their pre-computed sizes for this line.
            line_x: The starting x-coordinate for the line.
            line_y: The starting y-coordinate for the line.
            line_height: The maximum item height on this line.
        """
        current_x = line_x
        for item, item_size in items:
            # Vertically center the item within the line height
            offset_y = (line_height - item_size.height()) // 2
            item.setGeometry(QRect(QPoint(current_x, line_y + offset_y), item_size))
            current_x += item_size.width()
