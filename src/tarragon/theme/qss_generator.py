"""QSS generator — builds the application stylesheet from design tokens.

Reads ``tokens.json`` (via :func:`~tarragon.theme.tokens.load_tokens`) and
produces a deterministic QSS string that is functionally identical to the
hand-written ``app.qss``.  Changing token values in ``tokens.json`` and
re-running the generator updates the entire theme.

Example::

    from tarragon.theme.qss_generator import generate_qss
    from tarragon.theme.tokens import load_tokens

    qss = generate_qss(load_tokens())
    app.setStyleSheet(qss)
"""

from __future__ import annotations

from typing import Any

#: CSS generic font family keywords and system font identifiers — must NOT be quoted in QSS/CSS.
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


def generate_qss(tokens: dict[str, Any]) -> str:
    """Generate a complete QSS stylesheet from design tokens.

    The output is deterministic: the same *tokens* dictionary always produces
    the same QSS string.  All hardcoded colour, spacing, typography, and
    border-radius values are resolved from the token dictionaries.

    Args:
        tokens: Parsed ``tokens.json`` dictionary.  Must contain the keys
            ``colors``, ``typography``, ``spacing``, and ``radius``.

    Returns:
        A complete QSS stylesheet string ready for ``QApplication.setStyleSheet``.
    """
    c = tokens["colors"]
    t = tokens["typography"]
    s = tokens["spacing"]
    r = tokens["radius"]

    # Pre-format the font-family strings for QSS.
    # The primary font name is always quoted; generic fallbacks are not.
    # We construct QSS-safe font strings from the token values: the first (primary)
    # font is quoted, remaining names are passed through as-is.
    ui_font = _format_font_family(str(t["font_family"]))
    mono_font = _format_font_family(str(t["mono_family"]))

    body_px = int(t["body_size"])
    small_px = int(t["small_size"])
    log_px = int(t["log_size"])
    weight_semibold = int(t["weight_semibold"])

    xs = int(s["xs"])
    sm = int(s["sm"])
    md = int(s["md"])
    lg = int(s["lg"])

    r_xs = int(r["xs"])
    r_sm = int(r["sm"])
    r_md = int(r["md"])
    r_lg = int(r["lg"])

    # Colour shortcuts for readability.
    bg_primary = c["bg_primary"]
    bg_secondary = c["bg_secondary"]
    bg_tertiary = c["bg_tertiary"]
    surface_highlight = c["surface_highlight"]
    surface_hover = c["surface_hover"]
    coral_strong = c["coral_strong"]
    coral_muted = c["coral_muted"]
    coral_dark = c["coral_dark"]
    amber_accent = c["amber_accent"]
    amber_light = c["amber_light"]
    amber_dark = c["amber_dark"]
    text_primary = c["text_primary"]
    text_secondary = c["text_secondary"]
    bg_disabled = c["bg_disabled"]
    border_disabled = c["border_disabled"]
    bg_log_panel = c["bg_log_panel"]

    return f"""\
/* Tarragon Theme — Dark Coral-Amber Aesthetic */
/* Generated from tokens.json: colors, typography, spacing, radius, layout */

/* ── Base window background ─────────────────────────────────────── */
QMainWindow {{
    background-color: {bg_primary};
}}

/* ── Dock widgets (Library, Gallery, Preview) ───────────────────── */
QDockWidget {{
    background-color: {bg_secondary};
    color: {text_primary};
    font-family: {ui_font};
    font-size: {body_px}px;
}}

QDockWidget::title {{
    background-color: {bg_tertiary};
    color: {amber_accent};
    font-weight: {weight_semibold};
    font-size: {body_px}px;
    padding-left: {sm}px;
    padding-right: {sm}px;
}}

QDockWidget::close-button,
QDockWidget::float-button {{
    background-color: transparent;
    border: none;
}}

QDockWidget::close-button:hover,
QDockWidget::float-button:hover {{
    background-color: {surface_highlight};
}}

/* ── Buttons (toolbar, action buttons) ──────────────────────────── */
QPushButton {{
    background-color: {bg_tertiary};
    color: {text_primary};
    font-family: {ui_font};
    font-size: {body_px}px;
    border: none;
    border-radius: {r_sm}px;
    padding-left: {md}px;
    padding-right: {md}px;
    padding-top: {xs + 2}px;
    padding-bottom: {xs + 2}px;
}}

QPushButton:hover {{
    background-color: {surface_highlight};
    color: {amber_accent};
}}

QPushButton:pressed {{
    background-color: {coral_muted};
    color: {text_primary};
}}

/* ── Labels (static text) ───────────────────────────────────────── */
QLabel {{
    background-color: transparent;
    color: {text_primary};
    font-family: {ui_font};
    font-size: {body_px}px;
}}

/* ── Tag pills ──────────────────────────────────────────────────── */
QLabel[tagRole="primary"] {{
    background-color: {coral_dark};  /* dark tinted bg */
    color: {coral_strong};  /* coral text */
    padding: {xs - 1}px {sm}px;
    border-radius: {r_lg}px;
    font-size: {small_px}px;
}}

QLabel[tagRole="secondary"] {{
    background-color: {amber_dark};  /* dark tinted bg */
    color: {amber_accent};  /* amber text */
    padding: {xs - 1}px {sm}px;
    border-radius: {r_lg}px;
    font-size: {small_px}px;
}}

/* ── Line edits (search boxes, text input) ──────────────────────── */
QLineEdit {{
    background-color: {bg_tertiary};
    color: {text_primary};
    font-family: {ui_font};
    font-size: {body_px}px;
    border: none;
    border-radius: {r_sm}px;
    padding-left: {sm}px;
    padding-right: {sm}px;
    padding-top: {xs}px;
    padding-bottom: {xs}px;
}}

QLineEdit:hover {{
    background-color: {surface_highlight};
}}

QLineEdit:focus {{
    border: 1px solid {coral_strong};
}}

/* ── Scroll bars (gallery thumbnails) ───────────────────────────── */
QScrollBar:vertical {{
    background-color: {bg_secondary};
    width: {sm}px;
    margin: 0px;
}}

QScrollBar::handle:vertical {{
    background-color: {bg_tertiary};
    border-radius: {r_md}px;
    min-height: 20px;
}}

QScrollBar::handle:vertical:hover {{
    background-color: {coral_muted};
}}

QScrollBar::add-line:vertical,
QScrollBar::sub-line:vertical {{
    height: 0px;
}}

QScrollBar:horizontal {{
    background-color: {bg_secondary};
    height: {sm}px;
    margin: 0px;
}}

QScrollBar::handle:horizontal {{
    background-color: {bg_tertiary};
    border-radius: {r_md}px;
    min-width: 20px;
}}

QScrollBar::handle:horizontal:hover {{
    background-color: {coral_muted};
}}

QScrollBar::add-line:horizontal,
QScrollBar::sub-line:horizontal {{
    width: 0px;
}}

/* ── Tool bars ──────────────────────────────────────────────────── */
QToolBar {{
    background-color: {bg_secondary};
    border-bottom: 1px solid {bg_tertiary};
    spacing: {xs}px;
    padding: {xs}px;
}}

/* ── Status bar ─────────────────────────────────────────────────── */
QStatusBar {{
    background-color: {bg_secondary};
    color: {text_secondary};
    font-family: {ui_font};
    font-size: {small_px}px;
    border-top: 1px solid {bg_tertiary};
}}

/* ── List / Tree views (library panel) ──────────────────────────── */
QListView,
QTreeView {{
    background-color: {bg_secondary};
    color: {text_primary};
    font-family: {ui_font};
    font-size: {body_px}px;
    border: none;
    outline: none;
}}

QListView::item:hover,
QTreeView::item:hover {{
    background-color: {surface_highlight};
    color: {amber_accent};
}}

QListView::item:selected,
QTreeView::item:selected {{
    background-color: {coral_muted};
    color: {text_primary};
}}

/* ── Group boxes (if used in panels) ────────────────────────────── */
QGroupBox {{
    background-color: transparent;
    border: 1px solid {bg_tertiary};
    border-radius: {r_sm}px;
    margin-top: {md}px;
    padding-top: {lg}px;
    color: {text_primary};
}}

QGroupBox::title {{
    subcontrol-origin: margin;
    left: {sm}px;
    top: -{xs}px;
    color: {amber_accent};
}}

/* ── Checkboxes and radio buttons ───────────────────────────────── */
QCheckBox,
QRadioButton {{
    background-color: transparent;
    color: {text_primary};
    font-family: {ui_font};
    font-size: {body_px}px;
}}

QCheckBox::indicator {{
    width: 18px;
    height: 18px;
    border: 2px solid {amber_accent};
    border-radius: {r_xs}px;
    background-color: {bg_tertiary};
}}

QCheckBox::indicator:hover {{
    border-color: {amber_light};
    background-color: {surface_hover};
}}

QCheckBox::indicator:checked {{
    background-color: {coral_muted};
    border-color: {amber_accent};
    image: none;
}}

QCheckBox::indicator:checked:hover {{
    background-color: #E06540;
}}

QCheckBox::indicator:disabled {{
    background-color: {bg_disabled};
    border-color: {border_disabled};
}}

QGroupBox::indicator {{
    width: 18px;
    height: 18px;
    border: 2px solid {amber_accent};
    border-radius: {r_xs}px;
    background-color: {bg_tertiary};
}}

QGroupBox::indicator:hover {{
    border-color: {amber_light};
    background-color: {surface_hover};
}}

QGroupBox::indicator:checked {{
    background-color: {coral_muted};
    border-color: {amber_accent};
    image: none;
}}

QGroupBox::indicator:checked:hover {{
    background-color: #E06540;
}}

QGroupBox::indicator:disabled {{
    background-color: {bg_disabled};
    border-color: {border_disabled};
}}

/* ── Combo boxes ─────────────────────────────────────────────────── */
QComboBox {{
    background-color: {bg_tertiary};
    color: {text_primary};
    font-family: {ui_font};
    font-size: {body_px}px;
    border: none;
    border-radius: {r_sm}px;
    padding-left: {sm}px;
    padding-right: {sm}px;
}}

QComboBox:hover {{
    background-color: {surface_highlight};
}}

QComboBox::drop-down {{
    border: none;
    width: {lg}px;
}}

/* ── Scroll area backgrounds ────────────────────────────────────── */
QScrollArea {{
    border: none;
    background-color: transparent;
}}

/* ── Log Panel ─────────────────────────────────────────────────── */
QPlainTextEdit#logText {{
    background-color: {bg_log_panel};
    color: {text_primary};
    font-family: {mono_font};
    font-size: {log_px}px;
    border: 1px solid {surface_highlight};
    border-radius: {r_md}px;
    padding: {xs}px;
    selection-background-color: {coral_muted};
    selection-color: {text_primary};
}}
"""


__all__ = ["generate_qss"]
