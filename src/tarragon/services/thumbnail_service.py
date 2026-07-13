"""Thumbnail service — async orchestration for thumbnail generation and caching."""

from __future__ import annotations

import logging
import threading
import time
from pathlib import Path
from typing import Any, Callable

from PIL import Image
from PySide6.QtCore import QObject, QRunnable, QThreadPool, Signal, Slot

from tarragon.db import Database
from tarragon.scanner import FileInfo
from tarragon.services.settings_service import SettingsService
from tarragon.renderers.psd import _get_executor
from tarragon.thumbnail import (
    RESOLUTION_FULL,
    RESOLUTION_PREVIEW,
    RESOLUTION_THUMBNAIL,
    derive_smaller_sizes,
    generate_cache_paths,
    generate_cache_uuid,
    invalidate_cache_files,
    render_clip_image,
    render_plain_image,
    render_psd_image,
    save_to_cache,
)

logger = logging.getLogger(__name__)


class _RenderAllTask(QRunnable):
    """Runs multi-resolution rendering in a QThreadPool worker thread."""

    def __init__(
        self,
        file_info: FileInfo,
        on_error: Callable[..., Any],
        render_func: Callable[..., Any],
        cancel_event: threading.Event | None = None,
    ) -> None:
        super().__init__()
        self._file_info = file_info
        self._on_error = on_error
        self._render_func = render_func
        self._cancel_event = cancel_event

    def run(self) -> None:
        """Execute render in worker thread."""
        # Check cancellation before starting expensive work.
        if self._cancel_event is not None and self._cancel_event.is_set():
            logger.debug("_RenderAllTask cancelled before start: %s", self._file_info.path)
            return
        try:
            self._render_func(self._file_info)
        except Exception as exc:
            self._on_error(self._file_info, str(exc))


