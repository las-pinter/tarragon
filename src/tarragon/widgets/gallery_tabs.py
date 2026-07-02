"""Gallery scope tabs — Folder vs All Images."""

from __future__ import annotations

from PySide6.QtCore import Signal
from PySide6.QtWidgets import QTabWidget, QVBoxLayout, QWidget


class GalleryTabs(QWidget):
    """Tab widget for switching between folder-scoped and global image view."""

    scope_changed = Signal(bool)  # True = global (All Images), False = folder

    def __init__(self, parent: QWidget | None = None) -> None:
        """Build the tab widget with Folder and All Images tabs.

        Args:
            parent: Optional parent widget.
        """
        super().__init__(parent)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        self._tab_widget = QTabWidget()
        self._tab_widget.addTab(QWidget(), "Folder")
        self._tab_widget.addTab(QWidget(), "All Images")
        self._tab_widget.currentChanged.connect(self._on_tab_changed)

        layout.addWidget(self._tab_widget)

    def _on_tab_changed(self, index: int) -> None:
        """Emit scope_changed signal when tab changes.

        Args:
            index: The newly selected tab index.
        """
        is_global = index == 1  # "All Images" is index 1
        self.scope_changed.emit(is_global)

    def is_global_scope(self) -> bool:
        """Return True if 'All Images' tab is active."""
        return self._tab_widget.currentIndex() == 1
