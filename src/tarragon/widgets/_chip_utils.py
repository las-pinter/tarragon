"""Shared chip widget factory for removable filter chips."""

from __future__ import annotations

from collections.abc import Callable

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QWidget,
)

from tarragon.theme.constants import SPACING_XS


def create_chip_close_button(text: str, object_name: str) -> QPushButton:
    """Create a small close button used by removable chips and tag pills.

    Args:
        text: Glyph displayed on the button (e.g. ``"x"`` or ``"\u00d7"``).
        object_name: Qt object name for QSS targeting.

    Returns:
        A configured ``QPushButton`` with a fixed 16x16 size and a
        pointing-hand cursor.
    """
    btn = QPushButton(text)
    btn.setFixedSize(16, 16)
    btn.setCursor(Qt.CursorShape.PointingHandCursor)
    btn.setObjectName(object_name)
    return btn


def create_removable_chip(
    label_text: str,
    on_remove: Callable[[], None],
    tooltip: str | None = None,
) -> QWidget:
    """Create a removable amber-styled chip widget.

    The chip is a ``QFrame`` with a text label and a close button.
    Clicking the close button invokes *on_remove*.

    Parameters
    ----------
    label_text:
        Display text for the chip label.
    on_remove:
        Callback invoked when the button is clicked.
    tooltip:
        Optional tooltip shown on the label (e.g. a full folder path).

    Returns
    -------
        A ``QWidget`` (``QFrame``) containing the label and remove button.
    """
    chip = QFrame()
    chip.setObjectName("filterChip")

    chip_layout = QHBoxLayout(chip)
    chip_layout.setContentsMargins(SPACING_XS, SPACING_XS, SPACING_XS, SPACING_XS)
    chip_layout.setSpacing(SPACING_XS)

    label = QLabel(label_text)
    label.setObjectName("filterChipLabel")
    if tooltip is not None:
        label.setToolTip(tooltip)
    chip_layout.addWidget(label)

    remove_btn = create_chip_close_button("\u00d7", "filterChipRemoveBtn")
    remove_btn.clicked.connect(lambda: on_remove())
    chip_layout.addWidget(remove_btn)

    return chip
