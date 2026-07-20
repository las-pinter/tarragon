"""Consolidated color-bucket definitions for the color tagger and UI widgets."""

from __future__ import annotations

from collections.abc import Mapping

# Display order
BUCKET_COLORS: tuple[str, ...] = (
    "red",
    "orange",
    "yellow",
    "green",
    "teal",
    "cyan",
    "blue",
    "purple",
    "magenta",
    "neutral",
)

# Representative hex colors
BUCKET_HEX_COLORS: Mapping[str, str] = {
    "red": "#E74C3C",
    "orange": "#F39C12",
    "yellow": "#F1C40F",
    "green": "#27AE60",
    "teal": "#1ABC9C",
    "cyan": "#00BCD4",
    "blue": "#3498DB",
    "purple": "#9B59B6",
    "magenta": "#E91E63",
    "neutral": "#7F8C8D",
}

# Hue ranges for classification
#
# Each entry is a tuple of (min, max) half-open intervals covering the
# hue values (0–360) that belong to that bucket.  Red has two ranges to
# handle the 0°/360° wrap-around.
#
# Hues that fall in a gap between buckets (310–345°) are not matched by
# any chromatic bucket and should be treated as neutral.
COLOR_BUCKETS: Mapping[str, tuple[tuple[float, float], ...]] = {
    "red": ((345, 360), (0, 15)),
    "orange": ((15, 33),),
    "yellow": ((33, 50),),
    "green": ((50, 85),),
    "teal": ((85, 105),),
    "cyan": ((105, 140),),
    "blue": ((140, 200),),
    "purple": ((200, 270),),
    "magenta": ((270, 310),),
}