class ThumbnailService(QObject):
    """Coordinates thumbnail generation, caching, and UI signal emission.

    Owns the QThreadPool for plain image renders and delegates PSD/PSB
    compositing to the module-level ProcessPoolExecutor shared singleton.
    """

    thumbnailReady = Signal(str, object, object, object)  # noqa: N815 — (path, image, resolution_size, cache_path)
    errorOccurred = Signal(str, str)  # noqa: N815 — Qt signal follows camelCase convention
    tagsUpdated = Signal()  # noqa: N815 — emitted after auto-color tags are persisted

    def __init__(self, db: Database, settings_service: SettingsService, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._db = db
        self._settings_service = settings_service
        self._cancel_event = threading.Event()
        self._threadpool = QThreadPool()
        self._cache_format: str = self._settings_service.get_cache_format()

        # Pre-initialize the shared PSD ProcessPoolExecutor with the
        # user-configured worker count (falls back to RAM-adaptive default
        # when the setting is absent / None).
        max_psd_workers = self._settings_service.get_max_psd_workers()
        _get_executor(max_workers=max_psd_workers)

    def cancel_pending(self) -> None:
        """Cancel all pending thumbnail generation.

        Sets the cancel event (signalling in-flight tasks to abort) and
        clears the QThreadPool of queued-but-not-started runnables.
        """
        self._cancel_event.set()
        self._threadpool.clear()
        logger.debug("Cancelled pending thumbnail generation")

    def reset_cancel(self) -> None:
        """Reset the cancel flag so new tasks can proceed.

        Call this when starting a new folder scan after ``cancel_pending()``.
        """
        self._cancel_event.clear()

    def shutdown(self, timeout_ms: int = 5000) -> None:
        """Graceful shutdown — cancel pending tasks, wait for running ones.

        Args:
            timeout_ms: Maximum time to wait for in-flight tasks to finish
                (milliseconds).  Defaults to 5 000 (5 seconds).
        """
        self.cancel_pending()
        # Shut down the ProcessPoolExecutor first to cancel any pending PSD renders.
        # This prevents worker processes from blocking on queue.get() indefinitely.
        # Deferred import to avoid circular dependency
        from tarragon.renderers.psd import _shutdown_executor

        _shutdown_executor()
        self._threadpool.waitForDone(timeout_ms)
        logger.debug("ThumbnailService shutdown complete")

    @Slot(FileInfo)
    def check_and_render(self, file_info: FileInfo) -> str:
        """Check cache; if stale or missing, render all resolutions.

        Called from the main thread for each file discovered during a scan.
        Handles three resolution tiers: thumbnail (256), preview (1024),
        and full (original resolution).

        Returns a status string for batch summary logging:
            "cached"  — all resolutions served from cache
            "derived" — missing resolutions derived from existing cached image
            "queued"  — async render dispatched to thread pool
        """
        start = time.perf_counter()
        logger.debug("check_and_render: %s", file_info.path)
        cached = self._db.get_thumbnail(str(file_info.path))

        # Auto-regeneration: When source file mtime or size changes,
        # the cache is considered stale and triggers a full re-render.
        # This happens automatically on folder re-scan — no manual
        # invalidation needed. The comparison uses int(mtime) to match
        # the integer precision stored in the database.
        # Check if cache is valid (mtime + size match)
        if cached and cached["mtime"] == int(file_info.mtime) and cached["size"] == file_info.size:
            # Check if we have all three resolutions
            has_thumbnail = cached.get("thumbnail_cache_path") and Path(cached["thumbnail_cache_path"]).exists()
            has_preview = cached.get("preview_cache_path") and Path(cached["preview_cache_path"]).exists()
            has_full = cached.get("full_cache_path") and Path(cached["full_cache_path"]).exists()

            if has_thumbnail and has_preview and has_full:
                # All resolutions cached — emit signals for each
                self._emit_cached_thumbnails(file_info, cached)
                elapsed = time.perf_counter() - start
                logger.debug("check_and_render completed in %.3fs: status=cached", elapsed)
                return "cached"

            # Some resolutions missing — derive from largest available
            result = self._derive_missing_resolutions(file_info, cached)
            elapsed = time.perf_counter() - start
            logger.debug("check_and_render completed in %.3fs: status=%s", elapsed, result)
            return result

        # Cache miss or invalid — render all resolutions from source (async)
        task = _RenderAllTask(
            file_info=file_info,
            on_error=self._on_error,
            render_func=self._render_all_resolutions,
            cancel_event=self._cancel_event,
        )
        self._threadpool.start(task)
        logger.debug("Queued render for %s", file_info.path)
        elapsed = time.perf_counter() - start
        logger.debug("check_and_render completed in %.3fs: status=queued", elapsed)
        return "queued"

    def invalidate_and_render(self, source_path: Path) -> None:
        """Delete cached thumbnails and re-render from source.

        Invalidates all cache files for *source_path* (deletes PNGs from
        disk and removes the DB record), then triggers a fresh render
        via :meth:`check_and_render`.

        Parameters
        ----------
        source_path:
            Path to the original source image file.

        Notes
        -----
        If the source file does not exist on disk, the method logs a
        warning and returns without rendering.
        """
        logger.info("Regenerating thumbnail for: %s", source_path)

        # Delete existing cache files and DB record
        invalidate_cache_files(self._db, str(source_path))

        # Stat the file to build a FileInfo for re-rendering
        try:
            stat = source_path.stat()
        except OSError:
            logger.warning("invalidate_and_render: source file not found: %s", source_path)
            return

        file_info = FileInfo(
            path=source_path,
            mtime=stat.st_mtime,
            size=stat.st_size,
            extension=source_path.suffix.lower(),
        )

        # Re-render from scratch (cache was just invalidated)
        self.check_and_render(file_info)

    def _emit_cached_thumbnails(self, file_info: FileInfo, cached: dict[str, Any]) -> None:
        """Emit thumbnailReady signals for all cached resolutions."""
        for resolution_key, resolution_size in [
            ("thumbnail_cache_path", RESOLUTION_THUMBNAIL),
            ("preview_cache_path", RESOLUTION_PREVIEW),
            ("full_cache_path", RESOLUTION_FULL),
        ]:
            cache_path = cached.get(resolution_key)
            if cache_path and Path(cache_path).exists():
                try:
                    img = Image.open(cache_path)
                    self.thumbnailReady.emit(str(file_info.path), img, resolution_size, cache_path)
                except Exception:
                    logger.warning(
                        "Corrupt cache file, skipping resolution %s: %s", resolution_size, cache_path, exc_info=True
                    )

    def _save_and_record(
        self,
        img: Image.Image,
        file_info: FileInfo,
        resolution_size: int,
        cache_path: Path,
    ) -> str:
        """Save *img* to cache, emit thumbnailReady, and return the path string.

        Shared helper used by both :meth:`_derive_missing_resolutions` and
        :meth:`_render_all_resolutions` to avoid duplicating the
        save → emit → record pattern.
        """
        save_to_cache(img, cache_path, self._cache_format)
        path_str = str(cache_path)
        self.thumbnailReady.emit(str(file_info.path), img, resolution_size, path_str)
        return path_str

    def _derive_missing_resolutions(self, file_info: FileInfo, cached: dict[str, Any]) -> str:
        """Derive missing smaller resolutions from the largest available cached image.

        Returns a status string: "derived" or "queued".
        """
        # Find largest available resolution
        full_path = cached.get("full_cache_path")
        preview_path = cached.get("preview_cache_path")

        source_image: Image.Image | None = None
        source_resolution: int | None = None

        if full_path and Path(full_path).exists():
            try:
                source_image = Image.open(full_path)
                source_resolution = RESOLUTION_FULL  # Full resolution
            except Exception:
                logger.warning("Failed to open cached full resolution: %s", full_path, exc_info=True)

        if source_image is None and preview_path and Path(preview_path).exists():
            try:
                source_image = Image.open(preview_path)
                source_resolution = RESOLUTION_PREVIEW
            except Exception:
                logger.warning("Failed to open cached preview resolution: %s", preview_path, exc_info=True)

        if source_image is None:
            # No cached image available — render from source (async)
            task = _RenderAllTask(
                file_info=file_info,
                on_error=self._on_error,
                render_func=self._render_all_resolutions,
                cancel_event=self._cancel_event,
            )
            self._threadpool.start(task)
            logger.debug("Queued render for %s", file_info.path)
            return "queued"

        # Derive missing sizes — use per-folder UUID from DB (atomic)
        folder_path = str(file_info.path.parent)
        candidate_uuid = cached.get("cache_uuid") or generate_cache_uuid()
        cache_uuid = self._db.get_or_create_folder_uuid(folder_path, candidate_uuid)
        cache_paths = generate_cache_paths(file_info.path, cache_uuid)

        # Track which paths to write to DB (preserve existing, add new)
        final_thumb_path = cached.get("thumbnail_cache_path")
        final_preview_path = cached.get("preview_cache_path")
        final_full_path = cached.get("full_cache_path")

        # Save missing thumbnail (RESOLUTION_THUMBNAIL)
        if not cached.get("thumbnail_cache_path") or not Path(cached["thumbnail_cache_path"]).exists():
            if max(source_image.size) > RESOLUTION_THUMBNAIL:
                thumb_img = source_image.copy()
                thumb_img.thumbnail((RESOLUTION_THUMBNAIL, RESOLUTION_THUMBNAIL), Image.LANCZOS)
            else:
                # Source is smaller than RESOLUTION_THUMBNAIL — include as-is
                thumb_img = source_image.copy()
            final_thumb_path = self._save_and_record(
                thumb_img, file_info, RESOLUTION_THUMBNAIL, cache_paths[str(RESOLUTION_THUMBNAIL)]
            )

        # Save missing preview (RESOLUTION_PREVIEW) — only if source is full resolution
        if source_resolution == RESOLUTION_FULL and (
            not cached.get("preview_cache_path") or not Path(cached["preview_cache_path"]).exists()
        ):
            if max(source_image.size) > RESOLUTION_PREVIEW:
                preview_img = source_image.copy()
                preview_img.thumbnail((RESOLUTION_PREVIEW, RESOLUTION_PREVIEW), Image.LANCZOS)
            else:
                # Source is smaller than RESOLUTION_PREVIEW — include as-is
                preview_img = source_image.copy()
            final_preview_path = self._save_and_record(
                preview_img, file_info, RESOLUTION_PREVIEW, cache_paths[str(RESOLUTION_PREVIEW)]
            )

        # Emit signals for already-cached resolutions (BUG #3 fix)
        self._emit_cached_thumbnails(file_info, cached)

        # Update database with all paths (BUG #5 fix: preserve existing paths)
        self._db.upsert_thumbnail(
            path=str(file_info.path),
            mtime=int(file_info.mtime),
            size=file_info.size,
            width=source_image.width,
            height=source_image.height,
            cache_uuid=cache_uuid,
            thumbnail_cache_path=final_thumb_path,
            preview_cache_path=final_preview_path,
            full_cache_path=final_full_path,
        )
        logger.debug("Derived missing resolutions for %s", file_info.path)
        return "derived"

    def _render_all_resolutions(self, file_info: FileInfo) -> None:
        """Render all three resolutions from the source file.

        Checks ``self._cancel_event`` between expensive steps so that a
        folder switch or app shutdown can abort stale work early.
        """
        start = time.perf_counter()
        logger.debug("_render_all_resolutions: %s", file_info.path)

        # ── Cancel check: before any work ──────────────────────────────
        if self._cancel_event.is_set():
            logger.debug("_render_all_resolutions cancelled before start: %s", file_info.path)
            return

        # Get or create a per-folder UUID so all images in the same folder
        # share a cache directory.  Atomic insert prevents race conditions
        # when two threads process images from the same folder simultaneously.
        folder_path = str(file_info.path.parent)
        cache_uuid = self._db.get_or_create_folder_uuid(folder_path, generate_cache_uuid())
        cache_paths = generate_cache_paths(file_info.path, cache_uuid)

        # Render full resolution first
        if file_info.extension.lower() in {".psd", ".psb"}:
            threshold = self._settings_service.get_large_canvas_threshold_mp()
            grid_str = self._settings_service.get_tile_grid_size()  # e.g. "2x2"
            grid_x, grid_y = (int(d) for d in grid_str.split("x"))
            full_img = render_psd_image(
                file_info.path,
                threshold,
                grid_x,
                grid_y,
                target_size=RESOLUTION_FULL,
                cancel_event=self._cancel_event,
            )
        elif file_info.extension.lower() == ".clip":
            full_img = render_clip_image(file_info.path, target_size=RESOLUTION_FULL)
        else:
            full_img = render_plain_image(file_info.path, target_size=RESOLUTION_FULL)

        # ── Cancel check: after expensive render ───────────────────────
        if self._cancel_event.is_set():
            logger.debug("_render_all_resolutions cancelled after render: %s", file_info.path)
            return

        if full_img is None:
            self.errorOccurred.emit(str(file_info.path), "Failed to render image")
            self.thumbnailReady.emit(str(file_info.path), None, None, None)
            return

        # Save full resolution
        self._save_and_record(full_img, file_info, RESOLUTION_FULL, cache_paths["full"])

        # Derive and save smaller resolutions
        smaller_sizes = derive_smaller_sizes(full_img, [RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW])

        # Track which paths were actually saved (BUG #4 fix)
        thumb_path = None
        preview_path = None

        for size, img in smaller_sizes.items():
            # ── Cancel check: between resolution saves ─────────────────
            if self._cancel_event.is_set():
                logger.debug("_render_all_resolutions cancelled during smaller sizes: %s", file_info.path)
                return

            resolution_key = str(RESOLUTION_THUMBNAIL) if size == RESOLUTION_THUMBNAIL else str(RESOLUTION_PREVIEW)
            cache_path_str = self._save_and_record(img, file_info, size, cache_paths[resolution_key])
            if size == RESOLUTION_THUMBNAIL:
                thumb_path = cache_path_str
            elif size == RESOLUTION_PREVIEW:
                preview_path = cache_path_str

        # Extract and persist dominant color tags (from full resolution)
        if self._settings_service.get_color_tag_enabled():
            try:
                # Deferred import to avoid circular dependency
                from tarragon.color_tagger import extract_dominant_color_tags

                tags = extract_dominant_color_tags(
                    full_img,
                    palette_size=self._settings_service.get_color_tag_palette_size(),
                    min_share=self._settings_service.get_color_tag_min_share(),
                    neutral_s_threshold=self._settings_service.get_color_tag_neutral_s_threshold(),
                )
                self._db.replace_auto_color_tags(str(file_info.path), tags)
                self.tagsUpdated.emit()
            except (ImportError, OSError, RuntimeError, ValueError):
                logger.warning("Color tagging failed for %s", file_info.path, exc_info=True)

        # Update database with only the paths that were actually saved (BUG #4 fix)
        self._db.upsert_thumbnail(
            path=str(file_info.path),
            mtime=int(file_info.mtime),
            size=file_info.size,
            width=full_img.width,
            height=full_img.height,
            cache_uuid=cache_uuid,
            thumbnail_cache_path=thumb_path,
            preview_cache_path=preview_path,
            full_cache_path=str(cache_paths["full"]),
        )
        elapsed = time.perf_counter() - start
        logger.debug("_render_all_resolutions completed in %.3fs: %s", elapsed, file_info.path)

    def _on_error(self, file_info: FileInfo, error_message: str) -> None:
        """Handle render error."""
        self.errorOccurred.emit(str(file_info.path), error_message)
        self.thumbnailReady.emit(str(file_info.path), None, None, None)
