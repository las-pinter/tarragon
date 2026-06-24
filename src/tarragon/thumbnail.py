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

__all__ = ["render_plain_image", "render_psd_image", "save_to_cache"]

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


def _composite_psd_in_process(file_path_str: str) -> bytes | None:
    """Composite a PSD/PSB file inside a sub-process worker.

    This is a **module-level** function so that it can be pickled across
    process boundaries via ``ProcessPoolExecutor``.

    Returns raw PNG bytes on success, or ``None`` on any failure (failure
    isolation — never crash the worker).
    """
    file_path = Path(file_path_str)
    try:
        from psd_tools import PSDImage  # Import inside try — worker may lack psd_tools

        psd = PSDImage.open(file_path)
        canvas_mp = (psd.width * psd.height) / 1_000_000

        if canvas_mp <= 20.0:
            # Direct composite for small canvases
            image = psd.composite(force=True)
        else:
            # Tiled composite for large canvases (2x2 grid)
            w, h = psd.width, psd.height
            tw = max(1, w // 2)
            th = max(1, h // 2)
            target = Image.new("RGBA", (w, h), (0, 0, 0, 0))
            for tile_y in range(2):
                for tile_x in range(2):
                    x1 = tile_x * tw
                    y1 = tile_y * th
                    x2 = min((tile_x + 1) * tw, w)
                    y2 = min((tile_y + 1) * th, h)
                    try:
                        tile_img = psd.composite(viewport=(x1, y1, x2, y2), force=True)
                        target.paste(tile_img, (x1, y1))
                    except Exception:
                        # Skip problematic tiles — partial render is better than crash
                        pass
            image = target

        # Resize to master long edge if needed
        if max(image.size) > MASTER_LONG_EDGE:
            image.thumbnail((MASTER_LONG_EDGE, MASTER_LONG_EDGE), Image.LANCZOS)

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


def _get_executor() -> ProcessPoolExecutor:
    """Get or create the shared ``ProcessPoolExecutor`` singleton."""
    global _shared_executor
    if _shared_executor is None:
        with _executor_lock:
            if _shared_executor is None:  # double-checked locking
                worker_count = _compute_worker_count()
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


def render_psd_image(file_path: Path) -> Image.Image | None:
    """Dispatch PSD compositing via ``ProcessPoolExecutor``.

    Returns a PIL ``Image`` or ``None`` on failure (2-minute timeout).
    """
    executor = _get_executor()
    future = executor.submit(_composite_psd_in_process, str(file_path.resolve()))
    try:
        result_bytes = future.result(timeout=120)  # 2 minute timeout
        if result_bytes is not None:
            return Image.open(io.BytesIO(result_bytes))
        return None
    except Exception:
        return None
