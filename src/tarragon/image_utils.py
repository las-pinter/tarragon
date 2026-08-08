"""Image utility functions

EXIF orientation helpers for cached thumbnails.
"""

from __future__ import annotations

import logging
from pathlib import Path

from PIL import Image, ImageOps

logger = logging.getLogger(__name__)

EXIF_ORIENTATION_TAG = 0x0112


def apply_exif_from_original(image: Image.Image, original_path: Path) -> Image.Image:
    """Apply EXIF orientation from the original file to a cached image.

    Parameters
    ----------
    image:
        The (cached) PIL Image to transform in-place (a copy is returned).
    original_path:
        Path to the original source file whose EXIF orientation to read.

    Returns
    -------
    Image.Image
        The orientation-corrected image (may be the same object if no
        correction was needed).
    """
    try:
        with Image.open(original_path) as orig:
            orientation = orig.getexif().get(EXIF_ORIENTATION_TAG)
        if orientation and orientation != 1:
            image.getexif()[EXIF_ORIENTATION_TAG] = orientation
            image = ImageOps.exif_transpose(image)
    except Exception:
        logger.warning("Failed to read EXIF orientation from %s", original_path, exc_info=True)
    return image
