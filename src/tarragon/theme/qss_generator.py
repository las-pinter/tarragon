"""Generator for the application stylesheet."""

from __future__ import annotations

import tarragon.theme.colors as colors
import tarragon.theme.constants as constants
import tarragon.theme.typography as typography

# CSS generic font family keywords and system font identifiers
_CSS_GENERIC_FAMILIES = frozenset(
    {
        "serif",
        "sans-serif",
        "cursive",
        "fantasy",
        "monospace",
        "-apple-system",
        "system-ui",
    }
)


def _format_font_family(font_family: str) -> str:
    """Format a comma-separated font family string for QSS.

    Each font name is quoted unless it is a CSS generic family keyword
    (``serif``, ``sans-serif``, ``monospace``, etc.), matching standard
    CSS/QSS convention.

    Args:
        font_family: Comma-separated font names (e.g. ``"Segoe UI, sans-serif"``).

    Returns:
        QSS-formatted string (e.g. ``'"Segoe UI", sans-serif'``).
    """
    parts: list[str] = []
    for name in font_family.split(","):
        stripped = name.strip()
        if stripped.lower() in _CSS_GENERIC_FAMILIES:
            parts.append(stripped)
        else:
            parts.append(f'"{stripped}"')
    return ", ".join(parts)


def generate_qss() -> str:
    """Generate a complete QSS stylesheet from design tokens.

    Returns:
        A complete QSS stylesheet string ready for ``QApplication.setStyleSheet``.
    """

    return f"""\
/* Tarragon Theme, Dark Coral-Amber Aesthetic */

/* -------------------------------------------------------------------------------- */
/* Base window background */
/* -------------------------------------------------------------------------------- */
QMainWindow {{
    background-color: hsla{colors.BG_PRIMARY.getHsl()};
}}

/* Dock widgets (Library, Gallery, Preview) */
QDockWidget {{
    background-color: hsla{colors.BG_SECONDARY.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
    font-family: {_format_font_family(typography.FONT_FAMILY)};
    font-size: {typography.BODY_SIZE}px;
}}

QDockWidget::title {{
    background-color: hsla{colors.BG_TERTIARY.getHsl()};
    color: hsla{colors.AMBER_ACCENT.getHsl()};
    font-weight: {typography.WEIGHT_SEMIBOLD};
    font-size: {typography.BODY_SIZE}px;
    padding-left: {constants.SPACING_S}px;
    padding-right: {constants.SPACING_S}px;
}}

QDockWidget::close-button,
QDockWidget::float-button {{
    background-color: transparent;
    border: none;
}}

QDockWidget::close-button:hover,
QDockWidget::float-button:hover {{
    background-color: hsla{colors.SURFACE_HIGHLIGHT.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/*  Buttons */
/* -------------------------------------------------------------------------------- */
QPushButton {{
    background-color: hsla{colors.BG_TERTIARY.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
    font-family: {_format_font_family(typography.FONT_FAMILY)};
    font-size: {typography.BODY_SIZE}px;
    border: none;
    border-radius: {constants.RADIUS_S}px;
    padding-left: {constants.SPACING_M}px;
    padding-right: {constants.SPACING_M}px;
    padding-top: {constants.SPACING_XS + 2}px;
    padding-bottom: {constants.SPACING_XS + 2}px;
}}

QPushButton:hover {{
    background-color: hsla{colors.SURFACE_HIGHLIGHT.getHsl()};
    color: hsla{colors.AMBER_ACCENT.getHsl()};
}}

QPushButton:pressed {{
    background-color: hsla{colors.CORAL_MUTED.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Labels  */
/* -------------------------------------------------------------------------------- */
QLabel {{
    background-color: transparent;
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
    font-family: {_format_font_family(typography.FONT_FAMILY)};
    font-size: {typography.BODY_SIZE}px;
}}

/* Tag pills */
QLabel[tagRole="primary"] {{
    background-color: hsla{colors.CORAL_DARK.getHsl()};
    color: hsla{colors.CORAL_STRONG.getHsl()};
    padding: {constants.SPACING_XS - 1}px {constants.SPACING_S}px;
    border-radius: {constants.RADIUS_XL}px;
    font-size: {typography.SMALL_SIZE}px;
}}

QLabel[tagRole="secondary"] {{
    background-color: hsla{colors.AMBER_DARK.getHsl()};
    color: hsla{colors.AMBER_ACCENT.getHsl()};
    padding: {constants.SPACING_XS - 1}px {constants.SPACING_S}px;
    border-radius: {constants.RADIUS_XL}px;
    font-size: {typography.SMALL_SIZE}px;
}}

/* Line edits */
QLineEdit {{
    background-color: hsla{colors.BG_SECONDARY.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
    font-family: {_format_font_family(typography.FONT_FAMILY)};
    font-size: {typography.CAPTION_SIZE}px;
    border: none;
    border-radius: {constants.RADIUS_M}px;
    padding-left: 28px;
    padding-right: {constants.SPACING_S}px;
    padding-top: {constants.SPACING_XS}px;
    padding-bottom: {constants.SPACING_XS}px;
}}

QLineEdit:hover {{
    background-color: hsla{colors.SURFACE_HIGHLIGHT.getHsl()};
}}

QLineEdit:focus {{
    border: 1px solid hsla{colors.CORAL_STRONG.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Scroll bars (gallery thumbnails) */
/* -------------------------------------------------------------------------------- */
QScrollBar:vertical {{
    background-color: hsla{colors.BG_SECONDARY.getHsl()};
    width: {constants.SPACING_S}px;
    margin: 0px;
}}

QScrollBar::handle:vertical {{
    background-color: hsla{colors.BG_TERTIARY.getHsl()};
    border-radius: {constants.RADIUS_M}px;
    min-height: 20px;
}}

QScrollBar::handle:vertical:hover {{
    background-color: hsla{colors.CORAL_MUTED.getHsl()};
}}

QScrollBar::add-line:vertical,
QScrollBar::sub-line:vertical {{
    height: 0px;
}}

QScrollBar:horizontal {{
    background-color: hsla{colors.BG_SECONDARY.getHsl()};
    height: {constants.SPACING_S}px;
    margin: 0px;
}}

QScrollBar::handle:horizontal {{
    background-color: hsla{colors.BG_TERTIARY.getHsl()};
    border-radius: {constants.RADIUS_M}px;
    min-width: 20px;
}}

QScrollBar::handle:horizontal:hover {{
    background-color: hsla{colors.CORAL_MUTED.getHsl()};
}}

QScrollBar::add-line:horizontal,
QScrollBar::sub-line:horizontal {{
    width: 0px;
}}

/* -------------------------------------------------------------------------------- */
/* Tool bars */
/* -------------------------------------------------------------------------------- */
QToolBar {{
    background-color: hsla{colors.BG_SECONDARY.getHsl()};
    border-bottom: 1px solid hsla{colors.BG_TERTIARY.getHsl()};
    spacing: {constants.SPACING_XS}px;
    padding: {constants.SPACING_XS}px;
}}

/* Status bar */
QStatusBar {{
    background-color: hsla{colors.BG_SECONDARY.getHsl()};
    color: hsla{colors.TEXT_SECONDARY.getHsl()};
    font-family: {_format_font_family(typography.FONT_FAMILY)};
    font-size: {typography.SMALL_SIZE}px;
    border-top: 1px solid hsla{colors.BG_TERTIARY.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* List, Tree views */
/* -------------------------------------------------------------------------------- */
QListView,
QTreeView {{
    background-color: hsla{colors.BG_SECONDARY.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
    font-family: {_format_font_family(typography.FONT_FAMILY)};
    font-size: {typography.BODY_SIZE}px;
    border: none;
    outline: none;
}}

QListView::item:hover,
QTreeView::item:hover {{
    background-color: hsla{colors.SURFACE_HIGHLIGHT.getHsl()};
    color: hsla{colors.AMBER_ACCENT.getHsl()};
}}

QListView::item:selected,
QTreeView::item:selected {{
    background-color: hsla{colors.CORAL_MUTED.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Sidebar section headers */
/* -------------------------------------------------------------------------------- */
QLabel#sidebarSectionHeader {{
    color: hsla{colors.TEXT_MUTED.getHsl()};
    font-size: {typography.CAPTION_SIZE}px;
    margin: 0px 6px 6px 6px;
}}

/* -------------------------------------------------------------------------------- */
/* Sidebar favorites list */
/* -------------------------------------------------------------------------------- */
QListView#sidebarFavorites {{
    background-color: transparent;
    color: hsla{colors.TEXT_SECONDARY.getHsl()};
    font-size: {typography.BODY_SIZE}px;
}}

QListView#sidebarFavorites::item {{
    padding: 5px 6px;
    border-radius: {constants.RADIUS_M}px;
    color: hsla{colors.TEXT_SECONDARY.getHsl()};
}}

QListView#sidebarFavorites::item:hover {{
    background-color: hsla{colors.SURFACE_HOVER.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
}}

QListView#sidebarFavorites::item:selected {{
    background-color: hsla{colors.BG_TERTIARY.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Sidebar folder tree */
/* -------------------------------------------------------------------------------- */
QTreeView#sidebarFolderTree {{
    background-color: transparent;
    color: hsla{colors.TEXT_SECONDARY.getHsl()};
    font-size: {typography.BODY_SIZE}px;
}}

QTreeView#sidebarFolderTree::item {{
    color: hsla{colors.TEXT_SECONDARY.getHsl()};
}}

QTreeView#sidebarFolderTree::item:hover {{
    background-color: hsla{colors.SURFACE_HOVER.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
}}

QTreeView#sidebarFolderTree::item:selected {{
    background-color: hsla{colors.BG_TERTIARY.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Group boxes */
/* -------------------------------------------------------------------------------- */
QGroupBox {{
    background-color: transparent;
    border: 1px solid hsla{colors.BG_TERTIARY.getHsl()};
    border-radius: {constants.RADIUS_S}px;
    margin-top: {constants.SPACING_M}px;
    padding-top: {constants.SPACING_L}px;
    padding-left: {constants.SPACING_S}px;
    padding-right: {constants.SPACING_S}px;
    padding-bottom: {constants.SPACING_S}px;
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
}}

QGroupBox::title {{
    subcontrol-origin: margin;
    left: {constants.SPACING_S}px;
    top: -{constants.SPACING_XS}px;
    color: hsla{colors.AMBER_ACCENT.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Checkboxes and radio buttons */
/* -------------------------------------------------------------------------------- */
QCheckBox,
QRadioButton {{
    background-color: transparent;
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
    font-family: {_format_font_family(typography.FONT_FAMILY)};
    font-size: {typography.BODY_SIZE}px;
    padding: {constants.SPACING_XS}px;
}}

QCheckBox::indicator, QGroupBox::indicator {{
    width: 18px;
    height: 18px;
    border: 2px solid hsla{colors.AMBER_ACCENT.getHsl()};
    border-radius: {constants.RADIUS_XS}px;
    background-color: hsla{colors.BG_TERTIARY.getHsl()};
}}

QCheckBox::indicator:hover, QGroupBox::indicator:hover {{
    border-color: hsla{colors.AMBER_LIGHT.getHsl()};
    background-color: hsla{colors.SURFACE_HOVER.getHsl()};
}}

QCheckBox::indicator:checked, QGroupBox::indicator:checked {{
    background-color: hsla{colors.CORAL_MUTED.getHsl()};
    border-color: hsla{colors.AMBER_ACCENT.getHsl()};
    image: none;
}}

QCheckBox::indicator:checked:hover, QGroupBox::indicator:checked:hover {{
    background-color: hsla{colors.CORAL_BRIGHT.getHsl()};
}}

QCheckBox::indicator:disabled, QGroupBox::indicator:disabled {{
    background-color: hsla{colors.BG_DISABLED.getHsl()};
    border-color: hsla{colors.BORDER_DISABLED.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Combo boxes */
/* -------------------------------------------------------------------------------- */
QComboBox {{
    background-color: hsla{colors.BG_TERTIARY.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
    font-family: {_format_font_family(typography.FONT_FAMILY)};
    font-size: {typography.BODY_SIZE}px;
    border: none;
    border-radius: {constants.RADIUS_S}px;
    padding-left: {constants.SPACING_S}px;
    padding-right: {constants.SPACING_S}px;
}}

QComboBox:hover {{
    background-color: hsla{colors.SURFACE_HIGHLIGHT.getHsl()};
}}

QComboBox::drop-down {{
    border: none;
    width: {constants.SPACING_L}px;
}}

/* -------------------------------------------------------------------------------- */
/* Scroll area backgrounds */
/* -------------------------------------------------------------------------------- */
QScrollArea {{
    border: none;
    background-color: transparent;
}}

/* -------------------------------------------------------------------------------- */
/* Log Panel */
/* -------------------------------------------------------------------------------- */
QPlainTextEdit#logText {{
    background-color: hsla{colors.BG_LOG_PANEL.getHsl()};
    color: hsla{colors.TEXT_PRIMARY.getHsl()};
    font-family: {_format_font_family(typography.MONO_FAMILY)};
    font-size: {typography.LOG_SIZE}px;
    border: 1px solid hsla{colors.SURFACE_HIGHLIGHT.getHsl()};
    border-radius: {constants.RADIUS_M}px;
    padding: {constants.SPACING_XS}px;
    selection-background-color: hsla{colors.CORAL_MUTED.getHsl()};
    selection-color: hsla{colors.TEXT_PRIMARY.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Tab widgets */
/* -------------------------------------------------------------------------------- */
QTabWidget::pane {{
    border: 1px solid hsla{colors.BG_TERTIARY.getHsl()};
    border-radius: {constants.RADIUS_S}px;
    padding: {constants.SPACING_S}px;
}}

QTabBar::tab {{
    background-color: hsla{colors.BG_TERTIARY.getHsl()};
    color: hsla{colors.TEXT_SECONDARY.getHsl()};
    font-family: {_format_font_family(typography.FONT_FAMILY)};
    font-size: {typography.BODY_SIZE}px;
    padding: {constants.SPACING_XS}px {constants.SPACING_M}px;
    border: none;
}}

QTabBar::tab:selected {{
    background-color: hsla{colors.BG_SECONDARY.getHsl()};
    color: hsla{colors.AMBER_ACCENT.getHsl()};
}}

QTabBar::tab:hover {{
    background-color: hsla{colors.SURFACE_HIGHLIGHT.getHsl()};
    color: hsla{colors.AMBER_ACCENT.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Gallery info bar */
/* -------------------------------------------------------------------------------- */
QLabel#galleryInfoLabel {{
    color: hsla{colors.TEXT_MUTED.getHsl()};
    font-size: {typography.CAPTION_SIZE}px;
}}

QLabel#galleryActiveFiltersPill {{
    background-color: hsla{colors.AMBER_DARK.getHsl()};
    color: hsla{colors.AMBER_ACCENT.getHsl()};
    padding: 3px 8px;
    border-radius: 6px;
    font-size: {typography.CAPTION_SIZE}px;
}}

/* -------------------------------------------------------------------------------- */
/* Preview panel background */
/* -------------------------------------------------------------------------------- */
QWidget#previewPanel {{
    background-color: hsla{colors.BG_PRIMARY.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Preview panel section headers */
/* -------------------------------------------------------------------------------- */
QLabel#previewSectionHeader {{
    color: hsla{colors.TEXT_MUTED.getHsl()};
    font-size: {typography.CAPTION_SIZE}px;
}}

/* -------------------------------------------------------------------------------- */
/* Preview panel metadata */
/* -------------------------------------------------------------------------------- */
QLabel#previewMetaLabel {{
    color: hsla{colors.TEXT_MUTED.getHsl()};
    font-size: {typography.CAPTION_SIZE}px;
}}

QLabel#previewMetaValue {{
    color: hsla{colors.TEXT_TERTIARY.getHsl()};
    font-size: {typography.CAPTION_SIZE}px;
}}

/* -------------------------------------------------------------------------------- */
/* Preview panel add-tag button */
/* -------------------------------------------------------------------------------- */
QPushButton#previewAddTagBtn {{
    background-color: transparent;
    color: hsla{colors.TEXT_MUTED.getHsl()};
    border: 1px solid hsla{colors.BORDER_INTERACTIVE.getHsl()};
    border-radius: {constants.RADIUS_XL}px;
    padding: 3px 8px;
    font-size: {typography.CAPTION_SIZE}px;
}}

QPushButton#previewAddTagBtn:hover {{
    background-color: hsla{colors.SURFACE_HIGHLIGHT.getHsl()};
    color: hsla{colors.TEXT_SECONDARY.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Color square buttons (preview panel) */
/* -------------------------------------------------------------------------------- */
QPushButton[colorSquare="true"] {{
    border: none;
    border-radius: 4px;
}}

QPushButton[colorSquare="true"]:hover {{
    border: 1px solid hsla{colors.TEXT_PRIMARY.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Tag pill remove button */
/* -------------------------------------------------------------------------------- */
QPushButton#tagPillRemoveBtn {{
    color: hsla{colors.CORAL_MUTED.getHsl()};
    border: none;
    background: transparent;
    font-weight: bold;
    padding: 0;
    font-size: {typography.SMALL_SIZE}px;
}}

QPushButton#tagPillRemoveBtn:hover {{
    color: hsla{colors.CORAL_STRONG.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Filter chips */
/* -------------------------------------------------------------------------------- */
QFrame#filterChip {{
    background-color: hsla{colors.SURFACE_HOVER.getHsl()};
    border: 1px solid hsla{colors.AMBER_ACCENT.getHsl()};
    border-radius: {constants.RADIUS_XL}px;
    padding: 2px 6px;
}}

QLabel#filterChipLabel {{
    color: hsla{colors.AMBER_ACCENT.getHsl()};
    border: none;
    background: transparent;
}}

QPushButton#filterChipRemoveBtn {{
    color: hsla{colors.CORAL_MUTED.getHsl()};
    border: none;
    background: transparent;
    font-weight: bold;
    padding: 0;
    font-size: {typography.SMALL_SIZE}px;
}}

QPushButton#filterChipRemoveBtn:hover {{
    color: hsla{colors.CORAL_STRONG.getHsl()};
}}

/* -------------------------------------------------------------------------------- */
/* Preview panel image label */
/* -------------------------------------------------------------------------------- */
QLabel#previewImageLabel {{
    background-color: hsla{colors.BG_SECONDARY.getHsl()};
    border: none;
    border-radius: {constants.RADIUS_L}px;
    color: hsla{colors.TEXT_SECONDARY.getHsl()};
    font-size: {typography.BODY_SIZE}px;
}}
"""
