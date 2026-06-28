"""Plain image render pipeline — thumbnail generation for JPEG, PNG, WebP, TIFF, PSD, PSB."""

from __future__ import annotations

import atexit
import hashlib
import io
import threading
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import psutil
from PIL import Image, ImageOps

from tarragon.app_paths import cache_dir

__all__ = [
    "derive_smaller_sizes",
    "generate_cache_paths",
    "generate_cache_uuid",
    "render_plain_image",
    "render_psd_image",
    "save_to_cache",
]

MASTER_LONG_EDGE = 2048

# Resolution tiers for organized cache structure
RESOLUTION_THUMBNAIL = 256
RESOLUTION_PREVIEW = 1024
RESOLUTION_FULL = None  # Original resolution


_FORMAT_MAP: dict[str, tuple[str, str]] = {
    "png": (".png", "image/png"),
    "jpeg": (".jpg", "image/jpeg"),
}


# DEPRECATED: Use generate_cache_paths() for the new organized cache structure.
# Kept for backward compatibility until all callers are migrated.
def _cache_file_path(file_abs_path: Path, cache_format: str = "png") -> tuple[Path, str]:
    """Return (cache_filepath, mime_type_string) for a given source file path.

    .. deprecated::
        Use :func:`generate_cache_paths` for the new organized cache structure
        (``cache/{resolution}/{folder_name}_{uuid}/{filename}``).

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


def generate_cache_uuid() -> str:
    """Generate a short UUID for cache organization.

    Returns an 8-character hex string derived from a UUID4, providing
    sufficient uniqueness for cache folder naming while keeping paths
    short and human-readable.
    """
    import uuid

    return uuid.uuid4().hex[:8]  # 8 character hex string


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
    for resolution in ("256", "1024", "full"):
        resolution_dir = cache_dir() / resolution / base_name
        resolution_dir.mkdir(parents=True, exist_ok=True)
        paths[resolution] = resolution_dir / f"{filename}.png"

    return paths


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


def derive_smaller_sizes(source_image: Image.Image, target_sizes: list[int]) -> dict[int, Image.Image]:
    """Derive smaller image sizes from a source image.

    For each size in *target_sizes*, if the source image's longest side
    exceeds *size*, a copy is shrunk via Lanczos resampling and included
    in the result.  Sizes that are >= the source's longest side are skipped
    (no upscaling).

    Parameters
    ----------
    source_image:
        The full-resolution source PIL Image.
    target_sizes:
        List of target long-edge pixel sizes (e.g. ``[256, 1024]``).

    Returns
    -------
    dict[int, Image.Image]
        Mapping of target_size -> derived Image.  Only sizes smaller than
        the source are included.
    """
    results: dict[int, Image.Image] = {}
    for size in target_sizes:
        if max(source_image.size) > size:
            derived = source_image.copy()
            derived.thumbnail((size, size), Image.LANCZOS)
            results[size] = derived
    return results


# =========================================================================
# PSD / PSB compositing pipeline
# =========================================================================


def _compute_worker_count(manual_override: int | None = None) -> int:
    """Compute PSD worker count based on available RAM.

    Default 3, adaptive: max(1, min(available_ram // 200MB, 8)), clamped [1, 8].
    If *manual_override* is provided, it is clamped to [1, 8] and returned.
    """
    if manual_override is not None:
        return max(1, min(manual_override, 8))
    available = psutil.virtual_memory().available
    return max(1, min(available // 200_000_000, 8))


def _composite_psd_in_process(
    file_path_str: str,
    large_canvas_threshold_mp: float,
    tile_grid_x: int,
    tile_grid_y: int,
    target_size: int | None = None,
) -> bytes | None:
    """Composite a PSD/PSB file inside a sub-process worker.

    This is a **module-level** function so that it can be pickled across
    process boundaries via ``ProcessPoolExecutor``.

    *large_canvas_threshold_mp* and tile grid dimensions (*tile_grid_x*,
    *tile_grid_y*) are passed as plain scalars so they survive pickling.

    Parameters
    ----------
    target_size:
        If specified, shrink the composited image so the longest side is
        at most *target_size* pixels.  If ``None``, no resizing is performed
        (full resolution output).

    Returns raw PNG bytes on success, or ``None`` on any failure (failure
    isolation — never crash the worker).
    """
    file_path = Path(file_path_str)
    try:
        from psd_tools import PSDImage  # Import inside try — worker may lack psd_tools

        psd = PSDImage.open(file_path)
        canvas_mp = (psd.width * psd.height) / 1_000_000

        if canvas_mp <= large_canvas_threshold_mp:
            # Direct composite for small canvases
            image = psd.composite(force=True)
        else:
            # Tiled composite for large canvases
            w, h = psd.width, psd.height
            tw = max(1, w // tile_grid_x)
            th = max(1, h // tile_grid_y)
            target = Image.new("RGBA", (w, h), (0, 0, 0, 0))
            for tile_y in range(tile_grid_y):
                for tile_x in range(tile_grid_x):
                    x1 = tile_x * tw
                    y1 = tile_y * th
                    x2 = min((tile_x + 1) * tw, w)
                    y2 = min((tile_y + 1) * th, h)
                    try:
                        tile_img = psd.composite(viewport=(x1, y1, x2, y2), force=True)
                        target.paste(tile_img, (x1, y1))
                        del tile_img
                    except Exception:
                        # Skip problematic tiles — partial render is better than crash
                        pass
            image = target

        # Resize to target_size if specified
        if target_size is not None and max(image.size) > target_size:
            image.thumbnail((target_size, target_size), Image.LANCZOS)

        # Return as PNG bytes (PIL Image is not picklable)
        import io

        buf = io.BytesIO()
        image.save(buf, "PNG")
        return buf.getvalue()
    except Exception:
        return None  # Failure isolation — never crash the worker


# Shared ``ProcessPoolExecutor`` singleton
_shared_executor: ProcessPoolExecutor | None = None
_executor_lock = threading.Lock()


def _get_executor(max_workers: int | None = None) -> ProcessPoolExecutor:
    """Get or create the shared ``ProcessPoolExecutor`` singleton.

    On first creation, *max_workers* is forwarded to ``_compute_worker_count``
    so that user-configured overrides (e.g. from the settings table) take
    effect.  Subsequent calls ignore *max_workers* because the singleton
    already exists.
    """
    global _shared_executor
    if _shared_executor is None:
        with _executor_lock:
            if _shared_executor is None:  # double-checked locking
                worker_count = _compute_worker_count(max_workers)
                _shared_executor = ProcessPoolExecutor(max_workers=worker_count)
                atexit.register(_shutdown_executor)
    return _shared_executor


def _shutdown_executor() -> None:
    """Shut down the shared executor on exit."""
    global _shared_executor
    with _executor_lock:
        if _shared_executor is not None:
            _shared_executor.shutdown(wait=False)
            _shared_executor = None


def render_psd_image(
    file_path: Path,
    large_canvas_threshold_mp: float,
    tile_grid_x: int,
    tile_grid_y: int,
    target_size: int | None = None,
) -> Image.Image | None:
    """Dispatch PSD compositing via ``ProcessPoolExecutor``.

    Returns a PIL ``Image`` or ``None`` on failure (2-minute timeout).

    *large_canvas_threshold_mp* and tile grid dimensions are forwarded to
    the subprocess worker as plain scalars.

    Parameters
    ----------
    target_size:
        If specified, the composited image is shrunk so the longest side
        is at most *target_size* pixels.  If ``None``, no resizing is
        performed (full resolution output).
    """
    executor = _get_executor()
    future = executor.submit(
        _composite_psd_in_process,
        str(file_path.resolve()),
        large_canvas_threshold_mp,
        tile_grid_x,
        tile_grid_y,
        target_size,
    )
    try:
        result_bytes = future.result(timeout=120)  # 2 minute timeout
        if result_bytes is not None:
            return Image.open(io.BytesIO(result_bytes))
        return None
    except Exception:
        return None
