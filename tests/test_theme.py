"""Tests for theme loading — tokens.json validity, QSS parsing, and MainWindow integration."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

# ── Fixtures ───────────────────────────────────────────────────────────


@pytest.fixture(autouse=True)
def qapp():
    """Provide a shared QApplication instance for all Qt tests."""
    from PySide6.QtWidgets import QApplication

    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


# ── Tokens JSON Tests ──────────────────────────────────────────────────


def test_tokens_json_is_valid():
    """tokens.json can be parsed as valid JSON."""
    tokens_path = Path(__file__).resolve().parent.parent / "src" / "tarragon" / "theme" / "tokens.json"
    with open(tokens_path, encoding="utf-8") as fh:
        data = json.load(fh)
    assert isinstance(data, dict), "tokens.json must be a JSON object at the top level"


def test_tokens_json_has_all_sections():
    """tokens.json contains every required section from the design system spec."""
    tokens_path = Path(__file__).resolve().parent.parent / "src" / "tarragon" / "theme" / "tokens.json"
    with open(tokens_path, encoding="utf-8") as fh:
        data = json.load(fh)

    required_sections = {"colors", "typography", "spacing", "radius", "motion", "layout"}
    assert (
        set(data.keys()) == required_sections
    ), f"tokens.json sections {set(data.keys())} != expected {required_sections}"


def test_tokens_colors_section():
    """The colors section contains all expected color tokens."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    colors = tokens["colors"]
    expected_keys = {
        "bg_primary",
        "bg_secondary",
        "bg_tertiary",
        "surface_highlight",
        "coral_strong",
        "coral_muted",
        "amber_accent",
        "text_primary",
        "text_secondary",
        "text_tertiary",
    }
    assert set(colors.keys()) == expected_keys


def test_tokens_typography_section():
    """The typography section contains all expected font tokens."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    typos = tokens["typography"]
    expected_keys = {
        "font_family",
        "body_size",
        "heading_size",
        "small_size",
        "weight_regular",
        "weight_medium",
        "weight_semibold",
    }
    assert set(typos.keys()) == expected_keys


def test_tokens_spacing_section():
    """The spacing section contains xs, sm, md, lg, xl."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    spacing = tokens["spacing"]
    for key in ("xs", "sm", "md", "lg", "xl"):
        assert key in spacing, f"Missing spacing token: {key}"
        assert isinstance(spacing[key], int), f"Spacing value '{key}' must be an integer"


def test_tokens_radius_section():
    """The radius section contains none, sm, md, lg."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    radii = tokens["radius"]
    for key in ("none", "sm", "md", "lg"):
        assert key in radii, f"Missing radius token: {key}"


def test_tokens_motion_section():
    """The motion section contains duration and easing tokens."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    motion = tokens["motion"]
    for key in ("duration_fast", "duration_normal", "easing"):
        assert key in motion, f"Missing motion token: {key}"


def test_tokens_layout_section():
    """The layout section contains thumbnail_size, grid_gap, dock_width_min, preview_height_min."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    layout = tokens["layout"]
    for key in ("thumbnail_size", "grid_gap", "dock_width_min", "preview_height_min"):
        assert key in layout, f"Missing layout token: {key}"


def test_bg_primary_is_dark():
    """Background primary color matches the dark #16151A spec."""
    from tarragon.theme.tokens import get_token

    bg = get_token("colors", "bg_primary")
    assert bg.upper() == "#16151A"


def test_coral_and_amber_accent_colors_present():
    """Coral and amber accent colors are defined in tokens."""
    from tarragon.theme.tokens import get_token

    coral_strong = get_token("colors", "coral_strong")
    coral_muted = get_token("colors", "coral_muted")
    amber_accent = get_token("colors", "amber_accent")

    assert coral_strong == "#E76F51"
    assert coral_muted == "#C95A42"
    assert amber_accent == "#F4A261"


# ── QSS File Tests ─────────────────────────────────────────────────────


def test_qss_file_exists_and_is_readable():
    """app.qss exists in the theme package and can be read as text."""
    from tarragon.theme.loader import ThemeLoader

    loader = ThemeLoader()
    qss = loader.load_qss()
    assert isinstance(qss, str), "QSS content must be a string"
    assert len(qss) > 0, "QSS file must not be empty"


