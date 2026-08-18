"""Composite filter state for the image browser."""

from __future__ import annotations

from dataclasses import dataclass, field

from tarragon.db.common.tag import Tag


@dataclass
class FilterState:
    """Composite filter state for the image browser.

    Tracks four independent filter dimensions:

    - **filename_filter**: substring matched against file names.
    - **tags**: set of tags that must all be present.
    - **color_tags**: set of color tags applied as a filter.
    - **folder_filters**: set of folder paths for multi-folder scoping.

    All dimensions are combined with AND semantics — an image must satisfy
    every active filter to remain visible.1
    """

    filename_filter: str = ""
    tags: set[Tag] = field(default_factory=set[Tag])
    color_tags: set[Tag] = field(default_factory=set[Tag])
    folder_filters: set[str] = field(default_factory=set[str])

    def is_empty(self) -> bool:
        """Return ``True`` if no filters are active."""
        return not self.filename_filter and not self.tags and not self.color_tags and not self.folder_filters

    def clear(self) -> None:
        """Clear all filters, restoring the unfiltered state."""
        self.filename_filter = ""
        self.tags.clear()
        self.color_tags.clear()
        self.folder_filters.clear()
