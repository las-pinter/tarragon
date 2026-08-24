"""Cache management, paths, UUIDs, saving, and invalidation for thumbnail cache."""

from __future__ import annotations

import logging
import uuid as uuid_mod
from pathlib import Path
from typing import Any

from PIL import Image

from tarragon.app_paths import cache_dir, data_dir

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
    logger.debug("Called - source_path: %s", source_path)
    cached = db.get_thumbnail(source_path)
    if cached is None:
        logger.debug("No DB record for %s", source_path)
        return

    deleted_paths: list[str] = []
    for key in ("thumbnail_cache_path", "preview_cache_path", "full_cache_path"):
        cache_file = cached.get(key)
        if cache_file:
            Path(cache_file).unlink(missing_ok=True)
            deleted_paths.append(cache_file)

    db.delete_thumbnail(source_path)
    logger.info(
        "deleted %d cache file(s) for %s",
        len(deleted_paths),
        source_path,
    )


def clear_full_res_cache(enabled: bool = True) -> None:
    """Delete all files under the full-resolution cache tier and prune empty dirs.

    Removes every file under ``cache_dir()/full`` and then prunes the
    now-empty ``full/{folder}_{uuid}`` subdirectories bottom-up.  The
    ``full`` root directory itself is kept.  No database rows are touched.

    Parameters
    ----------
    enabled:
        When False, the function is a no-op.  Used to honor the
        ``clear_full_res_on_exit`` setting.

    Notes
    -----
    The resolved target directory name is verified to be exactly ``full``
    before any deletion, so a misconfigured custom cache dir can never
    cause the wrong tree to be removed.  Unlink failures (e.g. permission
    errors) are logged and skipped so one file cannot abort the cleanup.
    """
    if not enabled:
        logger.debug("Skipping full-res cache cleanup (disabled)")
        return

    full_dir = (cache_dir() / "full").resolve()
    if full_dir.name != "full":
        logger.error("Refusing to clear cache: expected a directory named 'full', got %s", full_dir)
        return
    if not full_dir.exists():
        logger.debug("Full-res cache dir does not exist: %s", full_dir)
        return

    _delete_tree_contents(full_dir)
    logger.debug("Cleared full-res cache: %s", full_dir)


def _delete_tree_contents(root: Path) -> None:
    """Delete all files and symlinks under *root* and prune empty dirs bottom-up.

    The root directory itself is kept.  Unlink failures (e.g. permission
    errors) are logged and skipped so one file cannot abort the cleanup.
    """
    for path in root.rglob("*"):
        if path.is_file() or path.is_symlink():
            try:
                path.unlink(missing_ok=True)
            except PermissionError:
                logger.warning("Failed to remove cache file: %s", path, exc_info=True)

    # Prune empty subdirectories bottom-up (deepest first).
    for path in sorted(root.rglob("*"), key=lambda p: len(p.parts), reverse=True):
        if path.is_dir():
            try:
                path.rmdir()
            except OSError:
                pass


def _is_safe_cache_dir(path: Path) -> bool:
    """Return True when *path* is safe to purge recursively.

    Refuses filesystem roots, the user's home directory (or any ancestor
    of it), and the data directory (or any ancestor of it) so a
    misconfigured custom cache dir can never cause the wrong tree to be
    deleted.
    """
    resolved = path.resolve()
    if resolved == Path(resolved.anchor):
        return False
    home = Path.home().resolve()
    if resolved == home or home.is_relative_to(resolved):
        return False
    data = data_dir().resolve()
    if resolved == data or data.is_relative_to(resolved):
        return False
    return True


def clear_cache() -> None:
    """Delete all files under the cache directory and prune empty dirs.

    Removes every file under ``cache_dir()`` (all resolution tiers) and
    prunes the now-empty subdirectories bottom-up.  The cache root
    directory itself is kept.  No database rows are touched.

    Notes
    -----
    The resolved cache directory is checked by :func:`_is_safe_cache_dir`
    before any deletion so a misconfigured custom cache dir can never
    cause the wrong tree to be removed.
    """
    cache_root = cache_dir().resolve()
    if not _is_safe_cache_dir(cache_root):
        logger.error("Refusing to clear cache: unsafe cache dir: %s", cache_root)
        return
    if not cache_root.exists():
        logger.debug("Cache dir does not exist: %s", cache_root)
        return
    _delete_tree_contents(cache_root)
    logger.debug("Cleared cache: %s", cache_root)


def compute_cache_size_bytes() -> int:
    """Return the total on-disk size of the thumbnail cache in bytes.

    Walks the entire cache tree under ``cache_dir()`` and sums the sizes
    of all regular files.  Returns 0 when the cache directory does not
    exist or contains no files.
    """
    cache_root = cache_dir()
    if not cache_root.exists():
        return 0
    return sum(p.stat().st_size for p in cache_root.rglob("*") if p.is_file())


def save_to_cache(img: Image.Image, cache_path: Path, format_setting: str = "PNG") -> None:
    """Write a rendered thumbnail to the cache directory.

    The cache directory (``cache_path.parent``) is created on demand.

    Decision A: Format is configurable via *format_setting*:
    * ``"PNG"`` (default): lossless, handles RGBA directly.
    * ``"JPEG"``: smaller files; RGBA images are flattened onto a
      white background before saving.
    """
    logger.debug("Called - cache_path: %s, format_setting: %s", cache_path, format_setting)
    cache_path.parent.mkdir(parents=True, exist_ok=True)

    if format_setting.upper() == "JPEG":
        # Convert to RGB for JPEG — it does not support alpha or non-RGB modes.
        if img.mode in ("RGBA", "LA", "PA"):
            # Flatten alpha-bearing modes onto a white background.
            rgba_img = img.convert("RGBA")
            background = Image.new("RGB", rgba_img.size, (255, 255, 255))
            background.paste(rgba_img, mask=rgba_img.split()[3])
            rgb_img = background
        else:
            # L, P, or already RGB — convert to RGB (no-op for RGB).
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
