"""Settings service — typed accessors and validation for settings.

Provides a thin wrapper over Settings that enforces type coercion and
range clamping for numeric configuration values, preventing invalid
state from reaching the persistence layer.
"""

from __future__ import annotations

import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from tarragon.settings import Settings

# ── Range constraints ────────────────────────────────────────────────────────

_MAX_PSD_WORKERS_RANGE = (1, 8)
_MAX_MULTI_PREVIEW_RANGE = (1, 100)
_LARGE_CANVAS_THRESHOLD_RANGE = (0.1, 1000.0)
_COLOR_TAG_PALETTE_SIZE_RANGE = (2, 32)
_COLOR_TAG_MIN_SHARE_RANGE = (0.0, 1.0)
_COLOR_TAG_NEUTRAL_S_THRESHOLD_RANGE = (0.0, 1.0)
_VALID_CACHE_FORMATS = ("png", "jpeg")
_TILE_GRID_PATTERN = re.compile(r"^\d+x\d+$")


def _clamp(value: int | float, lo: int | float, hi: int | float) -> int | float:
    """Clamp *value* to the inclusive range [*lo*, *hi*]."""
    return max(lo, min(hi, value))


class SettingsService:
    """Typed accessors and validation for application settings.

    Wraps a :class:`~tarragon.settings.Settings` instance and exposes
    named getter/setter pairs for every known configuration key.  Numeric
    values are coerced to the correct type and clamped to sensible ranges;
    string and boolean values are validated against allowed options.
    """

    def __init__(self, settings: Settings) -> None:
        self._settings = settings

    # ── max_psd_workers ──────────────────────────────────────────────────────

    def get_max_psd_workers(self) -> int:
        """Get max PSD workers, clamped to [1, 8]."""
        value = int(self._settings.get("max_psd_workers"))
        return int(_clamp(value, *_MAX_PSD_WORKERS_RANGE))

    def set_max_psd_workers(self, value: int) -> None:
        """Set max PSD workers, clamped to [1, 8]."""
        clamped = int(_clamp(int(value), *_MAX_PSD_WORKERS_RANGE))
        self._settings.set("max_psd_workers", clamped)

    # ── max_multi_preview ────────────────────────────────────────────────────

    def get_max_multi_preview(self) -> int:
        """Get max multi-preview count, clamped to [1, 100]."""
        value = int(self._settings.get("max_multi_preview"))
        return int(_clamp(value, *_MAX_MULTI_PREVIEW_RANGE))

    def set_max_multi_preview(self, value: int) -> None:
        """Set max multi-preview count, clamped to [1, 100]."""
        clamped = int(_clamp(int(value), *_MAX_MULTI_PREVIEW_RANGE))
        self._settings.set("max_multi_preview", clamped)

    # ── large_canvas_threshold_mp ────────────────────────────────────────────

    def get_large_canvas_threshold_mp(self) -> float:
        """Get large canvas threshold in megapixels, clamped to [0.1, 1000.0]."""
        value = float(self._settings.get("large_canvas_threshold_mp"))
        return float(_clamp(value, *_LARGE_CANVAS_THRESHOLD_RANGE))

    def set_large_canvas_threshold_mp(self, value: float) -> None:
        """Set large canvas threshold in megapixels, clamped to [0.1, 1000.0]."""
        clamped = float(_clamp(float(value), *_LARGE_CANVAS_THRESHOLD_RANGE))
        self._settings.set("large_canvas_threshold_mp", clamped)

    # ── tile_grid_size ───────────────────────────────────────────────────────

    def get_tile_grid_size(self) -> str:
        """Get tile grid size string (e.g. '2x2')."""
        value = str(self._settings.get("tile_grid_size"))
        if not _TILE_GRID_PATTERN.match(value):
            return "2x2"
        return value

    def set_tile_grid_size(self, value: str) -> None:
        """Set tile grid size string.

        Must match the format ``'NxN'`` where N is a positive integer.
        Raises ``ValueError`` for invalid formats.
        """
        if not _TILE_GRID_PATTERN.match(str(value)):
            raise ValueError(f"Invalid tile_grid_size: {value!r}. Expected format 'NxN' (e.g. '2x2').")
        self._settings.set("tile_grid_size", str(value))

    # ── color_tag_enabled ────────────────────────────────────────────────────

    def get_color_tag_enabled(self) -> bool:
        """Get whether color tagging is enabled."""
        return bool(self._settings.get("color_tag_enabled"))

    def set_color_tag_enabled(self, value: bool) -> None:
        """Set whether color tagging is enabled."""
        self._settings.set("color_tag_enabled", bool(value))

    # ── color_tag_palette_size ───────────────────────────────────────────────

    def get_color_tag_palette_size(self) -> int:
        """Get color tag palette size, clamped to [2, 32]."""
        value = int(self._settings.get("color_tag_palette_size"))
        return int(_clamp(value, *_COLOR_TAG_PALETTE_SIZE_RANGE))

    def set_color_tag_palette_size(self, value: int) -> None:
        """Set color tag palette size, clamped to [2, 32]."""
        clamped = int(_clamp(int(value), *_COLOR_TAG_PALETTE_SIZE_RANGE))
        self._settings.set("color_tag_palette_size", clamped)

    # ── color_tag_min_share ──────────────────────────────────────────────────

    def get_color_tag_min_share(self) -> float:
        """Get color tag minimum share, clamped to [0.0, 1.0]."""
        value = float(self._settings.get("color_tag_min_share"))
        return float(_clamp(value, *_COLOR_TAG_MIN_SHARE_RANGE))

    def set_color_tag_min_share(self, value: float) -> None:
        """Set color tag minimum share, clamped to [0.0, 1.0]."""
        clamped = float(_clamp(float(value), *_COLOR_TAG_MIN_SHARE_RANGE))
        self._settings.set("color_tag_min_share", clamped)

    # ── color_tag_neutral_s_threshold ────────────────────────────────────────

    def get_color_tag_neutral_s_threshold(self) -> float:
        """Get color tag neutral saturation threshold, clamped to [0.0, 1.0]."""
        value = float(self._settings.get("color_tag_neutral_s_threshold"))
        return float(_clamp(value, *_COLOR_TAG_NEUTRAL_S_THRESHOLD_RANGE))

    def set_color_tag_neutral_s_threshold(self, value: float) -> None:
        """Set color tag neutral saturation threshold, clamped to [0.0, 1.0]."""
        clamped = float(_clamp(float(value), *_COLOR_TAG_NEUTRAL_S_THRESHOLD_RANGE))
        self._settings.set("color_tag_neutral_s_threshold", clamped)

    # ── cache_format ─────────────────────────────────────────────────────────

    def get_cache_format(self) -> str:
        """Get cache format ('png' or 'jpeg')."""
        value = str(self._settings.get("cache_format"))
        if value not in _VALID_CACHE_FORMATS:
            return "png"
        return value

    def set_cache_format(self, value: str) -> None:
        """Set cache format. Must be 'png' or 'jpeg'.

        Raises ``ValueError`` for unsupported formats.
        """
        if str(value) not in _VALID_CACHE_FORMATS:
            raise ValueError(f"Invalid cache_format: {value!r}. Expected one of {_VALID_CACHE_FORMATS}.")
        self._settings.set("cache_format", str(value))
