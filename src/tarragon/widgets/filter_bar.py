"""FilterBar — combined filter row for the gallery view.

Hosts the ``ColorFilterBar`` (colour swatches), ``TagFilterBar`` (Add Tag+
and chips), and a folder multi-select widget (Add Folder+ button and
removable chips, visible only in "All Images" global mode).  All
sub-widgets share a single horizontal row, making the filter UI compact
and responsive to width.

Signals are forwarded from the child widgets so that the MainWindow can
connect to a single widget instead of three separate ones.
"""

from __future__ import annotations

import logging
from pathlib import Path

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QHBoxLayout,
    QMenu,
    QPushButton,
    QWidget,
)

from tarragon.db import Database
from tarragon.services.tag_service import TagService
from tarragon.theme.spacing import XS
from tarragon.widgets._chip_utils import create_removable_chip
from tarragon.widgets.color_filter_bar import ColorFilterBar
from tarragon.widgets.flow_layout import FlowLayout
from tarragon.widgets.tag_filter_bar import TagFilterBar

logger = logging.getLogger(__name__)


class FilterBar(QWidget):
    """Combined filter bar containing colour, tag, and folder filters.

    Signals:
        color_filter_changed: Forwarded from ``ColorFilterBar``.
        tag_filter_changed: Forwarded from ``TagFilterBar``.
        folder_filter_changed: Emitted when the set of selected folder
            paths changes.  Payload is a ``set[str]`` of selected folder
            paths (empty set means no folder filter).
    """

    color_filter_changed = Signal(set)  # set of "color:<bucket>" strings
    tag_filter_changed = Signal(set)  # set of int — active filter tag IDs
    folder_filter_changed = Signal(set)  # set of str — selected folder paths

    def __init__(
        self,
        tag_service: TagService,
        db: Database,
        parent: QWidget | None = None,
    ) -> None:
        """Build the combined filter bar.

        Args:
            tag_service: TagService instance for the tag filter bar.
            db: Database instance for populating the folder menu.
            parent: Optional parent widget.
        """
        super().__init__(parent)
        self._db = db
        self._selected_folders: set[str] = set()
        self._folder_chips: dict[str, QWidget] = {}

        # ── Sub-widgets ──────────────────────────────────────────────
        self._color_filter_bar = ColorFilterBar()
        self._tag_filter_bar = TagFilterBar(tag_service)

        # "Add Folder+" button (only visible in global scope)
        self._add_folder_btn = QPushButton("Add Folder+")
        self._add_folder_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._add_folder_btn.setToolTip("Filter by folder (All Images mode only)")
        self._add_folder_btn.clicked.connect(self._on_add_folder_clicked)

        # Container for active folder chips
        self._folder_chips_container = QWidget()
        self._folder_chips_layout = QHBoxLayout(self._folder_chips_container)
        self._folder_chips_layout.setContentsMargins(0, 0, 0, 0)
        self._folder_chips_layout.setSpacing(XS)

        # Folder menu (reused, cleared & repopulated on each show)
        self._folder_menu = QMenu(self)

        # ── Layout ───────────────────────────────────────────────────
        layout = FlowLayout(self, margin=4, spacing=6)

        layout.addWidget(self._color_filter_bar)
        layout.addWidget(self._tag_filter_bar)
        layout.addWidget(self._add_folder_btn)
        layout.addWidget(self._folder_chips_container)

        # ── Forward child signals ────────────────────────────────────
        # Use lambda wrappers instead of connecting directly to .emit,
        # which is an unreliable pattern in PySide6.
        self._color_filter_bar.color_filter_changed.connect(
            lambda colors: self.color_filter_changed.emit(colors)
        )
        self._tag_filter_bar.tag_filter_changed.connect(
            lambda tag_ids: self.tag_filter_changed.emit(tag_ids)
        )

        # ── Initial state: folder widgets hidden (local/folder mode) ─
        self._add_folder_btn.hide()
        self._folder_chips_container.hide()

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
        """Show or hide the folder widgets based on gallery scope.

        Args:
            is_global: ``True`` when "All Images" tab is active.
        """
        self._add_folder_btn.setVisible(is_global)
        # Folder chips remain visible if folders are selected, regardless of scope
        if is_global:
            self._update_folder_chips_visibility()

    def refresh_folders(self) -> None:
        """Refresh the folder menu (no-op for chip UI; kept for API compat)."""
        # With chips, there's no dropdown list to refresh.
        # However, we should prune chips for folders that no longer exist.
        self._prune_stale_chips()

    # ── Internal helpers ─────────────────────────────────────────────

    def _on_add_folder_clicked(self) -> None:
        """Show a menu of available folders not yet selected."""
        self._folder_menu.clear()

        try:
            all_folders = self._db.list_distinct_folders()
        except Exception:
            logger.debug("Failed to load folder list", exc_info=True)
            return

        # Only show folders not already selected
        available = sorted(f for f in all_folders if f not in self._selected_folders)

        if not available:
            action = self._folder_menu.addAction("(all folders added)")
            action.setEnabled(False)
        else:
            for folder_path in available:
                display_name = _short_folder_name(folder_path)
                action = self._folder_menu.addAction(display_name)
                action.setToolTip(folder_path)
                action.triggered.connect(lambda _checked=False, fp=folder_path: self._add_folder_chip(fp))

        # Show menu below the Add Folder+ button
        pos = self._add_folder_btn.mapToGlobal(self._add_folder_btn.rect().bottomLeft())
        self._folder_menu.popup(pos)

    def _add_folder_chip(self, folder_path: str) -> None:
        """Add a folder as a selected chip and emit the updated set.

        Args:
            folder_path: Full folder path to add.
        """
        if folder_path in self._selected_folders:
            return  # Already selected

        self._selected_folders.add(folder_path)
        chip = self._create_folder_chip(folder_path)
        self._folder_chips[folder_path] = chip
        self._folder_chips_layout.addWidget(chip)
        self._update_folder_chips_visibility()
        self.folder_filter_changed.emit(set(self._selected_folders))

    def _remove_folder_chip(self, folder_path: str) -> None:
        """Remove a folder chip and emit the updated set.

        Args:
            folder_path: Full folder path to remove.
        """
        self._selected_folders.discard(folder_path)
        chip = self._folder_chips.pop(folder_path, None)
        if chip is not None:
            self._folder_chips_layout.removeWidget(chip)
            chip.deleteLater()
        self._update_folder_chips_visibility()
        self.folder_filter_changed.emit(set(self._selected_folders))

    def _create_folder_chip(self, folder_path: str) -> QWidget:
        """Create a removable folder chip widget.

        Delegates to the shared chip factory for consistent styling.

        Args:
            folder_path: Full folder path for the chip.

        Returns:
            A QWidget containing the folder name label and a remove button.
        """
        display_name = _short_folder_name(folder_path)
        return create_removable_chip(
            label_text=display_name,
            on_remove=lambda fp=folder_path: self._remove_folder_chip(fp),
            tooltip=folder_path,
        )

    def _update_folder_chips_visibility(self) -> None:
        """Show or hide the folder chips container based on selection."""
        has_chips = len(self._selected_folders) > 0
        self._folder_chips_container.setVisible(has_chips)

    def _prune_stale_chips(self) -> None:
        """Remove chips for folders that no longer exist in the database."""
        try:
            current_folders = set(self._db.list_distinct_folders())
        except Exception:
            logger.debug("Failed to load folder list for pruning", exc_info=True)
            return

        stale = self._selected_folders - current_folders
        for folder_path in stale:
            self._remove_folder_chip(folder_path)


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
