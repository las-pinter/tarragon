"""Consolidated spacing, motion, layout, and color constants from design tokens."""

from __future__ import annotations

# Spacing
SPACING_XS: int = 4
SPACING_S: int = 8
SPACING_M: int = 12
SPACING_L: int = 16
SPACING_XL: int = 24

# Radius
RADIUS_NONE: int = 0
RADIUS_XS: int = 3
RADIUS_S: int = 4
RADIUS_M: int = 6
RADIUS_L: int = 8
RADIUS_XL: int = 10

# Color (exact-value tokens without a semantic QColor equivalent)
BORDER_INACTIVE: str = "#555555"

# Motion
DURATION_FAST: int = 150
DURATION_NORMAL: int = 200
GRID_GAP: int = 14

# Layout
THUMBNAIL_SIZE: int = 160
SIDEBAR_WIDTH_PX: int = 220
MULTI_PREVIEW_MAX_DEFAULT: int = 9

# PSD worker pool (single source for renderer, settings spec, and tests)
PSD_WORKER_DEFAULT: int = 3
PSD_WORKER_MIN: int = 1
PSD_WORKER_MAX: int = 8
PSD_WORKER_RAM_BYTES: int = 200_000_000
