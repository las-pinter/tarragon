"""Gallery scope tabs — Folder vs All Images."""

from __future__ import annotations

from PySide6.QtCore import Signal
from PySide6.QtWidgets import QTabWidget, QWidget


class GalleryTabs(QTabWidget):
    """Tab widget for switching between folder-scoped and global image view."""

    scope_changed = Signal(bool)  # True = global (All Images), False = folder

    def __init__(self, parent: QWidget | None = None) -> None:
        """Build the tab widget with Folder and All Images tabs.

        Args:
            parent: Optional parent widget.
        """
        super().__init__(parent)
        self.addTab(QWidget(), "Folder")
        self.addTab(QWidget(), "All Images")
        self.currentChanged.connect(self._on_tab_changed)

    def _on_tab_changed(self, index: int) -> None:
        """Emit scope_changed signal when tab changes.

        Args:
            index: The newly selected tab index.
        """
        self.scope_changed.emit(index == 1)  # "All Images" is index 1

    def is_global_scope(self) -> bool:
        """Return True if 'All Images' tab is active."""
        return self.currentIndex() == 1
