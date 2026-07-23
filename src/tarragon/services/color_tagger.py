"""Extracts dominant color tags from images.

Pure, deterministic algorithm for extracting color information from images.
"""

from __future__ import annotations

from PIL import Image

from tarragon.theme.color_buckets import COLOR_BUCKETS


def _rgb_to_hsv(r: float, g: float, b: float) -> tuple[float, float, float]:
    """Convert RGB (0-1 range) to HSV (H: 0-360, S: 0-1, V: 0-1)."""
    c_max = max(r, g, b)
    c_min = min(r, g, b)
    delta = c_max - c_min

    # Hue
    if delta == 0:
        h = 0.0
    elif c_max == r:
        h = (60.0 * ((g - b) / delta)) % 360
    elif c_max == g:
        h = (60.0 * ((b - r) / delta) + 120.0) % 360
    else:  # c_max == b
        h = (60.0 * ((r - g) / delta) + 240.0) % 360

    # Saturation
    s = 0.0 if c_max == 0 else (delta / c_max)

    # Value
    v = c_max

    return h, s, v


def _hue_to_bucket(hue: float) -> str | None:
    """Map a hue value (0-360) to a color bucket name.

    Returns None if the hue falls in a gap between buckets (310-345).
    """
    for name, ranges in COLOR_BUCKETS.items():
        for h_min, h_max in ranges:
            if h_min <= hue < h_max:
                return name
    return None


def extract_dominant_color_tags(
    image: Image.Image,
    palette_size: int = 8,
    min_share: float = 0.10,
    neutral_s_threshold: float = 0.15,
) -> list[str]:
    """Extract dominant color tags from a PIL image.

    Algorithm:
    1. Convert to RGB
    2. Downsample to 64x64
    3. Quantize to palette_size colors (MEDIANCUT)
    4. Get color counts
    5. Convert each color to HSV
    6. Map to color bucket
    7. Filter by min_share threshold

    Returns list of tag strings like ["color:red", "color:blue"],
    sorted alphabetically.
    """
    # Handle empty images
    if image.width == 0 or image.height == 0:
        return []

    # 1. Convert to RGB
    rgb = image.convert("RGB")

    # 2. Downsample to 64x64
    small = rgb.resize((64, 64), Image.Resampling.LANCZOS)

    # 3. Quantize to palette_size colors (MEDIANCUT)
    quantized = small.quantize(colors=palette_size, method=Image.Quantize.MEDIANCUT)

    # 4. Get color counts — convert back to RGB so getcolors returns (count, (r,g,b))
    rgb_quantized = quantized.convert("RGB")
    colors = rgb_quantized.getcolors()
    if not colors:
        return []

    # 5. Calculate total pixels
    total_pixels = sum(count for count, _ in colors)
    if total_pixels == 0:
        return []

    # 6. For each color: convert to HSV, map to bucket, accumulate share
    bucket_shares: dict[str, float] = {}
    for count, color in colors:
        # getcolors() is typed to allow a bare int/float pixel value for
        # some image modes; since rgb_quantized was explicitly converted
        # to "RGB" above, color is always an (r, g, b) tuple at runtime.
        if not isinstance(color, tuple):
            continue
        r, g, b = color
        rn, gn, bn = r / 255.0, g / 255.0, b / 255.0
        h, s, v = _rgb_to_hsv(rn, gn, bn)

        # Neutral check: low saturation, very bright, or very dark
        if s <= neutral_s_threshold or v >= 0.92 or v <= 0.08:
            bucket = "neutral"
        else:
            bucket = _hue_to_bucket(h) or "neutral"

        bucket_shares[bucket] = bucket_shares.get(bucket, 0.0) + count / total_pixels

    # 7. Filter by min_share and format as tags
    tags = [f"color:{name}" for name, share in bucket_shares.items() if share >= min_share]

    # 8. Sort alphabetically
    return sorted(tags)
