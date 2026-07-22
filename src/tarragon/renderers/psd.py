"""PSD / PSB compositing. subprocess-based rendering with a shared process pool."""

from __future__ import annotations

import atexit
import io
import logging
import threading
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import psutil
from PIL import Image

logger = logging.getLogger(__name__)


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
    isolation never crash the worker).
    """
    file_path = Path(file_path_str)
    try:
        from psd_tools import PSDImage  # Import inside, worker may lack psd_tools

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
                        # Skip problematic tiles, partial render is better than crash
                        pass
            image = target

        # Resize to target_size if specified
        if target_size is not None and max(image.size) > target_size:
            image.thumbnail((target_size, target_size), Image.Resampling.LANCZOS)

        # Return as PNG bytes (PIL Image is not picklable)
        buf = io.BytesIO()
        image.save(buf, "PNG")
        return buf.getvalue()
    except Exception:
        logger.warning("Rendering failed for %s", file_path_str)
        return None  # Failure isolation, never crash the worker


# Shared ``ProcessPoolExecutor`` singleton
_shared_executor: ProcessPoolExecutor | None = None
_executor_lock = threading.Lock()


def get_executor(max_workers: int | None = None) -> ProcessPoolExecutor:
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
                atexit.register(shutdown_executor)
    return _shared_executor


def shutdown_executor() -> None:
    """Shut down the shared executor on exit."""
    global _shared_executor
    with _executor_lock:
        if _shared_executor is not None:
            # Try graceful shutdown first
            try:
                _shared_executor.shutdown(wait=False, cancel_futures=True)
            except Exception:
                # If graceful shutdown fails, force terminate
                pass
            # Forcefully terminate any remaining worker processes
            # This is necessary because workers may be stuck in system calls
            # that can't be interrupted by cancel_futures
            try:
                if hasattr(_shared_executor, "_processes"):
                    for process in _shared_executor._processes.values():
                        if process.is_alive():
                            process.terminate()
            except Exception:
                # Ignore errors during termination
                pass
            _shared_executor = None


def render_psd_image(
    file_path: Path,
    large_canvas_threshold_mp: float,
    tile_grid_x: int,
    tile_grid_y: int,
    target_size: int | None = None,
    cancel_event: threading.Event | None = None,
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
    cancel_event:
        Optional ``threading.Event`` for cooperative cancellation.  When
        set, the future is cancelled and ``None`` is returned immediately.
        The event is polled every 500 ms while waiting for the subprocess.
    """
    executor = get_executor()
    future = executor.submit(
        _composite_psd_in_process,
        str(file_path.resolve()),
        large_canvas_threshold_mp,
        tile_grid_x,
        tile_grid_y,
        target_size,
    )
    try:
        # Poll with cancellation support instead of blocking for 120 s.
        while not future.done():
            if cancel_event is not None and cancel_event.is_set():
                future.cancel()
                return None
            try:
                result_bytes = future.result(timeout=0.5)
                if result_bytes is not None:
                    return Image.open(io.BytesIO(result_bytes))
                return None
            except TimeoutError:
                continue
        # Future completed, retrieve the result.
        result_bytes = future.result()
        if result_bytes is not None:
            return Image.open(io.BytesIO(result_bytes))
        return None
    except Exception:
        return None
