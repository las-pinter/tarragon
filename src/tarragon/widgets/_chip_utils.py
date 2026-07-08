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

from tarragon.theme.colors import AMBER_ACCENT, CORAL_MUTED, SURFACE_HOVER
from tarragon.theme.spacing import XS

# Brighter coral for hover feedback — no matching token yet.
_HOVER_CORAL = "#FF6B40"


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
    chip.setStyleSheet(
        f"QFrame {{  background-color: {SURFACE_HOVER.name()};  "
        f"border: 1px solid {AMBER_ACCENT.name()};  "
        f"border-radius: 10px;  padding: 2px 6px;}}"
    )

    chip_layout = QHBoxLayout(chip)
    chip_layout.setContentsMargins(XS, XS, XS, XS)
    chip_layout.setSpacing(XS)

    label = QLabel(label_text)
    label.setStyleSheet(
        f"QLabel {{ color: {AMBER_ACCENT.name()}; border: none; background: transparent; }}"
    )
    if tooltip is not None:
        label.setToolTip(tooltip)
    chip_layout.addWidget(label)

    remove_btn = QPushButton("\u00d7")
    remove_btn.setFixedSize(16, 16)
    remove_btn.setCursor(Qt.CursorShape.PointingHandCursor)
    remove_btn.setStyleSheet(
        f"QPushButton {{"
        f"  color: {CORAL_MUTED.name()};"
        f"  border: none;"
        f"  background: transparent;"
        f"  font-weight: bold;"
        f"  padding: 0;"
        f"}}"
        f"QPushButton:hover {{"
        f"  color: {_HOVER_CORAL};"
        f"}}"
    )
    remove_btn.clicked.connect(lambda _checked=False: on_remove())
    chip_layout.addWidget(remove_btn)

    return chip
