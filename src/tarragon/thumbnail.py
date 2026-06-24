"""Plain image render pipeline — thumbnail generation for JPEG, PNG, WebP, TIFF."""

from __future__ import annotations

import hashlib
from pathlib import Path

from PIL import Image, ImageOps

from tarragon.app_paths import cache_dir

__all__ = ["render_plain_image", "save_to_cache"]

MASTER_LONG_EDGE = 2048


_FORMAT_MAP: dict[str, tuple[str, str]] = {
    "png": (".png", "image/png"),
    "jpeg": (".jpg", "image/jpeg"),
}


def _cache_file_path(file_abs_path: Path, cache_format: str = "png") -> tuple[Path, str]:
    """Return (cache_filepath, mime_type_string) for a given source file path.

    The cache filename is derived from a SHA-1 hash of the source file's
    absolute path, making it deterministic and unique per path.  The file
    extension and MIME type depend on the *cache_format* setting.

    Decision A — Format is configurable via settings (``"png"`` or ``"jpeg"``).
    Default is ``"png"`` for simplicity and lossless storage.
    """
    mapping = _FORMAT_MAP.get(cache_format)
    if mapping is None:
        raise ValueError(f"Unknown cache_format: {cache_format!r}. Expected one of {list(_FORMAT_MAP)}")
    ext, mime = mapping
    sha = hashlib.sha1(str(file_abs_path.resolve()).encode()).hexdigest()
    return cache_dir() / f"{sha}{ext}", mime


def render_plain_image(file_path: Path) -> Image.Image | None:
    """Open a plain image, make it display-safe, and resize to the master long edge.

    Steps
    -----
    1. Open the file with Pillow (supports JPEG, PNG, WebP, TIFF, …).
    2. Convert non-RGB/RGBA modes to RGBA so the rest of the pipeline sees
       a consistent pixel format.
    3. Shrink the image in-place so the longest side is at most
       *MASTER_LONG_EDGE* (2048 px), preserving aspect ratio.

    Returns *None* (instead of raising) when the file cannot be opened or
    decoded — the caller should treat ``None`` as “skip / use placeholder”.
    """
    try:
        img = Image.open(file_path)
        img = ImageOps.exif_transpose(img) or img  # Apply EXIF rotation
        if img.mode not in ("RGBA", "RGB"):
            img = img.convert("RGBA")
        img.thumbnail((MASTER_LONG_EDGE, MASTER_LONG_EDGE), Image.LANCZOS)
        return img
    except (OSError, ValueError):
        return None


def save_to_cache(img: Image.Image, cache_path: Path, format_setting: str = "png") -> None:
    """Write a rendered thumbnail to the cache directory.

    The cache directory (``cache_path.parent``) is created on demand.

    Decision A — Format is configurable via *format_setting*:
    * ``"png"`` (default) — lossless, handles RGBA directly.
    * ``"jpeg"``           — smaller files; RGBA images are flattened onto a
      white background before saving.
    """
    if img is None:
        raise TypeError("save_to_cache() requires a valid PIL Image, got None")
    cache_path.parent.mkdir(parents=True, exist_ok=True)

    if format_setting == "jpeg":
        # Flatten RGBA onto white background — JPEG does not support alpha.
        if img.mode == "RGBA":
            background = Image.new("RGB", img.size, (255, 255, 255))
            background.paste(img, mask=img.split()[3])
            rgb_img = background
        else:
            rgb_img = img.convert("RGB")
        rgb_img.save(cache_path, "JPEG", quality=90)
    else:
        # PNG — default per Decision A.  Handles all modes including RGBA.
        img.save(cache_path, "PNG")
