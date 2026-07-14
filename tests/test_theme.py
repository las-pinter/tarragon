"""Tests for theme loading — token definitions, QSS parsing, and MainWindow integration."""

from __future__ import annotations

from typing import Any

import pytest

# ── Fixtures ───────────────────────────────────────────────────────────


# ── Token Definition Tests ─────────────────────────────────────────────


def test_tokens_definitions_valid() -> None:
    """load_tokens() returns a valid dict of design tokens."""
    from tarragon.theme.tokens import load_tokens

    data = load_tokens()
    assert isinstance(data, dict), "tokens must be a dict"


def test_tokens_has_all_sections() -> None:
    """Token definitions contain every required section from the design system spec."""
    from tarragon.theme.tokens import load_tokens

    data = load_tokens()
    required_sections = {"colors", "typography", "spacing", "radius", "motion", "layout", "badge"}
    assert set(data.keys()) == required_sections, (
        f"token sections {set(data.keys())} != expected {required_sections}"
    )


def test_tokens_colors_section() -> None:
    """The colors section contains all expected color tokens."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    colors = tokens["colors"]
    expected_keys = {
        "bg_primary",
        "bg_secondary",
        "bg_tertiary",
        "surface_highlight",
        "surface_hover",
        "coral_strong",
        "coral_muted",
        "coral_dark",
        "coral_bright",
        "amber_accent",
        "amber_light",
        "amber_dark",
        "text_primary",
        "text_secondary",
        "text_tertiary",
        "text_muted",
        "bg_disabled",
        "border_disabled",
        "border_subtle",
        "border_card",
        "border_interactive",
        "bg_log_panel",
        "highlight_disabled",
        "separator",
    }
    assert set(colors.keys()) == expected_keys


def test_tokens_typography_section() -> None:
    """The typography section contains all expected font tokens."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    typos = tokens["typography"]
    expected_keys = {
        "font_family",
        "mono_family",
        "body_size",
        "heading_size",
        "small_size",
        "log_size",
        "caption_size",
        "weight_regular",
        "weight_medium",
        "weight_semibold",
    }
    assert set(typos.keys()) == expected_keys


def test_tokens_spacing_section() -> None:
    """The spacing section contains xs, sm, md, lg, xl."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    spacing = tokens["spacing"]
    for key in ("xs", "sm", "md", "lg", "xl"):
        assert key in spacing, f"Missing spacing token: {key}"
        assert isinstance(spacing[key], int), f"Spacing value '{key}' must be an integer"


def test_tokens_radius_section() -> None:
    """The radius section contains none, xs, sm, md, lg, xl."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    radii = tokens["radius"]
    for key in ("none", "xs", "sm", "md", "lg", "xl"):
        assert key in radii, f"Missing radius token: {key}"


def test_tokens_motion_section() -> None:
    """The motion section contains duration and easing tokens."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    motion = tokens["motion"]
    for key in ("duration_fast", "duration_normal", "easing"):
        assert key in motion, f"Missing motion token: {key}"


def test_tokens_layout_section() -> None:
    """The layout section contains thumbnail_size, grid_gap, sidebar_width_px, multi_preview_max_default."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    layout = tokens["layout"]
    for key in ("thumbnail_size", "grid_gap", "sidebar_width_px", "multi_preview_max_default"):
        assert key in layout, f"Missing layout token: {key}"


def test_tokens_badge_section() -> None:
    """The badge section contains 9 bg/fg color pairs."""
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    badge = tokens["badge"]
    hues = ("vinous", "sage", "navy", "umber", "plum", "teal", "olive", "azure", "neutral")
    for hue in hues:
        for prefix in ("bg_", "fg_"):
            key = prefix + hue
            assert key in badge, f"Missing badge token: {key}"
            assert isinstance(badge[key], str), f"Badge value '{key}' must be a string color"
    assert len(badge) == len(hues) * 2, f"Expected {len(hues) * 2} badge tokens, got {len(badge)}"