def test_qss_contains_dark_background():
    """The QSS stylesheet references the dark background color #16151A."""
    from tarragon.theme.loader import ThemeLoader

    loader = ThemeLoader()
    qss = loader.load_qss()
    assert "#16151A" in qss, "QSS must contain bg_primary color (#16151A)"


def test_qss_contains_coral_accent_colors():
    """The QSS references coral accent colors for hover/select states."""
    from tarragon.theme.loader import ThemeLoader

    loader = ThemeLoader()
    qss = loader.load_qss()
    assert "#E76F51" in qss, "QSS must contain coral_strong (#E76F51)"
    assert "#C95A42" in qss, "QSS must contain coral_muted (#C95A42)"


def test_qss_contains_amber_accent_color():
    """The QSS references amber accent color for highlights and focus states."""
    from tarragon.theme.loader import ThemeLoader

    loader = ThemeLoader()
    qss = loader.load_qss()
    assert "#F4A261" in qss, "QSS must contain amber_accent (#F4A261)"


def test_qss_does_not_use_forbidden_properties():
    """The QSS does not use gradients or shadows (design constraint)."""
    from tarragon.theme.loader import ThemeLoader

    loader = ThemeLoader()
    qss = loader.load_qss().lower()
    assert "qgradient" not in qss, "QSS must not contain Qt gradient syntax"
    assert "qdrawsvg" not in qss, "QSS must not use QDrawSvg"


# ── MainWindow Integration Tests ────────────────────────────────────────


def test_apply_theme_no_exception(qapp):  # noqa: ARG001
    """MainWindow._apply_theme() applies stylesheet without raising."""
    from tarragon.main_window import MainWindow

    window = MainWindow()
    try:
        window._apply_theme()
        # If we get here, no exception was raised.
    finally:
        window.close()


def test_apply_theme_sets_stylesheet(qapp):  # noqa: ARG001
    """After _apply_theme(), the stylesheet is non-empty."""
    from tarragon.main_window import MainWindow

    window = MainWindow()
    try:
        window._apply_theme()
        style = window.styleSheet()
        assert isinstance(style, str), "setStyleSheet should store a string"
        assert len(style) > 0, "Style sheet must not be empty after apply"
    finally:
        window.close()


def test_stylesheet_contains_dark_background(qapp):  # noqa: ARG001
    """The applied stylesheet contains the dark background color."""
    from tarragon.main_window import MainWindow

    window = MainWindow()
    try:
        window._apply_theme()
        style = window.styleSheet()
        assert "#16151A" in style, "Applied stylesheet must contain bg_primary"
    finally:
        window.close()


def test_stylesheet_contains_coral_hover_state(qapp):  # noqa: ARG001
    """The applied stylesheet references coral for hover states."""
    from tarragon.main_window import MainWindow

    window = MainWindow()
    try:
        window._apply_theme()
        style = window.styleSheet()
        assert "#C95A42" in style, "Applied stylesheet must contain coral_muted for hover/pressed"
    finally:
        window.close()


def test_stylesheet_contains_amber_select_state(qapp):  # noqa: ARG001
    """The applied stylesheet references amber for selected/focus states."""
    from tarragon.main_window import MainWindow

    window = MainWindow()
    try:
        window._apply_theme()
        style = window.styleSheet()
        assert "#F4A261" in style, "Applied stylesheet must contain amber_accent"
    finally:
        window.close()


def test_theme_loader_loads_tokens():
    """ThemeLoader.load_tokens() returns a valid dict with all sections."""
    from tarragon.theme.loader import ThemeLoader

    loader = ThemeLoader()
    tokens = loader.load_tokens()
    assert isinstance(tokens, dict)
    required_sections = {"colors", "typography", "spacing", "radius", "motion", "layout"}
    assert set(tokens.keys()) == required_sections


def test_theme_loader_loads_qss():
    """ThemeLoader.load_qss() returns a non-empty string."""
    from tarragon.theme.loader import ThemeLoader

    loader = ThemeLoader()
    qss = loader.load_qss()
    assert isinstance(qss, str)
    assert len(qss) > 0


def test_theme_loader_is_singleton_safe():
    """Multiple ThemeLoader instances can coexist without issue."""
    from tarragon.theme.loader import ThemeLoader

    loader1 = ThemeLoader()
    loader2 = ThemeLoader()
    qss1 = loader1.load_qss()
    qss2 = loader2.load_qss()
    assert qss1 == qss2, "All loaders should return identical QSS content"
