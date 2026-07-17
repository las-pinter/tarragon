"""Tests for tarragon.services.color_tagger — dominant color tag extraction.

WAAAGH! Wrenchbasha's torture chamber for da color tagger!
Uses synthetic PIL images — no external files needed.
"""

from __future__ import annotations

import pytest
from PIL import Image

from tarragon.services.color_tagger import extract_dominant_color_tags

# =========================================================================
# Test Helpers — synthetic image factories
# =========================================================================


def _hsv_to_rgb(h: float, s: float, v: float) -> tuple[int, int, int]:
    """Convert HSV (H: 0-360, S: 0-1, V: 0-1) to RGB (0-255)."""
    c = v * s
    x = c * (1 - abs((h / 60) % 2 - 1))
    m = v - c

    if h < 60:
        r, g, b = c, x, 0.0
    elif h < 120:
        r, g, b = x, c, 0.0
    elif h < 180:
        r, g, b = 0.0, c, x
    elif h < 240:
        r, g, b = 0.0, x, c
    elif h < 300:
        r, g, b = x, 0.0, c
    else:
        r, g, b = c, 0.0, x

    return (
        int(round((r + m) * 255)),
        int(round((g + m) * 255)),
        int(round((b + m) * 255)),
    )


def _solid_image(h: float, s: float, v: float, size: tuple[int, int] = (100, 100)) -> Image.Image:
    """Create a solid-color image from HSV values."""
    rgb = _hsv_to_rgb(h, s, v)
    return Image.new("RGB", size, rgb)


def _split_image(
    h1: float,
    s1: float,
    v1: float,
    h2: float,
    s2: float,
    v2: float,
    ratio: float = 0.5,
    size: tuple[int, int] = (100, 100),
) -> Image.Image:
    """Create a horizontally split image: top *ratio* is color1, rest is color2."""
    w, h = size
    split_y = int(h * ratio)
    top_color = _hsv_to_rgb(h1, s1, v1)
    bottom_color = _hsv_to_rgb(h2, s2, v2)

    img = Image.new("RGB", size, top_color)
    bottom = Image.new("RGB", (w, h - split_y), bottom_color)
    img.paste(bottom, (0, split_y))
    return img


def _quad_image(
    colors_hsv: list[tuple[float, float, float]],
    size: int = 100,
) -> Image.Image:
    """Create an image with 4 color quadrants (TL, TR, BL, BR)."""
    half = size // 2
    img = Image.new("RGB", (size, size))
    positions = [(0, 0), (half, 0), (0, half), (half, half)]
    for (h, s, v), (x, y) in zip(colors_hsv, positions):
        quadrant = Image.new("RGB", (half, half), _hsv_to_rgb(h, s, v))
        img.paste(quadrant, (x, y))
    return img


# =========================================================================
# Tests — Arrange-Act-Assert pattern
# =========================================================================


def test_mostly_red_image() -> None:
    """Solid red image should produce ['color:red']."""
    # Arrange — red at hue=10, vivid but not too bright (V=0.7 < 0.92)
    img = _solid_image(h=10, s=0.8, v=0.7)

    # Act
    tags = extract_dominant_color_tags(img)

    # Assert
    assert tags == ["color:red"]


def test_mostly_blue_image() -> None:
    """Solid blue image should produce ['color:blue']."""
    # Arrange — blue at hue=170 (center of 140-200 bucket)
    img = _solid_image(h=170, s=0.8, v=0.7)

    # Act
    tags = extract_dominant_color_tags(img)

    # Assert
    assert tags == ["color:blue"]


def test_gray_image_is_neutral() -> None:
    """Gray image (zero saturation) should produce ['color:neutral']."""
    # Arrange — mid-gray: R=G=B=128
    img = Image.new("RGB", (100, 100), (128, 128, 128))

    # Act
    tags = extract_dominant_color_tags(img)

    # Assert
    assert tags == ["color:neutral"]


def test_two_dominant_colors() -> None:
    """60% green / 40% blue should produce both tags with default threshold."""
    # Arrange — green at hue=70, blue at hue=170
    img = _split_image(
        h1=70,
        s1=0.8,
        v1=0.7,  # green
        h2=170,
        s2=0.8,
        v2=0.7,  # blue
        ratio=0.6,
    )

    # Act
    tags = extract_dominant_color_tags(img)

    # Assert — both colors above 10% threshold
    assert "color:green" in tags
    assert "color:blue" in tags
    assert len(tags) == 2


