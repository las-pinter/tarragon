"""FilterState — composite filter state for the image browser."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class FilterState:
    """Composite filter state for the image browser.

    Tracks three independent filter dimensions:

    - **filename_filter**: substring matched against file names.
    - **tag_ids**: set of tag primary-key IDs that must all be present.
    - **color_tags**: set of color-label strings applied as a filter.

    All dimensions are combined with AND semantics — an image must satisfy
    every active filter to remain visible.
    """

    filename_filter: str = ""
    tag_ids: set[int] = field(default_factory=set)
    color_tags: set[str] = field(default_factory=set)
    folder_filter: str = ""

    def is_empty(self) -> bool:
        """Return ``True`` if no filters are active."""
        return (
            not self.filename_filter
            and not self.tag_ids
            and not self.color_tags
            and not self.folder_filter
        )

    def clear(self) -> None:
        """Clear all filters, restoring the unfiltered state."""
        self.filename_filter = ""
        self.tag_ids.clear()
        self.color_tags.clear()
        self.folder_filter = ""
