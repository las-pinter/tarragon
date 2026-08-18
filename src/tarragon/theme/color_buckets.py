"""Consolidated color-bucket definitions for the color tagger and UI widgets."""

from __future__ import annotations

from collections.abc import Mapping
from enum import StrEnum


class ColorBucket(StrEnum):
    RED = "red"
    ORANGE = "orange"
    YELLOW = "yellow"
    GREEN = "green"
    TEAL = "teal"
    CYAN = "cyan"
    BLUE = "blue"
    PURPLE = "purple"
    MAGENTA = "magenta"
    NEUTRAL = "neutral"


# Display order
BUCKET_COLORS: tuple[ColorBucket, ...] = (
    ColorBucket.RED,
    ColorBucket.ORANGE,
    ColorBucket.YELLOW,
    ColorBucket.GREEN,
    ColorBucket.TEAL,
    ColorBucket.CYAN,
    ColorBucket.BLUE,
    ColorBucket.PURPLE,
    ColorBucket.MAGENTA,
    ColorBucket.NEUTRAL,
)

# Representative hex colors
BUCKET_HEX_COLORS: Mapping[ColorBucket, str] = {
    ColorBucket.RED: "#E74C3C",
    ColorBucket.ORANGE: "#F39C12",
    ColorBucket.YELLOW: "#F1C40F",
    ColorBucket.GREEN: "#27AE60",
    ColorBucket.TEAL: "#1ABC9C",
    ColorBucket.CYAN: "#00BCD4",
    ColorBucket.BLUE: "#3498DB",
    ColorBucket.PURPLE: "#9B59B6",
    ColorBucket.MAGENTA: "#E91E63",
    ColorBucket.NEUTRAL: "#7F8C8D",
}

# Hue ranges for classification
#
# Each entry is a tuple of (min, max) half-open intervals covering the
# hue values (0–360) that belong to that bucket.  Red has two ranges to
# handle the 0°/360° wrap-around.
#
# Hues that fall in a gap between buckets (310–345°) are not matched by
# any chromatic bucket and should be treated as neutral.
COLOR_BUCKETS: Mapping[ColorBucket, tuple[tuple[float, float], ...]] = {
    ColorBucket.RED: ((345, 360), (0, 15)),
    ColorBucket.ORANGE: ((15, 33),),
    ColorBucket.YELLOW: ((33, 50),),
    ColorBucket.GREEN: ((50, 85),),
    ColorBucket.TEAL: ((85, 105),),
    ColorBucket.CYAN: ((105, 140),),
    ColorBucket.BLUE: ((140, 200),),
    ColorBucket.PURPLE: ((200, 270),),
    ColorBucket.MAGENTA: ((270, 310),),
}
