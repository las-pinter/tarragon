"""Inline folder filter widget for the gallery top bar.

Provides an "Add Folder" button that opens a context menu of checkable folder
actions, plus removable chips showing the currently active folder filters.
"""

from __future__ import annotations

import logging
from functools import partial
from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QHBoxLayout,
    QMenu,
    QPushButton,
    QWidget,
)

from tarragon.db.database import Database
from tarragon.theme.constants import SPACING_S
from tarragon.widgets._chip_utils import create_removable_chip
from tarragon.widgets.filter_bar_filter import FilterBarFilter

logger = logging.getLogger(__name__)


class FilterBarFolder(FilterBarFilter):
    """A compact tag filter widget with an Add Folder button and active-folder chips.

    Emits whenever the set of folders changes.
    The payload is a ``set[str]`` of active filter tag IDs.
    """

    def __init__(self, db: Database, parent: QWidget | None = None) -> None:
        """Build the filter bar with an Add Folder button and chips container."""
        super().__init__(parent)
        self._db = db
        self._selected_folders: set[str] = set()
        self._folder_chips: dict[str, QWidget] = {}

        layout = QHBoxLayout(self)
        layout.setContentsMargins(SPACING_S, 0, SPACING_S, 0)
        layout.setSpacing(SPACING_S)

        # Add Folder button
        self._add_folder_btn = QPushButton("Add Folder")
        self._add_folder_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._add_folder_btn.setToolTip("Filter by folder")
        self._add_folder_btn.clicked.connect(self._show_folder_menu)
        layout.addWidget(self._add_folder_btn)

        # Container for active folder chips
        self._chips_container = QWidget()
        self._chips_layout = QHBoxLayout(self._chips_container)
        self._chips_layout.setContentsMargins(0, 0, 0, 0)
        self._chips_layout.setSpacing(SPACING_S)
        layout.addWidget(self._chips_container)

        layout.addStretch()

        # Folder menu
        self._folder_menu = QMenu(self)

        # Initial state: folder widgets hidden
        self._add_folder_btn.hide()
        self._chips_container.hide()

    def _show_folder_menu(self) -> None:
        """Show a menu of available folders not yet selected."""
        self._folder_menu.clear()

        try:
            all_folders = self._db.list_distinct_folders()

            logger.debug("Available favorite folders: %s", all_folders)
            for folder_path in all_folders:
                display_name = self._short_folder_name(folder_path)
                action = self._folder_menu.addAction(display_name)
                action.setCheckable(True)
                checked = folder_path in self._selected_folders
                action.setData(display_name)
                action.triggered.connect(lambda checked=checked, fp=folder_path: self._toggle_folder(fp))

            # Show menu below the button
            pos = self._add_folder_btn.mapToGlobal(self._add_folder_btn.rect().bottomLeft())
            self._folder_menu.popup(pos)

        except Exception:
            logger.debug("Failed to load folder list", exc_info=True)
            return

    def _toggle_folder(self, folder_path: str) -> None:
        """Add a folder as a selected chip and emit the updated set.

        Args:
            folder_path: Full folder path to add.
        """
        if folder_path in self._selected_folders:
            return  # Already selected

        self._selected_folders.add(folder_path)
        chip = self._create_folder_chip(folder_path)
        self._folder_chips[folder_path] = chip
        self._chips_layout.addWidget(chip)
        self._update_folder_chips_visibility()
        self._emit_signal(set(self._selected_folders))

    def _remove_folder(self, folder_path: str) -> None:
        """Remove a folder chip and emit the updated set.

        Args:
            folder_path: Full folder path to remove.
        """
        self._selected_folders.discard(folder_path)
        chip = self._folder_chips.pop(folder_path, None)
        if chip is not None:
            self._chips_layout.removeWidget(chip)
            chip.deleteLater()
        self._update_folder_chips_visibility()
        self._emit_signal(set(self._selected_folders))

    def _create_folder_chip(self, folder_path: str) -> QWidget:
        """Create a removable folder chip widget.

        Delegates to the shared chip factory for consistent styling.

        Args:
            folder_path: Full folder path for the chip.

        Returns:
            A QWidget containing the folder name label and a remove button.
        """
        display_name = self._short_folder_name(folder_path)
        return create_removable_chip(
            label_text=display_name,
            on_remove=partial(self._remove_folder, folder_path),
            tooltip=folder_path,
        )

    def _update_folder_chips_visibility(self) -> None:
        """Show or hide the folder chips container based on selection."""
        has_chips = len(self._selected_folders) > 0
        self._chips_container.setVisible(has_chips)

    def _prune_stale_chips(self) -> None:
        """Remove chips for folders that no longer exist in the database."""
        try:
            current_folders = set(self._db.list_distinct_folders())
        except Exception:
            logger.debug("Failed to load folder list for pruning", exc_info=True)
            return

        stale = self._selected_folders - current_folders
        for folder_path in stale:
            self._remove_folder(folder_path)

    def _short_folder_name(self, folder_path: str) -> str:
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

    def set_scope(self, is_global: bool) -> None:
        """Show or hide the folder widgets based on gallery scope.

        Args:
            is_global: ``True`` when "All Images" tab is active.
        """
        self._add_folder_btn.setVisible(is_global)
        # Folder chips remain visible if folders are selected, regardless of scope
        if is_global:
            self._update_folder_chips_visibility()

    def refresh_folders(self) -> None:
        """Refresh the folder menu"""
        self._prune_stale_chips()
