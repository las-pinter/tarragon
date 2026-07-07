"""File extension badge color definitions.

Maps file extensions to (background, text) :class:`QColor` pairs used by
:class:`~tarragon.widgets.thumbnail_grid.ThumbnailDelegate` to paint a
color-coded badge on each thumbnail.
"""

from __future__ import annotations

from PySide6.QtGui import QColor

# Map of file extension -> (background_color, text_color)
BADGE_COLORS: dict[str, tuple[QColor, QColor]] = {
    "psd": (QColor("#4A1B0C"), QColor("#F0997B")),
    "psb": (QColor("#4A1B0C"), QColor("#F0997B")),
    "jpg": (QColor("#1A3A2A"), QColor("#7BC88F")),
    "jpeg": (QColor("#1A3A2A"), QColor("#7BC88F")),
    "png": (QColor("#1A2A3A"), QColor("#7BA8C8")),
    "tiff": (QColor("#3A2A1A"), QColor("#C8A87B")),
    "tif": (QColor("#3A2A1A"), QColor("#C8A87B")),
    "gif": (QColor("#2A1A3A"), QColor("#A87BC8")),
    "webp": (QColor("#1A3A3A"), QColor("#7BC8C8")),
    "bmp": (QColor("#3A3A1A"), QColor("#C8C87B")),
    "clip": (QColor("#2E86AB"), QColor("#D4EEF7")),
}

# Default fallback colors
DEFAULT_BADGE_COLORS: tuple[QColor, QColor] = (QColor("#2A2A2A"), QColor("#A8A5B0"))


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
