"""Inline tag filter widget for the gallery top bar.

Provides an "Add Tag" button that opens a context menu of checkable tag
actions, plus removable chips showing the currently active tag filters.
"""

from __future__ import annotations

import logging
from functools import partial

from PySide6.QtWidgets import QWidget

from tarragon.db.common.tag import Tag, TagSource
from tarragon.services.tag_service import TagService
from tarragon.widgets._chip_utils import create_removable_chip
from tarragon.widgets.filter_bar_chip_base import _ChipFilterBarBase

logger = logging.getLogger(__name__)


class FilterBarTag(_ChipFilterBarBase[Tag]):
    """A compact tag filter widget with an Add Tag button and active-tag chips.

    Emits whenever the set of active tag IDs changes.
    The payload is a ``set[int]`` of active filter tag IDs.
    """

    def __init__(self, tag_service: TagService, parent: QWidget | None = None) -> None:
        """Build the filter bar with an Add Tag button and chips container."""
        super().__init__("Filter Tags", parent=parent)
        self._tag_service = tag_service
        # Alias the base's active-set and menu to the names pinned by tests.
        self._active_tags = self._active
        self._tag_menu = self._menu
        self._available_tags: set[Tag] = set()

        # React to external tag changes
        self._tag_service.tags_changed.connect(self._refresh_tags)

        # Initial tag load
        self._refresh_tags()

    def _refresh_tags(self) -> None:
        """Rebuild the available tag list from the service.

        Filters out tags whose source is ``TagSource.AUTO_COLOR`` and
        preserves the current active filter selections across rebuilds.
        """
        tags = self._tag_service.get_all_tags()
        self._available_tags = {t for t in tags if not t.get_source() == TagSource.AUTO_COLOR}

        # Drop any active tags that no longer exist in the available list.
        # NOTE: pruning intentionally does NOT emit (follow-up intel, audit #18).
        for tag in list(self._active_tags - self._available_tags):
            self._discard_item_silently(tag)
        self._update_chips()

    def _load_items(self) -> list[Tag]:
        """Return the available tags sorted by name for the menu."""
        return sorted(self._available_tags)

    def _item_label(self, item: Tag) -> str:
        """Return the tag name shown in the menu."""
        return item.get_name()

    def _create_chip(self, tag: Tag) -> QWidget:
        """Create a removable tag chip widget.

        Delegates to the shared chip factory for consistent styling.

        Parameters
        ----------
        tag:
            The tag object.

        Returns
        -------
            A QWidget containing the tag name label and a remove button.
        """
        return create_removable_chip(
            label_text=tag.get_name(),
            on_remove=partial(self._remove_tag, tag),
        )

    def _toggle_tag(self, tag: Tag) -> None:
        """Toggle a tag in the active filter set."""
        self._toggle_item(tag)

    def _remove_tag(self, tag: Tag) -> None:
        """Remove a tag from the active filter set."""
        self._remove_item(tag)

    def get_active_tags(self) -> set[Tag]:
        """Return the set of currently active tags."""
        return set(self._active_tags)
