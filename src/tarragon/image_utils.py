"""Image utility functions — EXIF orientation helpers for cached thumbnails.

Cached PNG thumbnails strip EXIF metadata, so ``PIL.ImageOps.exif_transpose()``
is a no-op on them.  These utilities read the orientation tag from the
*original* source file and apply the corresponding geometric transformation
to a cached image so that old caches still display correctly.

This module has **no Qt dependency** — it only requires Pillow.
"""

from __future__ import annotations

import logging
from pathlib import Path

from PIL import Image

logger = logging.getLogger(__name__)

# EXIF orientation tag ID
_EXIF_ORIENTATION_TAG = 0x0112


def _apply_exif_from_original(image: Image.Image, original_path: Path) -> Image.Image:
    """Apply EXIF orientation from the original file to a cached image.

    Cached PNG thumbnails strip EXIF metadata, so ``ImageOps.exif_transpose()``
    is a no-op on them.  This helper reads the orientation tag from the
    *original* source file and applies the corresponding geometric
    transformation to *image* so that old caches still display correctly.

    WHY NOT ``ImageOps.exif_transpose()``?
    That function reads the orientation tag from the image object itself.
    Here we need to apply orientation from a *different* file (the original
    source) to a cached thumbnail dat has no EXIF data.  There is no way to
    tell ``exif_transpose()`` "use dis orientation value" — it only looks at
    the image's own EXIF.  Injecting EXIF into the cached image just to call
    it would be more complex and error-prone dan da manual mapping below.

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
            exif = orig.getexif()
            orientation = exif.get(_EXIF_ORIENTATION_TAG)
            if orientation and orientation != 1:
                image = _transpose_for_orientation(image, orientation)
    except Exception:  # noqa: BLE001 — best-effort; never block preview
        logger.warning("Failed to read EXIF orientation from %s", original_path, exc_info=True)
    return image


def _transpose_for_orientation(image: Image.Image, orientation: int) -> Image.Image:
    """Apply the EXIF orientation transformation to *image*.

    Mirrors the logic of :func:`PIL.ImageOps.exif_transpose` for orientation
    values 2–8.  Orientation 1 (normal) is handled by the caller.

    NOTE: This manual implementation exists because ``ImageOps.exif_transpose()``
    reads the orientation tag from the image *itself*, but we need to apply
    orientation read from a *different* file (the original source) to a cached
    thumbnail that has no EXIF data.  We cannot inject EXIF into the cached
    image and call ``exif_transpose()`` — that would be more complex and fragile
    than this straightforward mapping.  Do NOT replace dis wiv ``exif_transpose``.
    """
    if orientation == 2:
        return image.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
    if orientation == 3:
        return image.transpose(Image.Transpose.ROTATE_180)
    if orientation == 4:
        return image.transpose(Image.Transpose.FLIP_TOP_BOTTOM)
    if orientation == 5:
        return image.transpose(Image.Transpose.TRANSPOSE)
    if orientation == 6:
        return image.transpose(Image.Transpose.ROTATE_270)
    if orientation == 7:
        return image.transpose(Image.Transpose.TRANSVERSE)
    if orientation == 8:
        return image.transpose(Image.Transpose.ROTATE_90)
    return image
