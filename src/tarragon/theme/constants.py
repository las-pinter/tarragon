"""Consolidated spacing, motion, and layout constants from design tokens.

Loads all three sections of *tokens.json* in a single call and exposes
only the constants that have production consumers.  Use these instead of
hard-coding pixel values, animation durations, or layout dimensions.

Example::

    from tarragon.theme.constants import SM, DURATION_FAST, GRID_GAP
    layout.setContentsMargins(SM, SM, SM, SM)
    animation.setDuration(DURATION_FAST)
    grid.setSpacing(GRID_GAP)
"""

from __future__ import annotations

from tarragon.theme.tokens import load_tokens as _load_tokens

_tokens = _load_tokens()
_spacing: dict[str, int] = _tokens["spacing"]
_motion: dict[str, int | str] = _tokens["motion"]
_layout: dict[str, int] = _tokens["layout"]

# ── Spacing ──────────────────────────────────────────────────────────────────

#: Extra-small spacing — 4 px.  Tight gaps between related items.
XS: int = _spacing["xs"]

#: Small spacing — 8 px.  Default inner padding and icon-to-text gaps.
SM: int = _spacing["sm"]

#: Medium spacing — 12 px.  Standard group separation.
MD: int = _spacing["md"]

# ── Motion ───────────────────────────────────────────────────────────────────

#: Fast duration — 150 ms.  Micro-interactions: button press, checkbox toggle.
DURATION_FAST: int = _motion["duration_fast"]  # type: ignore[assignment]

#: Normal duration — 200 ms.  Standard transitions: panel slide, fade-in.
DURATION_NORMAL: int = _motion["duration_normal"]  # type: ignore[assignment]

# ── Layout ───────────────────────────────────────────────────────────────────

#: Default grid-gap between thumbnails — 14 px.
GRID_GAP: int = _layout["grid_gap"]

#: Thumbnail width/height — 160 px.  Used for preview tiles.
THUMBNAIL_SIZE: int = _layout["thumbnail_size"]

#: Sidebar width — 220 px.
SIDEBAR_WIDTH_PX: int = _layout["sidebar_width_px"]

#: Maximum number of thumbnails shown in the multi-preview strip — 9.
MULTI_PREVIEW_MAX_DEFAULT: int = _layout["multi_preview_max_default"]

__all__ = [
    "DURATION_FAST",
    "DURATION_NORMAL",
    "GRID_GAP",
    "MD",
    "MULTI_PREVIEW_MAX_DEFAULT",
    "SIDEBAR_WIDTH_PX",
    "SM",
    "THUMBNAIL_SIZE",
    "XS",
]
