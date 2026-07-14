"""Shared chip widget factory for removable filter chips.

Both ``FilterBar`` (folder chips) and ``TagFilterBar`` (tag chips) need
identical amber-styled removable chips.  This module provides a single
factory function to avoid duplicating the styling and layout code.
"""

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

from tarragon.theme.constants import XS


def create_removable_chip(
    label_text: str,
    on_remove: Callable[[], None],
    tooltip: str | None = None,
) -> QWidget:
    """Create a removable amber-styled chip widget.

    The chip is a ``QFrame`` with a text label and a "×" close button.
    Clicking the close button invokes *on_remove*.

    Parameters
    ----------
    label_text:
        Display text for the chip label.
    on_remove:
        Callback invoked when the "×" button is clicked.
    tooltip:
        Optional tooltip shown on the label (e.g. a full folder path).

    Returns
    -------
        A ``QWidget`` (``QFrame``) containing the label and remove button.
    """
    chip = QFrame()
    chip.setObjectName("filterChip")

    chip_layout = QHBoxLayout(chip)
    chip_layout.setContentsMargins(XS, XS, XS, XS)
    chip_layout.setSpacing(XS)

    label = QLabel(label_text)
    label.setObjectName("filterChipLabel")
    if tooltip is not None:
        label.setToolTip(tooltip)
    chip_layout.addWidget(label)

    remove_btn = QPushButton("\u00d7")
    remove_btn.setFixedSize(16, 16)
    remove_btn.setCursor(Qt.CursorShape.PointingHandCursor)
    remove_btn.setObjectName("filterChipRemoveBtn")
    remove_btn.clicked.connect(lambda _checked=False: on_remove())
    chip_layout.addWidget(remove_btn)

    return chip