def test_tiny_accent_below_threshold() -> None:
    """5% red accent on 95% gray should not produce a red tag."""
    # Arrange — 95% gray, 5% red
    img = _split_image(
        h1=0,
        s1=0.0,
        v1=0.5,  # gray (S=0 → neutral)
        h2=10,
        s2=0.8,
        v2=0.7,  # red
        ratio=0.95,
    )

    # Act
    tags = extract_dominant_color_tags(img)

    # Assert — red at 5% is below 10% threshold
    assert "color:red" not in tags
    assert "color:neutral" in tags


def test_low_saturation_is_neutral() -> None:
    """Beige / low-saturation color should be classified as neutral."""
    # Arrange — beige: hue=30, S=0.1 (< 0.15 threshold), V=0.7
    img = _solid_image(h=30, s=0.1, v=0.7)

    # Act
    tags = extract_dominant_color_tags(img)

    # Assert
    assert tags == ["color:neutral"]


def test_empty_image_returns_empty() -> None:
    """A zero-size image should return an empty list."""
    # Arrange
    img = Image.new("RGB", (0, 0))

    # Act
    tags = extract_dominant_color_tags(img)

    # Assert
    assert tags == []


# -- Parametrized bucket coverage ------------------------------------------

# Center hue of each bucket, with S=0.8 and V=0.7 to stay clearly colorful
_BUCKET_CASES = [
    (10, "color:red"),  # red: 0-15
    (24, "color:orange"),  # orange: 15-33
    (42, "color:yellow"),  # yellow: 33-50
    (68, "color:green"),  # green: 50-85
    (95, "color:teal"),  # teal: 85-105
    (123, "color:cyan"),  # cyan: 105-140
    (170, "color:blue"),  # blue: 140-200
    (235, "color:purple"),  # purple: 200-270
    (290, "color:magenta"),  # magenta: 270-310
]


@pytest.mark.parametrize(
    ("hue", "expected_tag"),
    _BUCKET_CASES,
    ids=[name for _, name in _BUCKET_CASES],
)
def test_all_buckets_covered(hue: float, expected_tag: str) -> None:
    """Each color bucket should be reachable with a solid-color image."""
    # Arrange — solid image at bucket center hue, vivid saturation
    img = _solid_image(h=hue, s=0.8, v=0.7)

    # Act
    tags = extract_dominant_color_tags(img)

    # Assert
    assert tags == [expected_tag]


# -- Parameter variation tests ---------------------------------------------


def test_custom_palette_size() -> None:
    """Different palette_size should affect the number of resulting tags."""
    # Arrange — 4 distinct colors in quadrants
    img = _quad_image(
        [
            (10, 0.8, 0.7),  # red
            (70, 0.8, 0.7),  # green
            (170, 0.8, 0.7),  # blue
            (290, 0.8, 0.7),  # magenta
        ]
    )

    # Act
    tags_few = extract_dominant_color_tags(img, palette_size=2)
    tags_many = extract_dominant_color_tags(img, palette_size=8)

    # Assert — more palette slots → at least as many tags
    assert len(tags_few) <= len(tags_many)
    # With 4 distinct colors and palette_size=8, all 4 should be found
    assert len(tags_many) == 4


def test_custom_min_share() -> None:
    """Different min_share threshold should filter tags differently."""
    # Arrange — 70% green, 30% blue
    img = _split_image(
        h1=70,
        s1=0.8,
        v1=0.7,  # green
        h2=170,
        s2=0.8,
        v2=0.7,  # blue
        ratio=0.7,
    )

    # Act & Assert
    # Low threshold: both colors pass
    tags_low = extract_dominant_color_tags(img, min_share=0.10)
    assert "color:green" in tags_low
    assert "color:blue" in tags_low

    # Medium threshold: only green passes (70% > 50%, 30% < 50%)
    tags_mid = extract_dominant_color_tags(img, min_share=0.50)
    assert "color:green" in tags_mid
    assert "color:blue" not in tags_mid

    # High threshold: neither passes (70% < 80%)
    tags_high = extract_dominant_color_tags(img, min_share=0.80)
    assert tags_high == []
