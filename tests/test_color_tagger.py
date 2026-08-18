"""Tests for ColorTagger"""

from __future__ import annotations

import pytest
from PIL import Image

from tarragon.services.color_tagger import extract_dominant_colors


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


_BUCKET_CASES = [
    (10, "red"),  # red: 0-15
    (24, "orange"),  # orange: 15-33
    (42, "yellow"),  # yellow: 33-50
    (68, "green"),  # green: 50-85
    (95, "teal"),  # teal: 85-105
    (123, "cyan"),  # cyan: 105-140
    (170, "blue"),  # blue: 140-200
    (235, "purple"),  # purple: 200-270
    (290, "magenta"),  # magenta: 270-310
]


class TestSolidColorExtraction:
    """Solid-color images produce the expected dominant color tag."""

    def test_mostly_red_image(self) -> None:
        """A solid red image produces the 'red' tag."""
        img = _solid_image(h=10, s=0.8, v=0.7)
        colors = extract_dominant_colors(img)
        assert colors == {"red"}

    def test_mostly_blue_image(self) -> None:
        """A solid blue image produces the 'blue' tag."""
        img = _solid_image(h=170, s=0.8, v=0.7)
        colors = extract_dominant_colors(img)
        assert colors == {"blue"}

    def test_gray_image_is_neutral(self) -> None:
        """A gray image produces the 'neutral' tag."""
        img = Image.new("RGB", (100, 100), (128, 128, 128))
        colors = extract_dominant_colors(img)
        assert colors == {"neutral"}

    def test_low_saturation_is_neutral(self) -> None:
        """A low-saturation beige image is classified as neutral."""
        img = _solid_image(h=30, s=0.1, v=0.7)
        colors = extract_dominant_colors(img)
        assert colors == {"neutral"}


class TestMultiColorImages:
    """Images with multiple colors produce multiple dominant color tags."""

    def test_two_dominant_colors(self) -> None:
        """A green/blue split image produces both color tags."""
        img = _split_image(
            h1=70,
            s1=0.8,
            v1=0.7,  # green
            h2=170,
            s2=0.8,
            v2=0.7,  # blue
            ratio=0.6,
        )
        tags = extract_dominant_colors(img)
        assert "green" in tags
        assert "blue" in tags
        assert len(tags) == 2


class TestEdgeCases:
    """Edge cases: tiny color shares and empty images."""

    def test_tiny_accent_below_threshold(self) -> None:
        """A small red accent on gray does not produce a red tag."""
        img = _split_image(
            h1=0,
            s1=0.0,
            v1=0.5,  # gray (S=0 -> neutral)
            h2=10,
            s2=0.8,
            v2=0.7,  # red
            ratio=0.95,
        )
        tags = extract_dominant_colors(img)
        assert "red" not in tags
        assert "neutral" in tags

    def test_empty_image_returns_empty(self) -> None:
        """A zero-size image returns an empty list."""
        img = Image.new("RGB", (0, 0))
        colors = extract_dominant_colors(img)
        assert colors == set()


class TestBucketCoverage:
    """Every hue bucket is reachable with a solid-color image."""

    @pytest.mark.parametrize(
        ("hue", "expected_tag"),
        _BUCKET_CASES,
        ids=[name for _, name in _BUCKET_CASES],
    )
    def test_all_buckets_covered(self, hue: float, expected_tag: str) -> None:
        """Each color bucket is reachable with a solid-color image."""
        img = _solid_image(h=hue, s=0.8, v=0.7)
        colors = extract_dominant_colors(img)
        assert colors == {expected_tag}


class TestCustomizationOptions:
    """Custom palette_size and min_share options control tag extraction."""

    def test_custom_palette_size(self) -> None:
        """A larger palette_size returns at least as many tags."""
        img = _quad_image(
            [
                (10, 0.8, 0.7),  # red
                (70, 0.8, 0.7),  # green
                (170, 0.8, 0.7),  # blue
                (290, 0.8, 0.7),  # magenta
            ]
        )
        tags_few = extract_dominant_colors(img, palette_size=2)
        tags_many = extract_dominant_colors(img, palette_size=8)
        assert len(tags_few) <= len(tags_many)
        assert len(tags_many) == 4

    def test_custom_min_share(self) -> None:
        """Higher min_share thresholds filter out smaller color shares."""
        img = _split_image(
            h1=70,
            s1=0.8,
            v1=0.7,  # green
            h2=170,
            s2=0.8,
            v2=0.7,  # blue
            ratio=0.7,
        )
        colors_low = extract_dominant_colors(img, min_share=0.10)
        assert "green" in colors_low
        assert "blue" in colors_low

        colors_mid = extract_dominant_colors(img, min_share=0.50)
        assert "green" in colors_mid
        assert "blue" not in colors_mid

        colors_high = extract_dominant_colors(img, min_share=0.80)
        assert colors_high == set()
