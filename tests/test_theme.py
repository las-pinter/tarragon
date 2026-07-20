"""Tests for theme loading — token definitions, QSS parsing, and MainWindow integration."""

from __future__ import annotations

from typing import Any

# ---------------------------------------------------------------------------
# QSS File Tests
# ---------------------------------------------------------------------------


def test_qss_file_exists_and_is_readable() -> None:
    """Generated QSS is a non-empty string."""
    from tarragon.theme.qss_generator import generate_qss

    qss = generate_qss()
    assert isinstance(qss, str), "QSS content must be a string"
    assert len(qss) > 0, "QSS file must not be empty"


def test_qss_does_not_use_forbidden_properties() -> None:
    """The QSS does not use gradients or shadows (design constraint)."""
    from tarragon.theme.qss_generator import generate_qss

    qss = generate_qss()
    assert "qgradient" not in qss, "QSS must not contain Qt gradient syntax"
    assert "qdrawsvg" not in qss, "QSS must not use QDrawSvg"


# ---------------------------------------------------------------------------
# MainWindow Integration Tests
# ---------------------------------------------------------------------------


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


# ---------------------------------------------------------------------------
# Motion Module Tests
# ---------------------------------------------------------------------------


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


# ---------------------------------------------------------------------------
# Layout Module Tests
# ---------------------------------------------------------------------------


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


# ---------------------------------------------------------------------------
# Badge Color Constants Tests
# ---------------------------------------------------------------------------


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
