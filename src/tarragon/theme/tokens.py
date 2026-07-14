"""Design tokens for Tarragon's dark coral-amber aesthetic.

All design tokens are defined as Python data structures in this module.
The :func:`load_tokens` and :func:`get_token` functions provide the
same API as the previous JSON-based implementation.
"""

from __future__ import annotations

import copy
from typing import Any

_TOKENS: dict[str, Any] = {
    "colors": {
        "bg_primary": "#16151A",
        "bg_secondary": "#1c1b22",
        "bg_tertiary": "#211f29",
        "surface_highlight": "#3D3B44",
        "surface_hover": "#2a2836",
        "coral_strong": "#F0997B",
        "coral_muted": "#D85A30",
        "coral_dark": "#4A1B0C",
        "coral_bright": "#E06540",
        "amber_accent": "#FAC775",
        "amber_light": "#FFD480",
        "amber_dark": "#412402",
        "text_primary": "#ece9f2",
        "text_secondary": "#8d8a98",
        "text_tertiary": "#a8a5b0",
        "text_muted": "#65626f",
        "bg_disabled": "#1a1820",
        "border_disabled": "#3a3845",
        "border_subtle": "rgba(255,255,255,0.06)",
        "border_card": "rgba(255,255,255,0.08)",
        "border_interactive": "rgba(255,255,255,0.12)",
        "bg_log_panel": "#1a1a2e",
        "highlight_disabled": "#4A3A35",
        "separator": "#1E1D23",
    },
    "typography": {
        "font_family": "Segoe UI, -apple-system, sans-serif",
        "mono_family": "Consolas, Courier New, monospace",
        "body_size": 12,
        "heading_size": 16,
        "small_size": 10,
        "log_size": 11,
        "caption_size": 11,
        "weight_regular": 400,
        "weight_medium": 500,
        "weight_semibold": 600,
    },
    "spacing": {
        "xs": 4,
        "sm": 8,
        "md": 12,
        "lg": 16,
        "xl": 24,
    },
    "radius": {
        "none": 0,
        "xs": 3,
        "sm": 4,
        "md": 6,
        "lg": 8,
        "xl": 10,
    },
    "motion": {
        "duration_fast": 150,
        "duration_normal": 200,
        "easing": "ease-out",
    },
    "layout": {
        "thumbnail_size": 160,
        "grid_gap": 14,
        "sidebar_width_px": 220,
        "multi_preview_max_default": 9,
    },
    "badge": {
        "bg_vinous": "#4A1B0C",
        "fg_vinous": "#F0997B",
        "bg_sage": "#1A3A2A",
        "fg_sage": "#7BC88F",
        "bg_navy": "#1A2A3A",
        "fg_navy": "#7BA8C8",
        "bg_umber": "#3A2A1A",
        "fg_umber": "#C8A87B",
        "bg_plum": "#2A1A3A",
        "fg_plum": "#A87BC8",
        "bg_teal": "#1A3A3A",
        "fg_teal": "#7BC8C8",
        "bg_olive": "#3A3A1A",
        "fg_olive": "#C8C87B",
        "bg_azure": "#2E86AB",
        "fg_azure": "#D4EEF7",
        "bg_neutral": "#2A2A2A",
        "fg_neutral": "#A8A5B0",
    },
}


def load_tokens() -> dict[str, Any]:
    """Load and return a copy of the design tokens.

    Returns:
        Dictionary with keys: badge, colors, typography, spacing, radius, motion, layout.
    """
    return copy.deepcopy(_TOKENS)


def get_token(section: str, key: str) -> Any:
    """Return a single token value by section and key.

    Args:
        section: Top-level token group (e.g. 'colors', 'typography').
        key: The specific token name within that section.

    Returns:
        The token value.

    Raises:
        KeyError: If the section or key does not exist in tokens.
    """
    return _TOKENS[section][key]


__all__ = ["load_tokens", "get_token"]
