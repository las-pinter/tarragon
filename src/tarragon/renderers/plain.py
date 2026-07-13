"""Plain image rendering — open and normalise standard image formats."""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageOps


def render_plain_image(file_path: Path, target_size: int | None = None) -> Image.Image | None:
    """Open a plain image and make it display-safe.

    Parameters
    ----------
    file_path:
        Path to the source image file.
    target_size:
        If specified, shrink the image so the longest side is at most
        *target_size* pixels.  If ``None``, render at full resolution
        (no resizing beyond EXIF correction and mode conversion).

    Steps
    -----
    1. Open the file with Pillow (supports JPEG, PNG, WebP, TIFF, …).
    2. Convert non-RGB/RGBA modes to RGBA so the rest of the pipeline sees
       a consistent pixel format.
    3. Apply EXIF orientation correction.
    4. If *target_size* is given, shrink in-place preserving aspect ratio.

    Returns *None* (instead of raising) when the file cannot be opened or
    decoded — the caller should treat ``None`` as "skip / use placeholder".
    """
    try:
        img = Image.open(file_path)
        img = ImageOps.exif_transpose(img) or img  # Apply EXIF rotation
        if img.mode not in ("RGBA", "RGB"):
            img = img.convert("RGBA")
        if target_size is not None:
            img.thumbnail((target_size, target_size), Image.LANCZOS)
        return img
    except (OSError, ValueError):
        return None
