"""Tests for file_type_badge module."""

from __future__ import annotations

import pytest
from PySide6.QtGui import QColor
from tarragon.theme.file_type_badge import (
    BADGE_COLORS,
    DEFAULT_BADGE_COLORS,
    get_badge_colors,
)

# ── get_badge_colors ─────────────────────────────────────────────────


@pytest.mark.parametrize(
    "ext",
    ["psd", "PSD", ".psd", ".PSD", "Psd", ".Psd"],
)
def test_get_badge_colors_psd_variants(ext: str) -> None:
    """PSD extension in various forms returns the PSD color scheme."""
    bg, text = get_badge_colors(ext)
    assert bg == QColor("#4A1B0C")
    assert text == QColor("#F0997B")


@pytest.mark.parametrize(
    "ext,expected_bg,expected_text",
    [
        ("jpg", "#1A3A2A", "#7BC88F"),
        ("jpeg", "#1A3A2A", "#7BC88F"),
        ("png", "#1A2A3A", "#7BA8C8"),
        ("tiff", "#3A2A1A", "#C8A87B"),
        ("tif", "#3A2A1A", "#C8A87B"),
        ("gif", "#2A1A3A", "#A87BC8"),
        ("webp", "#1A3A3A", "#7BC8C8"),
        ("bmp", "#3A3A1A", "#C8C87B"),
    ],
)
def test_get_badge_colors_known_extensions(ext: str, expected_bg: str, expected_text: str) -> None:
    """Known extensions return their designated color scheme."""
    bg, text = get_badge_colors(ext)
    assert bg == QColor(expected_bg)
    assert text == QColor(expected_text)


@pytest.mark.parametrize(
    "ext",
    ["exr", "hdr", "raw", "cr2", "nef", "svg", "ico", "xyz", ""],
)
def test_get_badge_colors_unknown_extensions(ext: str) -> None:
    """Unknown extensions return the default gray color scheme."""
    bg, text = get_badge_colors(ext)
    assert bg == DEFAULT_BADGE_COLORS[0]
    assert text == DEFAULT_BADGE_COLORS[1]


def test_get_badge_colors_strips_dot() -> None:
    """Extensions with leading dot are handled correctly."""
    bg_with_dot, text_with_dot = get_badge_colors(".png")
    bg_without_dot, text_without_dot = get_badge_colors("png")
    assert bg_with_dot == bg_without_dot
    assert text_with_dot == text_without_dot


def test_get_badge_colors_case_insensitive() -> None:
    """Extension lookup is case-insensitive."""
    bg_lower, text_lower = get_badge_colors("jpg")
    bg_upper, text_upper = get_badge_colors("JPG")
    bg_mixed, text_mixed = get_badge_colors("Jpg")
    assert bg_lower == bg_upper == bg_mixed
    assert text_lower == text_upper == text_mixed


# ── BADGE_COLORS dict ────────────────────────────────────────────────


def test_badge_colors_has_expected_keys() -> None:
    """BADGE_COLORS contains all specified file extensions."""
    expected_keys = {"psd", "psb", "jpg", "jpeg", "png", "tiff", "tif", "gif", "webp", "bmp", "clip"}
    assert set(BADGE_COLORS.keys()) == expected_keys


def test_badge_colors_values_are_qcolor_tuples() -> None:
    """Every entry in BADGE_COLORS is a tuple of two QColor instances."""
    for ext, (bg, text) in BADGE_COLORS.items():
        assert isinstance(bg, QColor), f"Background for {ext} is not QColor"
        assert isinstance(text, QColor), f"Text color for {ext} is not QColor"


def test_default_badge_colors_is_qcolor_tuple() -> None:
    """DEFAULT_BADGE_COLORS is a tuple of two QColor instances."""
    bg, text = DEFAULT_BADGE_COLORS
    assert isinstance(bg, QColor)
    assert isinstance(text, QColor)
    assert bg == QColor("#2A2A2A")
    assert text == QColor("#A8A5B0")
