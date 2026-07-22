"""Cache management, paths, UUIDs, saving, and invalidation for thumbnail cache."""

from __future__ import annotations

import logging
import uuid as uuid_mod
from pathlib import Path
from typing import Any

from PIL import Image

from tarragon.app_paths import cache_dir

logger = logging.getLogger(__name__)

# Resolution tiers for organized cache structure
MASTER_LONG_EDGE = 2048
RESOLUTION_THUMBNAIL = 256
RESOLUTION_PREVIEW = 1024
RESOLUTION_FULL = None  # Original resolution


def generate_cache_uuid() -> str:
    """Generate a short UUID for cache organization.

    Returns an 8-character hex string derived from a UUID4, providing
    sufficient uniqueness for cache folder naming while keeping paths
    short and human-readable.
    """
    return uuid_mod.uuid4().hex[:8]  # 8 character hex string


def generate_cache_paths(source_path: Path, cache_uuid: str) -> dict[str, Path]:
    """Generate cache paths for all resolutions.

    Structure: ``cache/{resolution}/{folder_name}_{uuid}/{filename}``

    Parameters
    ----------
    source_path:
        The original source image path.  The parent folder name and file
        stem are extracted to build human-readable cache filenames.
    cache_uuid:
        A short UUID string (see :func:`generate_cache_uuid`) used to
        uniquely identify this cache entry.

    Returns
    -------
    dict[str, Path]
        Mapping of resolution tier names to their cache file paths.
        Keys: ``'256'``, ``'1024'``, ``'full'``.

    Notes
    -----
    Directories are created on demand (``mkdir(parents=True, exist_ok=True)``).
    Uses :func:`tarragon.app_paths.cache_dir` as the cache root.
    """
    # Extract folder name and filename
    folder_name = source_path.parent.name
    filename = source_path.stem

    # Create base directory: cache/{folder_name}_{uuid}
    base_name = f"{folder_name}_{cache_uuid}"

    # Generate paths for each resolution
    paths: dict[str, Path] = {}
    for resolution in (str(RESOLUTION_THUMBNAIL), str(RESOLUTION_PREVIEW), "full"):
        resolution_dir = cache_dir() / resolution / base_name
        resolution_dir.mkdir(parents=True, exist_ok=True)
        paths[resolution] = resolution_dir / f"{filename}.png"

    return paths


def invalidate_cache_files(db: Any, source_path: str) -> None:
    """Delete cached thumbnail files from disk and remove the DB record.

    Looks up the cache record for *source_path* in *db*, deletes the
    actual PNG files for all three resolution tiers (256px, 1024px, full),
    then removes the database row.

    Parameters
    ----------
    db:
        A :class:`~tarragon.db.Database` instance (or compatible mock).
    source_path:
        The original source file path (as a string).

    Notes
    -----
    Uses ``Path.unlink(missing_ok=True)`` so missing files do not raise.
    If no DB record exists for *source_path*, the function is a no-op.
    """
    cached = db.get_thumbnail(source_path)
    if cached is None:
        logger.debug("invalidate_cache_files: no DB record for %s", source_path)
        return

    deleted_paths: list[str] = []
    for key in ("thumbnail_cache_path", "preview_cache_path", "full_cache_path"):
        cache_file = cached.get(key)
        if cache_file:
            Path(cache_file).unlink(missing_ok=True)
            deleted_paths.append(cache_file)

    db.delete_thumbnail(source_path)
    logger.info(
        "invalidate_cache_files: deleted %d cache file(s) for %s",
        len(deleted_paths),
        source_path,
    )


def save_to_cache(img: Image.Image, cache_path: Path, format_setting: str = "png") -> None:
    """Write a rendered thumbnail to the cache directory.

    The cache directory (``cache_path.parent``) is created on demand.

    Decision A: Format is configurable via *format_setting*:
    * ``"png"`` (default): lossless, handles RGBA directly.
    * ``"jpeg"``: smaller files; RGBA images are flattened onto a
      white background before saving.
    """
    cache_path.parent.mkdir(parents=True, exist_ok=True)

    if format_setting == "jpeg":
        # Flatten RGBA onto white background, JPEG does not support alpha.
        if img.mode == "RGBA":
            background = Image.new("RGB", img.size, (255, 255, 255))
            background.paste(img, mask=img.split()[3])
            rgb_img = background
        else:
            rgb_img = img.convert("RGB")
        rgb_img.save(cache_path, "JPEG", quality=90)
    else:
        # PNG default per Decision A.  Handles all modes including RGBA.
        img.save(cache_path, "PNG")


def derive_smaller_sizes(source_image: Image.Image, target_sizes: list[int]) -> dict[int, Image.Image]:
    """Derive smaller image sizes from a source image.

    For each size in *target_sizes*, if the source image's longest side
    exceeds *size*, a copy is shrunk via Lanczos resampling.  When the
    source is already smaller than or equal to *size*, a copy is included
    as-is (no upscaling) so that all cache tiers are populated.

    Parameters
    ----------
    source_image:
        The full-resolution source PIL Image.
    target_sizes:
        List of target long-edge pixel sizes (e.g. ``[256, 1024]``).

    Returns
    -------
    dict[int, Image.Image]
        Mapping of target_size -> derived Image.  All requested sizes
        are included, either resized down or copied as-is.
    """
    results: dict[int, Image.Image] = {}
    for size in target_sizes:
        if max(source_image.size) > size:
            derived = source_image.copy()
            derived.thumbnail((size, size), Image.Resampling.LANCZOS)
            results[size] = derived
        else:
            # Image is smaller than or equal to target. Include as-is
            results[size] = source_image.copy()
    return results
