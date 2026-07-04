"""FilterBar — combined filter row for the gallery view.

Hosts the ``ColorFilterBar`` (colour swatches), ``TagFilterBar`` (Add Tag+
and chips), and a ``QComboBox`` for folder filtering (visible only in
"All Images" global mode).  All three sub-widgets share a single
horizontal row, making the filter UI compact and responsive to width.

Signals are forwarded from the child widgets so that the MainWindow can
connect to a single widget instead of three separate ones.
"""

from __future__ import annotations

import logging
from pathlib import Path

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QComboBox, QHBoxLayout, QWidget

from tarragon.db import Database
from tarragon.services.tag_service import TagService
from tarragon.theme.spacing import XS
from tarragon.widgets.color_filter_bar import ColorFilterBar
from tarragon.widgets.tag_filter_bar import TagFilterBar

logger = logging.getLogger(__name__)


class FilterBar(QWidget):
    """Combined filter bar containing colour, tag, and folder filters.

    Signals:
        color_filter_changed: Forwarded from ``ColorFilterBar``.
        tag_filter_changed: Forwarded from ``TagFilterBar``.
        folder_filter_changed: Emitted when the folder combo selection
            changes.  Payload is the folder path string (empty for "All
            Folders").
    """

    color_filter_changed = Signal(set)  # set of "color:<bucket>" strings
    tag_filter_changed = Signal(set)  # set of int — active filter tag IDs
    folder_filter_changed = Signal(str)  # folder path or ""

    def __init__(
        self,
        tag_service: TagService,
        db: Database,
        parent: QWidget | None = None,
    ) -> None:
        """Build the combined filter bar.

        Args:
            tag_service: TagService instance for the tag filter bar.
            db: Database instance for populating the folder dropdown.
            parent: Optional parent widget.
        """
        super().__init__(parent)
        self._db = db

        # ── Sub-widgets ──────────────────────────────────────────────
        self._color_filter_bar = ColorFilterBar()
        self._tag_filter_bar = TagFilterBar(tag_service)

        # Folder filter combo box (only visible in global mode)
        self._folder_combo = QComboBox()
        self._folder_combo.addItem("All Folders", "")
        self._folder_combo.setToolTip("Filter by folder (All Images mode only)")
        self._refresh_folder_list()
        self._folder_combo.currentIndexChanged.connect(self._on_folder_combo_changed)

        # ── Layout ───────────────────────────────────────────────────
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(XS)

        layout.addWidget(self._color_filter_bar)
        layout.addWidget(self._tag_filter_bar)
        layout.addWidget(self._folder_combo)
        layout.addStretch()

        # ── Forward child signals ────────────────────────────────────
        self._color_filter_bar.color_filter_changed.connect(self.color_filter_changed.emit)
        self._tag_filter_bar.tag_filter_changed.connect(self.tag_filter_changed.emit)

        # ── Initial state: folder combo hidden (local/folder mode) ───
        self._folder_combo.hide()

    # ── Public API ───────────────────────────────────────────────────

    @property
    def color_filter_bar(self) -> ColorFilterBar:
        """Access the embedded ``ColorFilterBar`` widget."""
        return self._color_filter_bar

    @property
    def tag_filter_bar(self) -> TagFilterBar:
        """Access the embedded ``TagFilterBar`` widget."""
        return self._tag_filter_bar

    def set_scope(self, is_global: bool) -> None:  # noqa: FBT001
        """Show or hide the folder dropdown based on gallery scope.

        Args:
            is_global: ``True`` when "All Images" tab is active.
        """
        self._folder_combo.setVisible(is_global)
        if is_global:
            self._refresh_folder_list()

    def refresh_folders(self) -> None:
        """Re-populate the folder dropdown from the database."""
        self._refresh_folder_list()

    # ── Internal helpers ─────────────────────────────────────────────

    def _on_folder_combo_changed(self, index: int) -> None:
        """Emit ``folder_filter_changed`` when the combo selection changes."""
        folder = self._folder_combo.itemData(index)
        self.folder_filter_changed.emit(folder if isinstance(folder, str) else "")

    def _refresh_folder_list(self) -> None:
        """Rebuild the folder combo items from ``db.list_distinct_folders()``.

        Signals are blocked during the rebuild to prevent spurious
        ``folder_filter_changed`` emissions, and the previously selected
        folder is restored if it still exists in the refreshed list.
        """
        # Remember current selection before clearing
        current_folder = self._folder_combo.currentData()

        self._folder_combo.blockSignals(True)
        try:
            # Preserve the first item ("All Folders")
            while self._folder_combo.count() > 1:
                self._folder_combo.removeItem(1)

            try:
                folders = self._db.list_distinct_folders()
            except Exception:
                logger.debug("Failed to load folder list", exc_info=True)
                return

            for folder_path in folders:
                # Display only the last two path components for readability
                display_name = _short_folder_name(folder_path)
                self._folder_combo.addItem(display_name, folder_path)
                # Add tooltip with full path for ambiguous names
                self._folder_combo.setItemData(
                    self._folder_combo.count() - 1,
                    folder_path,
                    Qt.ItemDataRole.ToolTipRole,
                )

            # Restore previous selection if still available
            if current_folder:
                idx = self._folder_combo.findData(current_folder)
                if idx >= 0:
                    self._folder_combo.setCurrentIndex(idx)
        finally:
            self._folder_combo.blockSignals(False)


def _short_folder_name(folder_path: str) -> str:
    """Create a short display name from a folder path.

    Shows the last two path components for readability.  For example,
    ``/home/user/photos/vacation`` becomes ``photos/vacation``.

    Args:
        folder_path: Full folder path.

    Returns:
        Shortened display string.
    """
    parts = Path(folder_path).parts
    if len(parts) <= 2:
        return folder_path
    return str(Path(*parts[-2:]))
