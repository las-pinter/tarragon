"""Horizontal scrollable row of clickable hue swatches."""

from __future__ import annotations

from typing import Any

from PySide6.QtCore import Signal
from PySide6.QtWidgets import QWidget


class FilterBarFilter(QWidget):
    """An abstract class for filters in the filter bar."""

    filter_changed = Signal(set)

    def __init__(self, parent: QWidget | None = None) -> None:
        """Create the filter"""
        super().__init__(parent)

    def _emit_signal(self, data: set[Any]) -> None:
        """Emiting signal."""
        self.filter_changed.emit(data)
