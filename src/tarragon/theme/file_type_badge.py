"""File extension badge color definitions.

Maps file extensions to (background, text) :class:`QColor` pairs used by
:class:`~tarragon.widgets.thumbnail_grid.ThumbnailDelegate` to paint a
color-coded badge on each thumbnail.
"""

from __future__ import annotations

from PySide6.QtGui import QColor

from tarragon.theme.colors import (
    BADGE_BG_AZURE,
    BADGE_BG_NAVY,
    BADGE_BG_NEUTRAL,
    BADGE_BG_OLIVE,
    BADGE_BG_PLUM,
    BADGE_BG_SAGE,
    BADGE_BG_TEAL,
    BADGE_BG_UMBER,
    BADGE_BG_VINOUS,
    BADGE_FG_AZURE,
    BADGE_FG_NAVY,
    BADGE_FG_NEUTRAL,
    BADGE_FG_OLIVE,
    BADGE_FG_PLUM,
    BADGE_FG_SAGE,
    BADGE_FG_TEAL,
    BADGE_FG_UMBER,
    BADGE_FG_VINOUS,
)

# Map of file extension -> (background_color, text_color)
BADGE_COLORS: dict[str, tuple[QColor, QColor]] = {
    "psd": (BADGE_BG_VINOUS, BADGE_FG_VINOUS),
    "psb": (BADGE_BG_VINOUS, BADGE_FG_VINOUS),
    "jpg": (BADGE_BG_SAGE, BADGE_FG_SAGE),
    "jpeg": (BADGE_BG_SAGE, BADGE_FG_SAGE),
    "png": (BADGE_BG_NAVY, BADGE_FG_NAVY),
    "tiff": (BADGE_BG_UMBER, BADGE_FG_UMBER),
    "tif": (BADGE_BG_UMBER, BADGE_FG_UMBER),
    "gif": (BADGE_BG_PLUM, BADGE_FG_PLUM),
    "webp": (BADGE_BG_TEAL, BADGE_FG_TEAL),
    "bmp": (BADGE_BG_OLIVE, BADGE_FG_OLIVE),
    "clip": (BADGE_BG_AZURE, BADGE_FG_AZURE),
}

# Default fallback colors
DEFAULT_BADGE_COLORS: tuple[QColor, QColor] = (BADGE_BG_NEUTRAL, BADGE_FG_NEUTRAL)


def get_badge_colors(extension: str) -> tuple[QColor, QColor]:
    """Get badge colors for a file extension.

    Args:
        extension: File extension (with or without dot, case-insensitive).

    Returns:
        Tuple of (background_color, text_color).
    """
    ext = extension.lower().lstrip(".")
    return BADGE_COLORS.get(ext, DEFAULT_BADGE_COLORS)


__all__ = [
    "BADGE_COLORS",
    "DEFAULT_BADGE_COLORS",
    "get_badge_colors",
]
