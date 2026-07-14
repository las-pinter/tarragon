"""Layout constants derived from tokens.json.

All values correspond to the ``layout`` section of *tokens.json*.
Pixel values are plain ``int`` counts; use these instead of hard-coding
layout dimensions in widgets, grids, and sidebars.

Example::

    from tarragon.theme.layout import THUMBNAIL_SIZE, GRID_GAP
    grid.setSpacing(GRID_GAP)
    thumbnail.setFixedSize(THUMBNAIL_SIZE, THUMBNAIL_SIZE)
"""

from __future__ import annotations

from tarragon.theme.tokens import load_tokens

_layout: dict[str, int] = load_tokens()["layout"]

#: Default grid-gap between thumbnails — 14 px.
GRID_GAP: int = _layout["grid_gap"]

#: Thumbnail width/height — 160 px.  Used for preview tiles.
THUMBNAIL_SIZE: int = _layout["thumbnail_size"]

#: Sidebar width — 220 px.
SIDEBAR_WIDTH_PX: int = _layout["sidebar_width_px"]

#: Maximum number of thumbnails shown in the multi-preview strip — 9.
MULTI_PREVIEW_MAX_DEFAULT: int = _layout["multi_preview_max_default"]

__all__ = [
    "GRID_GAP",
    "MULTI_PREVIEW_MAX_DEFAULT",
    "SIDEBAR_WIDTH_PX",
    "THUMBNAIL_SIZE",
]
