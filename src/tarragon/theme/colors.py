"""Typed QColor constants and palette."""

from __future__ import annotations

from PySide6.QtGui import QColor, QPalette

# Background colors
BG_PRIMARY: QColor = QColor("#16151A")
BG_SECONDARY: QColor = QColor("#1c1b22")
BG_TERTIARY: QColor = QColor("#211f29")
SURFACE_HIGHLIGHT: QColor = QColor("#3D3B44")

# Accent colors
CORAL_STRONG: QColor = QColor("#F0997B")
CORAL_MUTED: QColor = QColor("#D85A30")
CORAL_DARK: QColor = QColor("#4A1B0C")
CORAL_BRIGHT: QColor = QColor("#E06540")
AMBER_ACCENT: QColor = QColor("#FAC775")
AMBER_LIGHT: QColor = QColor("#FFD480")
AMBER_DARK: QColor = QColor("#412402")

# Surface interaction colors
SURFACE_HOVER: QColor = QColor("#2a2836")

# Text colors
TEXT_PRIMARY: QColor = QColor("#ece9f2")
TEXT_SECONDARY: QColor = QColor("#8d8a98")
TEXT_TERTIARY: QColor = QColor("#a8a5b0")
TEXT_MUTED: QColor = QColor("#65626f")

# Border colors
BORDER_SUBTLE: QColor = QColor("rgba(255,255,255,0.06)")
BORDER_CARD: QColor = QColor("rgba(255,255,255,0.08)")
BORDER_INTERACTIVE: QColor = QColor("rgba(255,255,255,0.12)")

# Misc colors
BG_DISABLED: QColor = QColor("#1a1820")
BORDER_DISABLED: QColor = QColor("#3a3845")
BG_LOG_PANEL: QColor = QColor("#1a1a2e")
HIGHLIGHT_DISABLED: QColor = QColor("#4A3A35")
SEPARATOR: QColor = QColor("#1E1D23")

# Badge colors
BADGE_BG_VINOUS: QColor = QColor("#4A1B0C")
BADGE_FG_VINOUS: QColor = QColor("#F0997B")
BADGE_BG_SAGE: QColor = QColor("#1A3A2A")
BADGE_FG_SAGE: QColor = QColor("#7BC88F")
BADGE_BG_NAVY: QColor = QColor("#1A2A3A")
BADGE_FG_NAVY: QColor = QColor("#7BA8C8")
BADGE_BG_UMBER: QColor = QColor("#3A2A1A")
BADGE_FG_UMBER: QColor = QColor("#C8A87B")
BADGE_BG_PLUM: QColor = QColor("#2A1A3A")
BADGE_FG_PLUM: QColor = QColor("#A87BC8")
BADGE_BG_TEAL: QColor = QColor("#1A3A3A")
BADGE_FG_TEAL: QColor = QColor("#7BC8C8")
BADGE_BG_OLIVE: QColor = QColor("#3A3A1A")
BADGE_FG_OLIVE: QColor = QColor("#C8C87B")
BADGE_BG_AZURE: QColor = QColor("#2E86AB")
BADGE_FG_AZURE: QColor = QColor("#D4EEF7")
BADGE_BG_NEUTRAL: QColor = QColor("#2A2A2A")
BADGE_FG_NEUTRAL: QColor = QColor("#A8A5B0")


def create_palette() -> QPalette:
    palette = QPalette()
    palette.setColor(QPalette.ColorRole.Window, BG_PRIMARY)
    palette.setColor(QPalette.ColorRole.WindowText, TEXT_PRIMARY)
    palette.setColor(QPalette.ColorRole.Base, BG_SECONDARY)
    palette.setColor(QPalette.ColorRole.AlternateBase, BG_TERTIARY)
    palette.setColor(QPalette.ColorRole.ToolTipBase, BG_TERTIARY)
    palette.setColor(QPalette.ColorRole.ToolTipText, TEXT_PRIMARY)
    palette.setColor(QPalette.ColorRole.Text, TEXT_PRIMARY)
    palette.setColor(QPalette.ColorRole.Button, BG_TERTIARY)
    palette.setColor(QPalette.ColorRole.ButtonText, TEXT_PRIMARY)
    palette.setColor(QPalette.ColorRole.BrightText, CORAL_STRONG)
    palette.setColor(QPalette.ColorRole.Highlight, CORAL_MUTED)
    palette.setColor(QPalette.ColorRole.HighlightedText, TEXT_PRIMARY)
    palette.setColor(QPalette.ColorRole.Link, AMBER_ACCENT)

    # Disabled color group for disabled widgets
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.WindowText, TEXT_TERTIARY)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.Text, TEXT_TERTIARY)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.ButtonText, TEXT_TERTIARY)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.HighlightedText, TEXT_TERTIARY)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.Highlight, HIGHLIGHT_DISABLED)

    # Additional roles for completeness
    palette.setColor(QPalette.ColorRole.Mid, SEPARATOR)
    palette.setColor(QPalette.ColorRole.LinkVisited, TEXT_SECONDARY)
    palette.setColor(QPalette.ColorRole.PlaceholderText, TEXT_TERTIARY)

    return palette
