"""Tests for FlowLayout — wrapping layout widget.

Covers:
    - Single-line placement when width is sufficient
    - Wrapping to next line when width is narrow
    - Height-for-width calculation
    - Minimum size computation
    - count() / itemAt() / takeAt() item tracking
    - Empty layout behaviour
    - Spacing configuration
    - Multiple-line wrapping
"""

from __future__ import annotations

from PySide6.QtCore import QRect
from PySide6.QtWidgets import QLabel, QWidget

from tarragon.widgets.flow_layout import FlowLayout


# =========================================================================
# TestFlowLayout
# =========================================================================


class TestFlowLayout:
    """FlowLayout arranges items left-to-right and wraps when width is exceeded."""

    def test_single_line_when_width_sufficient(self) -> None:
        """All items stay on one line when there is enough width."""
        container = QWidget()
        layout = FlowLayout(container, margin=0, spacing=6)

        # Three 100px-wide labels — need 100+6+100+6+100 = 312px on one line
        labels = [QLabel(f"Item {i}") for i in range(3)]
        for lbl in labels:
            lbl.setFixedSize(100, 30)
            layout.addWidget(lbl)

        # Give plenty of width
        layout.setGeometry(QRect(0, 0, 500, 400))

        # All items should be on the same y-coordinate
        y_positions = {lbl.y() for lbl in labels}
        assert len(y_positions) == 1, "All items should be on the same line"
        container.close()

    def test_wraps_when_width_narrow(self) -> None:
        """Items wrap to the next line when width is too narrow."""
        container = QWidget()
        layout = FlowLayout(container, margin=0, spacing=6)

        # Three 100px-wide labels
        labels = [QLabel(f"Item {i}") for i in range(3)]
        for lbl in labels:
            lbl.setFixedSize(100, 30)
            layout.addWidget(lbl)

        # Only enough width for ~2 items: 100+6+100 = 206, but not 3rd (206+6+100=312)
        layout.setGeometry(QRect(0, 0, 210, 400))

        # Third item should be on a different (lower) y-coordinate
        y_positions = [lbl.y() for lbl in labels]
        assert y_positions[0] == y_positions[1], "First two items on same line"
        assert y_positions[2] > y_positions[0], "Third item wrapped to next line"
        container.close()

    def test_height_for_width(self) -> None:
        """heightForWidth returns more height when width is narrower."""
        container = QWidget()
        layout = FlowLayout(container, margin=0, spacing=6)

        labels = [QLabel(f"Item {i}") for i in range(4)]
        for lbl in labels:
            lbl.setFixedSize(100, 30)
            layout.addWidget(lbl)

        # Wide: all 4 on one line → height ≈ 30
        wide_height = layout.heightForWidth(500)
        # Narrow: forces wrapping → height > 30
        narrow_height = layout.heightForWidth(150)

        assert narrow_height > wide_height
        container.close()

    def test_minimum_size(self) -> None:
        """minimumSize returns the largest single item plus margins."""
        container = QWidget()
        layout = FlowLayout(container, margin=4, spacing=6)

        lbl_small = QLabel("S")
        lbl_small.setFixedSize(50, 20)
        layout.addWidget(lbl_small)

        lbl_large = QLabel("L")
        lbl_large.setFixedSize(120, 40)
        layout.addWidget(lbl_large)

        min_size = layout.minimumSize()
        # Should be at least as large as the biggest item + 2*margin
        assert min_size.width() >= 120 + 2 * 4
        assert min_size.height() >= 40 + 2 * 4
        container.close()

    def test_count_and_item_at(self) -> None:
        """count() and itemAt() correctly track added items."""
        container = QWidget()
        layout = FlowLayout(container)

        assert layout.count() == 0
        assert layout.itemAt(0) is None

        lbl = QLabel("test")
        lbl.setFixedSize(50, 20)
        layout.addWidget(lbl)

        assert layout.count() == 1
        assert layout.itemAt(0) is not None
        assert layout.itemAt(1) is None
        container.close()

    def test_take_at_removes_item(self) -> None:
        """takeAt() removes and returns the item at the given index."""
        container = QWidget()
        layout = FlowLayout(container)

        lbl = QLabel("test")
        lbl.setFixedSize(50, 20)
        layout.addWidget(lbl)
        assert layout.count() == 1

        removed = layout.takeAt(0)
        assert removed is not None
        assert layout.count() == 0
        container.close()

    def test_take_at_out_of_range(self) -> None:
        """takeAt() returns None for out-of-range indices."""
        container = QWidget()
        layout = FlowLayout(container)

        assert layout.takeAt(0) is None
        assert layout.takeAt(-1) is None
        assert layout.takeAt(100) is None
        container.close()

    def test_empty_layout_height(self) -> None:
        """heightForWidth on an empty layout returns a sensible value."""
        container = QWidget()
        layout = FlowLayout(container, margin=4, spacing=6)

        height = layout.heightForWidth(200)
        # Should be at least 2*margin (top + bottom)
        assert height >= 2 * 4
        container.close()

    def test_default_spacing(self) -> None:
        """Default spacing is 6 when not explicitly provided."""
        container = QWidget()
        layout = FlowLayout(container)

        assert layout._spacing == 6
        container.close()

    def test_explicit_spacing(self) -> None:
        """Explicit spacing value is respected."""
        container = QWidget()
        layout = FlowLayout(container, spacing=12)

        assert layout._spacing == 12
        container.close()

    def test_has_height_for_width(self) -> None:
        """hasHeightForWidth returns True."""
        container = QWidget()
        layout = FlowLayout(container)

        assert layout.hasHeightForWidth() is True
        container.close()

    def test_multiple_lines_wrapping(self) -> None:
        """Items wrap across multiple lines when width is very narrow."""
        container = QWidget()
        layout = FlowLayout(container, margin=0, spacing=6)

        # Six 100px items — only room for 1 per line at width=100
        labels = [QLabel(f"Item {i}") for i in range(6)]
        for lbl in labels:
            lbl.setFixedSize(100, 30)
            layout.addWidget(lbl)

        # Width exactly fits one item
        layout.setGeometry(QRect(0, 0, 100, 600))

        y_positions = sorted({lbl.y() for lbl in labels})
        # Should have 6 separate lines
        assert len(y_positions) == 6
        container.close()
