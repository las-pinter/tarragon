"""Tests for the QSS generator — deterministic output, token coverage, edge cases."""

from __future__ import annotations

from pathlib import Path
from typing import Any

import pytest

from tarragon.theme.qss_generator import generate_qss
from tarragon.theme.tokens import load_tokens

# ── Fixtures ────────────────────────────────────────────────────────────


@pytest.fixture()
def tokens() -> dict[str, Any]:
    """Return the real design tokens from tokens.json."""
    return load_tokens()


@pytest.fixture()
def minimal_tokens() -> dict[str, Any]:
    """Return the smallest valid token dict that generate_qss accepts."""
    return {
        "colors": {
            "bg_primary": "#000000",
            "bg_secondary": "#111111",
            "bg_tertiary": "#222222",
            "surface_highlight": "#333333",
            "surface_hover": "#444444",
            "coral_strong": "#F0997B",
            "coral_muted": "#D85A30",
            "coral_dark": "#4A1B0C",
            "amber_accent": "#FAC775",
            "amber_light": "#FFD480",
            "amber_dark": "#412402",
            "text_primary": "#ece9f2",
            "text_secondary": "#A09CA3",
            "text_tertiary": "#74707B",
            "text_muted": "#5E5B63",
            "bg_disabled": "#1a1820",
            "border_disabled": "#3a3845",
            "border_interactive": "rgba(255,255,255,0.12)",
            "bg_log_panel": "#1a1a2e",
        },
        "typography": {
            "font_family": "Arial, sans-serif",
            "mono_family": "Courier, monospace",
            "body_size": 14,
            "heading_size": 18,
            "small_size": 11,
            "log_size": 12,
            "weight_regular": 400,
            "weight_medium": 500,
            "weight_semibold": 600,
        },
        "spacing": {"xs": 2, "sm": 6, "md": 10, "lg": 14, "xl": 20},
        "radius": {"none": 0, "xs": 2, "sm": 4, "md": 6, "lg": 8, "xl": 10},
    }


# ── Happy-path tests ────────────────────────────────────────────────────


def test_generate_qss_returns_non_empty_string(tokens: dict[str, Any]) -> None:
    """generate_qss returns a non-empty string for real tokens."""
    qss = generate_qss(tokens)
    assert isinstance(qss, str)
    assert len(qss) > 0


def test_generate_qss_contains_expected_color_tokens(tokens: dict[str, Any]) -> None:
    """Key color values from tokens appear in the generated QSS."""
    qss = generate_qss(tokens)
    # These colors are all referenced in the QSS template.
    expected_colors = [
        "bg_primary",
        "bg_secondary",
        "bg_tertiary",
        "surface_highlight",
        "surface_hover",
        "coral_strong",
        "coral_muted",
        "coral_dark",
        "amber_accent",
        "amber_light",
        "amber_dark",
        "text_primary",
        "text_secondary",
        "text_muted",
        "bg_disabled",
        "border_disabled",
        "bg_log_panel",
    ]
    for key in expected_colors:
        color_value = tokens["colors"][key]
        assert color_value in qss, f"Color '{key}' ({color_value}) missing from generated QSS"


def test_generate_qss_contains_all_selectors(tokens: dict[str, Any]) -> None:
    """The generated QSS contains all expected Qt widget selectors."""
    qss = generate_qss(tokens)
    expected_selectors = [
        "QMainWindow",
        "QDockWidget",
        "QPushButton",
        "QLabel",
        "QLineEdit",
        "QScrollBar:vertical",
        "QScrollBar:horizontal",
        "QToolBar",
        "QStatusBar",
        "QListView",
        "QTreeView",
        "QLabel#sidebarSectionHeader",
        "QListView#sidebarFavorites",
        "QTreeView#sidebarFolderTree",
        "QGroupBox",
        "QCheckBox",
        "QRadioButton",
        "QComboBox",
        "QScrollArea",
        "QPlainTextEdit#logText",
    ]
    for selector in expected_selectors:
        assert selector in qss, f"Selector '{selector}' missing from generated QSS"


def test_generate_qss_contains_pseudo_states(tokens: dict[str, Any]) -> None:
    """The generated QSS includes hover, pressed, focus, selected, and disabled states."""
    qss = generate_qss(tokens)
    for pseudo in [":hover", ":pressed", ":focus", ":selected", ":checked", ":disabled"]:
        assert pseudo in qss, f"Pseudo-state '{pseudo}' missing from generated QSS"


def test_generate_qss_matches_original_app_qss(tokens: dict[str, Any]) -> None:
    """Generated QSS is identical to the reference app.qss file (ignoring deprecation header)."""
    qss = generate_qss(tokens)
    qss_path = Path(__file__).resolve().parent.parent / "src" / "tarragon" / "theme" / "app.qss"
    original = qss_path.read_text(encoding="utf-8")
    # Strip the deprecation header if present — it is not part of generated QSS.
    if original.startswith("/* DEPRECATED:"):
        original = original.split("*/\n", 1)[1].lstrip("\n")
    assert qss == original, "Generated QSS must match the reference app.qss exactly"


def test_generate_qss_no_gradients(tokens: dict[str, Any]) -> None:
    """Generated QSS does not contain gradients (design constraint)."""
    qss = generate_qss(tokens).lower()
    assert "qgradient" not in qss
    assert "qdrawsvg" not in qss


