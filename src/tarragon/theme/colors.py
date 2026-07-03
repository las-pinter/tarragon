"""Typed QColor constants derived from tokens.json.

Every colour in the ``colors`` section of *tokens.json* is exposed as a
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

# ── Background colours ───────────────────────────────────────────────────────
BG_PRIMARY: QColor = QColor(_colors["bg_primary"])
BG_SECONDARY: QColor = QColor(_colors["bg_secondary"])
BG_TERTIARY: QColor = QColor(_colors["bg_tertiary"])
SURFACE_HIGHLIGHT: QColor = QColor(_colors["surface_highlight"])

# ── Accent colours ───────────────────────────────────────────────────────────
CORAL_STRONG: QColor = QColor(_colors["coral_strong"])
CORAL_MUTED: QColor = QColor(_colors["coral_muted"])
AMBER_ACCENT: QColor = QColor(_colors["amber_accent"])

# ── Text colours ─────────────────────────────────────────────────────────────
TEXT_PRIMARY: QColor = QColor(_colors["text_primary"])
TEXT_SECONDARY: QColor = QColor(_colors["text_secondary"])
TEXT_TERTIARY: QColor = QColor(_colors["text_tertiary"])

__all__ = [
    "AMBER_ACCENT",
    "BG_PRIMARY",
    "BG_SECONDARY",
    "BG_TERTIARY",
    "CORAL_MUTED",
    "CORAL_STRONG",
    "SURFACE_HIGHLIGHT",
    "TEXT_PRIMARY",
    "TEXT_SECONDARY",
    "TEXT_TERTIARY",
]
