"""TagFilterBar — inline tag filter widget for the gallery top bar.

Provides an "Add Tag+" button that opens a context menu of checkable tag
actions, plus removable chips showing the currently active tag filters.
Auto-color tags (``color:``) are excluded from the filter options.

Design patterns:
    - Observer pattern  — ``tag_filter_changed`` signal for filter changes
    - Service Layer     — uses ``TagService``, never touches DB directly
"""

from __future__ import annotations

from functools import partial

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QHBoxLayout,
    QMenu,
    QPushButton,
    QWidget,
)

from tarragon.services.tag_service import TagService
from tarragon.theme.constants import SPACING_S
from tarragon.widgets._chip_utils import create_removable_chip


class TagFilterBar(QWidget):
    """A compact tag filter widget with an Add Tag+ button and active-tag chips.

    Emits ``tag_filter_changed(set)`` whenever the set of active tag IDs
    changes.  The payload is a ``set[int]`` of active filter tag IDs.
    """

    tag_filter_changed = Signal(set)  # set of int — active filter tag IDs

    def __init__(self, tag_service: TagService, parent: QWidget | None = None) -> None:
        """Build the filter bar with an Add Tag+ button and chips container."""
        super().__init__(parent)
        self._tag_service = tag_service
        self._active_tag_ids: set[int] = set()
        self._available_tags: dict[int, str] = {}  # id -> name (excl. auto-color)

        # ── Layout ──────────────────────────────────────────────────────
        layout = QHBoxLayout(self)
        # No vertical margins — the parent FlowLayout handles vertical
        # centering of items with different heights.  Keeping vertical
        # margins here would push the Add Tag+ button below the baseline
        # of sibling widgets (e.g. the Add Folder+ button) that have no
        # such internal padding.
        layout.setContentsMargins(SPACING_S, 0, SPACING_S, 0)
        layout.setSpacing(SPACING_S)

        # "Add Tag+" button — always visible, opens tag menu on click
        self._add_button = QPushButton("Add Tag+")
        self._add_button.setCursor(Qt.CursorShape.PointingHandCursor)
        self._add_button.clicked.connect(self._show_tag_menu)
        layout.addWidget(self._add_button)

        # Container for active tag chips
        self._chips_container = QWidget()
        self._chips_layout = QHBoxLayout(self._chips_container)
        self._chips_layout.setContentsMargins(0, 0, 0, 0)
        self._chips_layout.setSpacing(SPACING_S)
        layout.addWidget(self._chips_container)

        layout.addStretch()

        # Tag menu (reused, cleared & repopulated on each show)
        self._tag_menu = QMenu(self)

        # React to external tag changes (new tags created, tags deleted)
        self._tag_service.tags_changed.connect(self._refresh_tags)

        # Initial tag load
        self._refresh_tags()

    # ── Public API ──────────────────────────────────────────────────────

    def get_active_tag_ids(self) -> set[int]:
        """Return the set of currently active tag filter IDs."""
        return set(self._active_tag_ids)

    def has_active_filters(self) -> bool:
        """Return True if any tag filter is active."""
        return len(self._active_tag_ids) > 0

    def clear_filters(self) -> None:
        """Remove all active tag filters and emit an empty set."""
        self._active_tag_ids.clear()
        self._update_chips()
        self.tag_filter_changed.emit(set())

    # ── Internal helpers ───────────────────────────────────────────────

    def _refresh_tags(self) -> None:
        """Rebuild the available tag list from the service.

        Filters out auto-color tags (``color:`` prefix) and preserves
        the current active filter selections across rebuilds.
        """
        tags = self._tag_service.get_all_tags()
        self._available_tags = {tag["id"]: tag["name"] for tag in tags if not tag["name"].startswith("color:")}

        # Drop any active IDs that no longer exist in available tags
        self._active_tag_ids &= set(self._available_tags.keys())
        self._update_chips()

    def _show_tag_menu(self) -> None:
        """Show context menu with available tags as checkable actions."""
        self._tag_menu.clear()

        for tag_id, tag_name in sorted(self._available_tags.items(), key=lambda x: x[1]):
            action = self._tag_menu.addAction(tag_name)
            action.setCheckable(True)
            action.setChecked(tag_id in self._active_tag_ids)
            action.setData(tag_id)
            action.triggered.connect(lambda checked, tid=tag_id: self._toggle_tag(tid))

        # Show menu below the Add Tag+ button
        pos = self._add_button.mapToGlobal(self._add_button.rect().bottomLeft())
        self._tag_menu.popup(pos)

    def _toggle_tag(self, tag_id: int) -> None:
        """Toggle a tag in the active filter set."""
        if tag_id in self._active_tag_ids:
            self._active_tag_ids.remove(tag_id)
        else:
            self._active_tag_ids.add(tag_id)
        self._update_chips()
        self.tag_filter_changed.emit(set(self._active_tag_ids))

    def _remove_tag(self, tag_id: int) -> None:
        """Remove a tag from the active filter set."""
        self._active_tag_ids.discard(tag_id)
        self._update_chips()
        self.tag_filter_changed.emit(set(self._active_tag_ids))

    def _update_chips(self) -> None:
        """Rebuild the displayed tag chips to match the active filter set."""
        # Clear existing chips
        while self._chips_layout.count():
            item = self._chips_layout.takeAt(0)
            if item is not None:
                widget = item.widget()
                if widget is not None:
                    widget.deleteLater()

        # Add chips for each active tag
        for tag_id in sorted(self._active_tag_ids):
            tag_name = self._available_tags.get(tag_id, f"Tag {tag_id}")
            chip = self._create_chip(tag_id, tag_name)
            self._chips_layout.addWidget(chip)

    def _create_chip(self, tag_id: int, tag_name: str) -> QWidget:
        """Create a removable tag chip widget.

        Delegates to the shared chip factory for consistent styling.

        Parameters
        ----------
        tag_id:
            The database ID of the tag.
        tag_name:
            Display name for the tag label.

        Returns
        -------
            A QWidget containing the tag name label and a remove button.
        """
        return create_removable_chip(
            label_text=tag_name,
            on_remove=partial(self._remove_tag, tag_id),
        )