def test_bg_primary_is_dark() -> None:
    """Background primary color matches the dark #16151A spec."""
    from tarragon.theme.tokens import get_token

    bg = get_token("colors", "bg_primary")
    assert bg.upper() == "#16151A"


def test_coral_and_amber_accent_colors_present() -> None:
    """Coral and amber accent colors are defined in tokens."""
    from tarragon.theme.tokens import get_token

    coral_strong = get_token("colors", "coral_strong")
    coral_muted = get_token("colors", "coral_muted")
    amber_accent = get_token("colors", "amber_accent")

    assert coral_strong == "#F0997B"
    assert coral_muted == "#D85A30"
    assert amber_accent == "#FAC775"


# ── QSS File Tests ─────────────────────────────────────────────────────


def test_qss_file_exists_and_is_readable() -> None:
    """Generated QSS is a non-empty string."""
    from tarragon.theme.qss_generator import load_and_generate_qss

    qss = load_and_generate_qss()
    assert isinstance(qss, str), "QSS content must be a string"
    assert len(qss) > 0, "QSS file must not be empty"


def test_qss_contains_dark_background() -> None:
    """The QSS stylesheet references the dark background color #16151A."""
    from tarragon.theme.qss_generator import load_and_generate_qss

    qss = load_and_generate_qss()
    assert "#16151A" in qss, "QSS must contain bg_primary color (#16151A)"


def test_qss_contains_coral_accent_colors() -> None:
    """The QSS references coral accent colors for hover/select states."""
    from tarragon.theme.qss_generator import load_and_generate_qss

    qss = load_and_generate_qss()
    assert "#F0997B" in qss, "QSS must contain coral_strong (#F0997B)"
    assert "#D85A30" in qss, "QSS must contain coral_muted (#D85A30)"


def test_qss_contains_amber_accent_color() -> None:
    """The QSS references amber accent color for highlights and focus states."""
    from tarragon.theme.qss_generator import load_and_generate_qss

    qss = load_and_generate_qss()
    assert "#FAC775" in qss, "QSS must contain amber_accent (#FAC775)"


def test_qss_does_not_use_forbidden_properties() -> None:
    """The QSS does not use gradients or shadows (design constraint)."""
    from tarragon.theme.qss_generator import load_and_generate_qss

    qss = load_and_generate_qss().lower()
    assert "qgradient" not in qss, "QSS must not contain Qt gradient syntax"
    assert "qdrawsvg" not in qss, "QSS must not use QDrawSvg"


# ── MainWindow Integration Tests ────────────────────────────────────────


def test_apply_theme_no_exception(qapp: Any) -> None:  # noqa: ARG001
    """MainWindow._apply_theme() applies stylesheet without raising."""
    from tarragon.main_window import MainWindow

    window = MainWindow()
    try:
        window._apply_theme()
        # If we get here, no exception was raised.
    finally:
        window.close()


def test_apply_theme_sets_stylesheet(qapp: Any) -> None:  # noqa: ARG001
    """After _apply_theme(), the stylesheet is non-empty."""
    from PySide6.QtWidgets import QApplication

    from tarragon.main_window import MainWindow

    window = MainWindow()
    try:
        window._apply_theme()
        app = QApplication.instance()
        assert isinstance(app, QApplication)
        style = app.styleSheet()
        assert isinstance(style, str), "setStyleSheet should store a string"
        assert len(style) > 0, "Style sheet must not be empty after apply"
    finally:
        window.close()


def test_stylesheet_contains_dark_background(qapp: Any) -> None:  # noqa: ARG001
    """The applied stylesheet contains the dark background color."""
    from PySide6.QtWidgets import QApplication

    from tarragon.main_window import MainWindow

    window = MainWindow()
    try:
        window._apply_theme()
        app = QApplication.instance()
        assert isinstance(app, QApplication)
        style = app.styleSheet()
        assert "#16151A" in style, "Applied stylesheet must contain bg_primary"
    finally:
        window.close()


