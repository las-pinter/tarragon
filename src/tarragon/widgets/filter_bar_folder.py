"""Inline folder filter widget for the gallery top bar.

Provides an "Add Folder" button that opens a context menu of checkable folder
actions, plus removable chips showing the currently active folder filters.
"""

from __future__ import annotations

import logging
from functools import partial
from pathlib import Path

from PySide6.QtWidgets import QWidget

from tarragon.db.database import Database
from tarragon.widgets._chip_utils import create_removable_chip
from tarragon.widgets.filter_bar_chip_base import _ChipFilterBarBase

logger = logging.getLogger(__name__)


class FilterBarFolder(_ChipFilterBarBase[str]):
    """A compact folder filter widget with an Add Folder button and active-folder chips.

    Emits whenever the set of folders changes.
    The payload is a ``set[str]`` of active folder paths.
    """

    def __init__(self, db: Database, parent: QWidget | None = None) -> None:
        """Build the filter bar with an Add Folder button and chips container."""
        super().__init__(
            "Filter Folders",
            button_tooltip="Filter by folder",
            initially_hidden=True,
            parent=parent,
        )
        self._db = db
        # Alias the base's state to the names pinned by tests / used by the gallery.
        self._selected_folders = self._active
        self._folder_chips = self._chips
        self._add_folder_btn = self._add_button
        self._folder_menu = self._menu

    def refresh_folders(self) -> None:
        """Refresh the folder menu"""
        self._prune_stale_chips()

    def _load_items(self) -> list[str] | None:
        """Load available folders, or ``None`` when the database query fails."""
        try:
            all_folders = self._db.list_distinct_folders()
            logger.debug("Available favorite folders: %s", all_folders)
            return all_folders
        except Exception:
            logger.debug("Failed to load folder list", exc_info=True)
            return None

    def _item_label(self, item: str) -> str:
        """Return the short display name for the menu."""
        return self._short_folder_name(item)

    def _item_data(self, item: str) -> object:
        """Store the short display name on the action (legacy behavior)."""
        return self._short_folder_name(item)

    def _create_chip(self, folder_path: str) -> QWidget:
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

    def _create_folder_chip(self, folder_path: str) -> QWidget:
        """Continuity alias for ``_create_chip``."""
        return self._create_chip(folder_path)

    def _toggle_folder(self, folder_path: str) -> None:
        """Toggle a folder in the active folders set.

        Args:
            folder_path: Full folder path to toggle.
        """
        self._toggle_item(folder_path)

    def _remove_folder(self, folder_path: str) -> None:
        """Remove a folder chip and emit the updated set.

        Args:
            folder_path: Full folder path to remove.
        """
        self._remove_item(folder_path)

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
            self._update_chips()