# ── Determinism tests ───────────────────────────────────────────────────


def test_generate_qss_is_deterministic(tokens: dict[str, Any]) -> None:
    """Calling generate_qss twice with the same tokens produces identical output."""
    qss1 = generate_qss(tokens)
    qss2 = generate_qss(tokens)
    assert qss1 == qss2, "generate_qss must be deterministic"


def test_generate_qss_different_tokens_produce_different_qss(
    tokens: dict[str, Any],
    minimal_tokens: dict[str, Any],
) -> None:
    """Different token values produce different QSS output."""
    qss_real = generate_qss(tokens)
    qss_minimal = generate_qss(minimal_tokens)
    assert qss_real != qss_minimal


# ── Minimal / custom tokens tests ───────────────────────────────────────


def test_generate_qss_with_custom_tokens(minimal_tokens: dict[str, Any]) -> None:
    """generate_qss works with custom (non-default) token values."""
    qss = generate_qss(minimal_tokens)
    assert isinstance(qss, str)
    assert len(qss) > 0
    # Verify custom values appear in output
    assert "#000000" in qss  # bg_primary from minimal tokens
    assert "14px" in qss  # body_size from minimal tokens
    assert "Arial" in qss  # font_family from minimal tokens


def test_generate_qss_uses_spacing_tokens(minimal_tokens: dict[str, Any]) -> None:
    """Spacing values from tokens appear correctly in the generated QSS."""
    qss = generate_qss(minimal_tokens)
    # xs=2, sm=6, md=10, lg=14 should all appear
    assert "2px" in qss  # xs
    assert "6px" in qss  # sm
    assert "10px" in qss  # md
    assert "14px" in qss  # lg


def test_generate_qss_uses_radius_tokens(minimal_tokens: dict[str, Any]) -> None:
    """Radius values from tokens appear correctly in the generated QSS."""
    qss = generate_qss(minimal_tokens)
    # xs=2, sm=4, md=6, lg=8, xl=10 should appear as border-radius values
    assert "border-radius: 2px" in qss  # r_xs
    assert "border-radius: 4px" in qss  # r_sm
    assert "border-radius: 6px" in qss  # r_md
    assert "border-radius: 10px" in qss  # r_xl


# ── Edge-case tests ─────────────────────────────────────────────────────


def test_generate_qss_missing_color_key_raises() -> None:
    """generate_qss raises KeyError when a required color token is missing."""
    bad_tokens = load_tokens()
    del bad_tokens["colors"]["bg_primary"]
    with pytest.raises(KeyError):
        generate_qss(bad_tokens)


def test_generate_qss_missing_spacing_key_raises() -> None:
    """generate_qss raises KeyError when a required spacing token is missing."""
    bad_tokens = load_tokens()
    del bad_tokens["spacing"]["xs"]
    with pytest.raises(KeyError):
        generate_qss(bad_tokens)


def test_generate_qss_missing_typography_key_raises() -> None:
    """generate_qss raises KeyError when a required typography token is missing."""
    bad_tokens = load_tokens()
    del bad_tokens["typography"]["body_size"]
    with pytest.raises(KeyError):
        generate_qss(bad_tokens)


def test_generate_qss_missing_radius_key_raises() -> None:
    """generate_qss raises KeyError when a required radius token is missing."""
    bad_tokens = load_tokens()
    del bad_tokens["radius"]["sm"]
    with pytest.raises(KeyError):
        generate_qss(bad_tokens)


# ── Integration with ThemeLoader ────────────────────────────────────────


def test_theme_loader_load_qss_uses_generator() -> None:
    """ThemeLoader.load_qss() returns generated QSS (not a static file)."""
    from tarragon.theme.loader import ThemeLoader

    loader = ThemeLoader()
    qss_from_loader = loader.load_qss()
    qss_from_generator = generate_qss(load_tokens())
    assert qss_from_loader == qss_from_generator


def test_load_and_generate_qss_convenience_function() -> None:
    """load_and_generate_qss() returns the same QSS as the generator."""
    from tarragon.theme.loader import load_and_generate_qss

    qss = load_and_generate_qss()
    assert isinstance(qss, str)
    assert len(qss) > 0
    assert qss == generate_qss(load_tokens())


# ── Font-family formatting tests ────────────────────────────────────────


def test_format_font_family_quotes_multi_word() -> None:
    """Multi-word font names are quoted; generic keywords are not."""
    from tarragon.theme.qss_generator import _format_font_family

    result = _format_font_family("Segoe UI, sans-serif")
    assert result == '"Segoe UI", sans-serif'


def test_format_font_family_quotes_primary_mono() -> None:
    """Primary mono font is quoted; 'monospace' keyword is not."""
    from tarragon.theme.qss_generator import _format_font_family

    result = _format_font_family("Consolas, Courier New, monospace")
    assert result == '"Consolas", "Courier New", monospace'


def test_format_font_family_system_font_unquoted() -> None:
    """System font identifiers like -apple-system are not quoted."""
    from tarragon.theme.qss_generator import _format_font_family

    result = _format_font_family("Segoe UI, -apple-system, sans-serif")
    assert result == '"Segoe UI", -apple-system, sans-serif'
