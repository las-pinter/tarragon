"""Krita image rendering"""

from __future__ import annotations

import logging
import zipfile
from io import BytesIO
from pathlib import Path

from PIL import Image, ImageOps

logger = logging.getLogger(__name__)


def render_kra_image(file_path: Path, target_size: int | None = None) -> Image.Image | None:
    """Extract the merged image from a kra archive"""
    logger.debug("Called - file_path: %s, target_size: %d", file_path, target_size)
    try:
        with zipfile.ZipFile(file_path) as zf:
            names = zf.namelist()
            if "mergedimage.png" in names:
                entry = "mergedimage.png"
            elif "preview.png" in names:
                entry = "preview.png"
            else:
                logger.warning("KRA has no merged or preview image")
                return None

            with zf.open(entry) as f:
                data = f.read()
    except (OSError, zipfile.BadZipFile, KeyError) as exc:
        logger.warning("Failed to read KRA file %s: %s", file_path, exc)
        return None

    try:
        img: Image.Image = Image.open(BytesIO(data))
        img = ImageOps.exif_transpose(img) or img
        if img.mode not in ("RGBA", "RGB"):
            img = img.convert("RGBA")
        if target_size is not None:
            img.thumbnail((target_size, target_size))
        return img
    except (OSError, ValueError) as exc:
        logger.warning("Failed to decode KRA image %s: %s", file_path, exc)
        return None
    finally:
        logger.debug("End - file_path: %s", file_path)