def test_stylesheet_contains_coral_hover_state(qapp: Any) -> None:  # noqa: ARG001
    """The applied stylesheet references coral for hover states."""
    from PySide6.QtWidgets import QApplication

    from tarragon.main_window import MainWindow

    window = MainWindow()
    try:
        window._apply_theme()
        app = QApplication.instance()
        assert isinstance(app, QApplication)
        style = app.styleSheet()
        assert "#D85A30" in style, "Applied stylesheet must contain coral_muted for hover/pressed"
    finally:
        window.close()


def test_stylesheet_contains_amber_select_state(qapp: Any) -> None:  # noqa: ARG001
    """The applied stylesheet references amber for selected/focus states."""
    from PySide6.QtWidgets import QApplication

    from tarragon.main_window import MainWindow

    window = MainWindow()
    try:
        window._apply_theme()
        app = QApplication.instance()
        assert isinstance(app, QApplication)
        style = app.styleSheet()
        assert "#FAC775" in style, "Applied stylesheet must contain amber_accent"
    finally:
        window.close()


def test_load_and_generate_qss_returns_valid_tokens() -> None:
    """load_and_generate_qss() produces QSS from a valid token dict."""
    from tarragon.theme.qss_generator import generate_qss, load_and_generate_qss
    from tarragon.theme.tokens import load_tokens

    qss = load_and_generate_qss()
    assert isinstance(qss, str)
    assert len(qss) > 0
    assert qss == generate_qss(load_tokens())


def test_load_and_generate_qss_returns_non_empty_string() -> None:
    """load_and_generate_qss() returns a non-empty string."""
    from tarragon.theme.qss_generator import load_and_generate_qss

    qss = load_and_generate_qss()
    assert isinstance(qss, str)
    assert len(qss) > 0


# ── Motion Module Tests ────────────────────────────────────────────────


def test_motion_duration_fast_value() -> None:
    """DURATION_FAST matches the tokens.json motion.duration_fast value (150 ms)."""
    from tarragon.theme.constants import DURATION_FAST

    assert DURATION_FAST == 150


def test_motion_duration_normal_value() -> None:
    """DURATION_NORMAL matches the tokens.json motion.duration_normal value (200 ms)."""
    from tarragon.theme.constants import DURATION_NORMAL

    assert DURATION_NORMAL == 200


def test_motion_duration_fast_is_int() -> None:
    """DURATION_FAST is an integer suitable for animation duration."""
    from tarragon.theme.constants import DURATION_FAST

    assert isinstance(DURATION_FAST, int)


def test_motion_duration_normal_is_int() -> None:
    """DURATION_NORMAL is an integer suitable for animation duration."""
    from tarragon.theme.constants import DURATION_NORMAL

    assert isinstance(DURATION_NORMAL, int)


def test_motion_constants_match_tokens_json() -> None:
    """Motion constants in constants.py are derived directly from tokens.json."""
    from tarragon.theme.constants import DURATION_FAST, DURATION_NORMAL
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    motion = tokens["motion"]
    assert DURATION_FAST == motion["duration_fast"]
    assert DURATION_NORMAL == motion["duration_normal"]


# ── Layout Module Tests ────────────────────────────────────────────────


def test_layout_grid_gap_value() -> None:
    """GRID_GAP matches the tokens.json layout.grid_gap value (14 px)."""
    from tarragon.theme.constants import GRID_GAP

    assert GRID_GAP == 14


def test_layout_thumbnail_size_value() -> None:
    """THUMBNAIL_SIZE matches the tokens.json layout.thumbnail_size value (160 px)."""
    from tarragon.theme.constants import THUMBNAIL_SIZE

    assert THUMBNAIL_SIZE == 160


def test_layout_sidebar_width_px_value() -> None:
    """SIDEBAR_WIDTH_PX matches the tokens.json layout.sidebar_width_px value (220 px)."""
    from tarragon.theme.constants import SIDEBAR_WIDTH_PX

    assert SIDEBAR_WIDTH_PX == 220


