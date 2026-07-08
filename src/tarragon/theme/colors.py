"""Typed QColor constants derived from tokens.json.

Every color in the ``colors`` section of *tokens.json* is exposed as a
module-level :class:`QColor` constant using ``UPPER_CASE`` naming.

Example::

    from tarragon.theme.colors import BG_PRIMARY, CORAL_STRONG
    widget.setStyleSheet(f"background: {BG_PRIMARY.name()};")
"""

from __future__ import annotations

from PySide6.QtGui import QColor

from tarragon.theme.tokens import load_tokens

# ── Load tokens once at import time ──────────────────────────────────────────
_colors: dict[str, str] = load_tokens()["colors"]

# ── Background colors ────────────────────────────────────────────────────────
BG_PRIMARY: QColor = QColor(_colors["bg_primary"])
BG_SECONDARY: QColor = QColor(_colors["bg_secondary"])
BG_TERTIARY: QColor = QColor(_colors["bg_tertiary"])
SURFACE_HIGHLIGHT: QColor = QColor(_colors["surface_highlight"])

# ── Accent colors ────────────────────────────────────────────────────────────
CORAL_STRONG: QColor = QColor(_colors["coral_strong"])
CORAL_MUTED: QColor = QColor(_colors["coral_muted"])
CORAL_DARK: QColor = QColor(_colors["coral_dark"])
CORAL_BRIGHT: QColor = QColor(_colors["coral_bright"])
AMBER_ACCENT: QColor = QColor(_colors["amber_accent"])
AMBER_LIGHT: QColor = QColor(_colors["amber_light"])
AMBER_DARK: QColor = QColor(_colors["amber_dark"])

# ── Surface interaction colors ───────────────────────────────────────────────
SURFACE_HOVER: QColor = QColor(_colors["surface_hover"])

# ── Text colors ──────────────────────────────────────────────────────────────
TEXT_PRIMARY: QColor = QColor(_colors["text_primary"])
TEXT_SECONDARY: QColor = QColor(_colors["text_secondary"])
TEXT_TERTIARY: QColor = QColor(_colors["text_tertiary"])
TEXT_MUTED: QColor = QColor(_colors["text_muted"])

# ── Border colors ────────────────────────────────────────────────────────────
BORDER_SUBTLE: QColor = QColor(_colors["border_subtle"])
BORDER_CARD: QColor = QColor(_colors["border_card"])
BORDER_INTERACTIVE: QColor = QColor(_colors["border_interactive"])

# ── Misc colors ──────────────────────────────────────────────────────────────
BG_DISABLED: QColor = QColor(_colors["bg_disabled"])
BORDER_DISABLED: QColor = QColor(_colors["border_disabled"])
BG_LOG_PANEL: QColor = QColor(_colors["bg_log_panel"])
HIGHLIGHT_DISABLED: QColor = QColor(_colors["highlight_disabled"])
SEPARATOR: QColor = QColor(_colors["separator"])

__all__ = [
    "AMBER_ACCENT",
    "AMBER_DARK",
    "AMBER_LIGHT",
    "BG_DISABLED",
    "BG_LOG_PANEL",
    "BG_PRIMARY",
    "BG_SECONDARY",
    "BG_TERTIARY",
    "BORDER_CARD",
    "BORDER_DISABLED",
    "BORDER_INTERACTIVE",
    "BORDER_SUBTLE",
    "CORAL_BRIGHT",
    "CORAL_DARK",
    "CORAL_MUTED",
    "CORAL_STRONG",
    "HIGHLIGHT_DISABLED",
    "SEPARATOR",
    "SURFACE_HIGHLIGHT",
    "SURFACE_HOVER",
    "TEXT_MUTED",
    "TEXT_PRIMARY",
    "TEXT_SECONDARY",
    "TEXT_TERTIARY",
]
