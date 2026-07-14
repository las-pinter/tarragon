"""Typed QColor constants derived from tokens.json.

Every color in the ``colors`` and ``badge`` sections of *tokens.json* is
exposed as a module-level :class:`QColor` constant using ``UPPER_CASE``
naming.

Example::

    from tarragon.theme.colors import BG_PRIMARY, CORAL_STRONG, BADGE_BG_VINOUS
    widget.setStyleSheet(f"background: {BG_PRIMARY.name()};")
"""

from __future__ import annotations

from PySide6.QtGui import QColor

from tarragon.theme.tokens import load_tokens

# ── Load tokens once at import time ──────────────────────────────────────────
_tokens = load_tokens()
_colors: dict[str, str] = _tokens["colors"]
_badge: dict[str, str] = _tokens["badge"]

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

# ── Badge colors ─────────────────────────────────────────────────────────────
BADGE_BG_VINOUS: QColor = QColor(_badge["bg_vinous"])
BADGE_FG_VINOUS: QColor = QColor(_badge["fg_vinous"])
BADGE_BG_SAGE: QColor = QColor(_badge["bg_sage"])
BADGE_FG_SAGE: QColor = QColor(_badge["fg_sage"])
BADGE_BG_NAVY: QColor = QColor(_badge["bg_navy"])
BADGE_FG_NAVY: QColor = QColor(_badge["fg_navy"])
BADGE_BG_UMBER: QColor = QColor(_badge["bg_umber"])
BADGE_FG_UMBER: QColor = QColor(_badge["fg_umber"])
BADGE_BG_PLUM: QColor = QColor(_badge["bg_plum"])
BADGE_FG_PLUM: QColor = QColor(_badge["fg_plum"])
BADGE_BG_TEAL: QColor = QColor(_badge["bg_teal"])
BADGE_FG_TEAL: QColor = QColor(_badge["fg_teal"])
BADGE_BG_OLIVE: QColor = QColor(_badge["bg_olive"])
BADGE_FG_OLIVE: QColor = QColor(_badge["fg_olive"])
BADGE_BG_AZURE: QColor = QColor(_badge["bg_azure"])
BADGE_FG_AZURE: QColor = QColor(_badge["fg_azure"])
BADGE_BG_NEUTRAL: QColor = QColor(_badge["bg_neutral"])
BADGE_FG_NEUTRAL: QColor = QColor(_badge["fg_neutral"])

__all__ = [
    "AMBER_ACCENT",
    "AMBER_DARK",
    "AMBER_LIGHT",
    "BADGE_BG_AZURE",
    "BADGE_BG_NEUTRAL",
    "BADGE_BG_NAVY",
    "BADGE_BG_OLIVE",
    "BADGE_BG_PLUM",
    "BADGE_BG_SAGE",
    "BADGE_BG_TEAL",
    "BADGE_BG_UMBER",
    "BADGE_BG_VINOUS",
    "BADGE_FG_AZURE",
    "BADGE_FG_NEUTRAL",
    "BADGE_FG_NAVY",
    "BADGE_FG_OLIVE",
    "BADGE_FG_PLUM",
    "BADGE_FG_SAGE",
    "BADGE_FG_TEAL",
    "BADGE_FG_UMBER",
    "BADGE_FG_VINOUS",
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