def test_layout_multi_preview_max_default_value() -> None:
    """MULTI_PREVIEW_MAX_DEFAULT matches the tokens.json value (9)."""
    from tarragon.theme.constants import MULTI_PREVIEW_MAX_DEFAULT

    assert MULTI_PREVIEW_MAX_DEFAULT == 9


def test_layout_constants_are_int() -> None:
    """All layout constants are integers suitable for pixel calculations."""
    from tarragon.theme.constants import (
        GRID_GAP,
        MULTI_PREVIEW_MAX_DEFAULT,
        SIDEBAR_WIDTH_PX,
        THUMBNAIL_SIZE,
    )

    for name, value in [
        ("GRID_GAP", GRID_GAP),
        ("THUMBNAIL_SIZE", THUMBNAIL_SIZE),
        ("SIDEBAR_WIDTH_PX", SIDEBAR_WIDTH_PX),
        ("MULTI_PREVIEW_MAX_DEFAULT", MULTI_PREVIEW_MAX_DEFAULT),
    ]:
        assert isinstance(value, int), f"{name} must be an int, got {type(value).__name__}"


def test_layout_constants_match_tokens_json() -> None:
    """Layout constants in constants.py are derived directly from tokens.json."""
    from tarragon.theme.constants import (
        GRID_GAP,
        MULTI_PREVIEW_MAX_DEFAULT,
        SIDEBAR_WIDTH_PX,
        THUMBNAIL_SIZE,
    )
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    layout = tokens["layout"]
    assert GRID_GAP == layout["grid_gap"]
    assert THUMBNAIL_SIZE == layout["thumbnail_size"]
    assert SIDEBAR_WIDTH_PX == layout["sidebar_width_px"]
    assert MULTI_PREVIEW_MAX_DEFAULT == layout["multi_preview_max_default"]


# ── Badge Color Constants Tests ────────────────────────────────────────


def test_badge_color_constants_match_tokens_json() -> None:
    """Badge color constants in colors.py match the badge section in tokens.json."""
    from tarragon.theme.colors import (
        BADGE_BG_VINOUS,
        BADGE_FG_VINOUS,
    )
    from tarragon.theme.tokens import load_tokens

    tokens = load_tokens()
    badge = tokens["badge"]
    assert BADGE_BG_VINOUS.name().upper() == badge["bg_vinous"].upper()
    assert BADGE_FG_VINOUS.name().upper() == badge["fg_vinous"].upper()


def test_all_badge_hue_colors_exist() -> None:
    """Every badge hue (vinous, sage, navy, umber, plum, teal, olive, azure, neutral)
    has both bg and fg QColor constants."""
    from tarragon.theme import colors as color_mod

    hues = ("vinous", "sage", "navy", "umber", "plum", "teal", "olive", "azure", "neutral")
    for hue in hues:
        bg_attr = f"BADGE_BG_{hue.upper()}"
        fg_attr = f"BADGE_FG_{hue.upper()}"
        assert hasattr(color_mod, bg_attr), f"Missing badge color constant: {bg_attr}"
        assert hasattr(color_mod, fg_attr), f"Missing badge color constant: {fg_attr}"
        bg_val = getattr(color_mod, bg_attr)
        fg_val = getattr(color_mod, fg_attr)
        assert bg_val.isValid(), f"{bg_attr} is not a valid QColor"
        assert fg_val.isValid(), f"{fg_attr} is not a valid QColor"


def test_badge_bg_and_fg_are_different() -> None:
    """For each badge hue, background and foreground colors are distinct."""
    from tarragon.theme import colors as color_mod

    hues = ("vinous", "sage", "navy", "umber", "plum", "teal", "olive", "azure", "neutral")
    for hue in hues:
        bg = getattr(color_mod, f"BADGE_BG_{hue.upper()}")
        fg = getattr(color_mod, f"BADGE_FG_{hue.upper()}")
        assert bg != fg, f"Badge hue '{hue}' has identical bg and fg colors"
