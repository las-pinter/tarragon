"""Async orchestration for thumbnail generation and caching."""

from __future__ import annotations

import logging
import threading
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any, ClassVar

from PIL import Image
from PySide6.QtCore import QObject, QRunnable, QThreadPool, Signal, Slot

from tarragon.db.common.tag import Tag, TagSource
from tarragon.db.database import Database
from tarragon.renderers import registry
from tarragon.renderers.cache import (
    RESOLUTION_FULL,
    RESOLUTION_PREVIEW,
    RESOLUTION_THUMBNAIL,
    clear_cache,
    clear_full_res_cache,
    derive_smaller_sizes,
    generate_cache_paths,
    generate_cache_uuid,
    invalidate_cache_files,
    load_image,
    resize_long_edge,
    save_to_cache,
    tier_key,
)
from tarragon.renderers.psd import get_executor
from tarragon.scanner import FileInfo, scan_folder
from tarragon.services.color_tagger import extract_dominant_colors
from tarragon.services.settings_service import SettingsService
from tarragon.services.tag_service import TagService

logger = logging.getLogger(__name__)

# Maximum time to wait for in-flight renders to drain before giving up.
POOL_DRAIN_TIMEOUT_MS = 5000


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
            logger.debug("Cancelled before start: %s", self._file_info.path)
            return
        try:
            self._render_func(self._file_info)
        except Exception as exc:
            self._on_error(self._file_info, str(exc))


class _ScanFolderTask(QRunnable):
    """Performs a folder scan (walk + stat + DB stubs + render dispatch) off the GUI thread."""

    def __init__(
        self,
        folder_path: Path,
        token: int,
        service: ThumbnailService,
        cancel_event: threading.Event,
        supersede_event: threading.Event,
    ) -> None:
        super().__init__()
        self._folder_path = folder_path
        self._token = token
        self._service = service
        self._cancel_event = cancel_event
        self._supersede_event = supersede_event

    def _aborted(self) -> bool:
        """True when the scan should stop: cancellation or a newer scan superseded it."""
        return self._cancel_event.is_set() or self._supersede_event.is_set()

    def run(self) -> None:
        """Scan the folder; DB stubs land before the completion signal so queries see results."""
        try:
            if self._aborted():
                return
            file_infos = scan_folder(self._folder_path)
            if self._aborted():
                return
            stubs = [(str(fi.path), int(fi.mtime), fi.size) for fi in file_infos]
            self._service._db.bulk_upsert_stubs(stubs)  # noqa: SLF001
            for fi in file_infos:
                if self._aborted():
                    break
                self._service.check_and_render(fi)
            # A superseded scan must never report completion: the single-flight
            # supersede sets this task's event before dispatching the new scan,
            # so a stale worker cannot emit after the newer scan has begun.
            if self._aborted():
                return
            self._service.scan_completed.emit(self._token, file_infos)
        finally:
            self._service.scan_finished.emit(self._token)


class _PurgeCacheTask(QRunnable):
    """Performs the cache purge (drain-wait + unlink + DB clear) off the GUI thread."""

    def __init__(self, service: ThumbnailService) -> None:
        super().__init__()
        self._service = service

    def run(self) -> None:
        """Run the purge body and always report completion so the GUI can re-navigate."""
        try:
            self._service._purge_cache_sync()  # noqa: SLF001
        finally:
            self._service._purge_in_progress = False  # noqa: SLF001
            self._service.cache_purged.emit()


class ThumbnailService(QObject):
    """Coordinates thumbnail generation, caching, and UI signal emission.

    Owns the QThreadPool for plain image renders and delegates PSD/PSB
    compositing to the module-level ProcessPoolExecutor shared singleton.
    Cache purges run on a dedicated pool so the purge body can wait for
    the render pool to drain without deadlocking (a task cannot wait on
    the pool it is itself running on).
    """

    thumbnail_ready = Signal(str, object, object)
    error_occurred = Signal(str, str)
    tags_updated = Signal()
    # Folder scan lifecycle. ``scan_completed`` carries (token, file_infos);
    # the token lets the GUI discard results from a superseded scan.
    scan_started = Signal(int, str)
    scan_completed = Signal(int, object)
    scan_finished = Signal(int)
    # Cache purge lifecycle.
    purge_started = Signal()
    cache_purged = Signal()

    # Test seam: when True, scan/purge QRunnables run inline on the calling
    # thread so tests observe fully synchronous state (production is async).
    synchronous_workers: ClassVar[bool] = False

    def __init__(
        self,
        db: Database,
        settings_service: SettingsService,
        tag_service: TagService,
        parent: QObject | None = None,
        synchronous_workers: bool | None = None,
    ) -> None:
        super().__init__(parent)
        self._db = db
        self._settings_service = settings_service
        registry.configure_settings(lambda: self._settings_service)
        self._tag_service = tag_service
        self._cancel_event = threading.Event()
        # Render pool: plain image thumbnails/previews are rendered here and
        # scan tasks dispatch render requests into it.
        self._threadpool = QThreadPool()
        # Dedicated purge pool: the purge body drains `_threadpool` before
        # unlinking the cache, so it must never run on that same pool (a
        # running task counts as active, making the drain unwinnable).
        self._purge_pool = QThreadPool()
        self._scan_token = 0
        # Supersede event of the most recently dispatched scan; a new scan
        # sets it BEFORE dispatching so the older worker suppresses its
        # completion emission (closes the source-side stale-scan race).
        self._scan_supersede_event: threading.Event | None = None
        self._purge_in_progress = False
        self._synchronous_workers = (
            type(self).synchronous_workers if synchronous_workers is None else synchronous_workers
        )

        # Pre-initialize the shared PSD ProcessPoolExecutor with the
        # user-configured worker count (falls back to RAM-adaptive default
        # when the setting is absent / None).
        max_psd_workers = self._settings_service.max_psd_workers.get()
        get_executor(max_workers=max_psd_workers)

    @property
    def _cache_format(self) -> str:
        """Current cache format, read live so settings changes apply immediately.

        Compatibility shim for callers that referenced the old instance
        snapshot attribute; render paths read ``cache_format.get()`` directly.
        """
        return self._settings_service.cache_format.get()

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

    def request_folder_scan(self, folder_path: Path) -> int:
        """Scan *folder_path* in a worker, cancelling any previous scan.

        Single-flight: the previous scan is cancelled (cancel event set +
        pool clear) before the new one starts. The returned token appears
        in ``scan_started``/``scan_completed``/``scan_finished`` so the GUI
        can discard results from a superseded scan.

        Returns:
            An integer token identifying this scan.
        """
        self._scan_token += 1
        token = self._scan_token
        # Mark the previous scan as superseded before dispatching the new
        # one so a stale worker can never emit scan_completed after the new
        # scan has begun (previously only the GUI's token guard contained
        # this race).
        if self._scan_supersede_event is not None:
            self._scan_supersede_event.set()
        supersede_event = threading.Event()
        self._scan_supersede_event = supersede_event
        self._cancel_event.set()
        self._threadpool.clear()
        self._cancel_event.clear()
        self.scan_started.emit(token, str(folder_path))
        task = _ScanFolderTask(
            folder_path=folder_path,
            token=token,
            service=self,
            cancel_event=self._cancel_event,
            supersede_event=supersede_event,
        )
        self._start_background_task(task)
        return token

    def purge_cache(self) -> None:
        """Purge the entire thumbnail cache off the GUI thread (single-flight).

        Cancels pending renders and waits for the thread pool to drain so
        no in-flight render can write to the cache while files are being
        deleted; the drain, cache-tree unlink, and DB clear all happen in
        a worker. ``cache_purged`` is emitted when the purge attempt
        finishes, so the GUI re-navigation follows completion.
        """
        if self._purge_in_progress:
            logger.debug("purge_cache: already in progress; ignoring request")
            return
        self._purge_in_progress = True
        self.purge_started.emit()
        self._start_purge_task(_PurgeCacheTask(service=self))

    def _purge_cache_sync(self) -> None:
        """Run the purge body: cancel, drain, unlink cache tree, clear DB rows.

        If the pool does not drain within the timeout, the purge is
        aborted so no cache files are deleted while a render may still be
        writing.
        """
        self.cancel_pending()
        if not self._threadpool.waitForDone(POOL_DRAIN_TIMEOUT_MS):
            logger.warning("purge_cache: thread pool did not drain within %d ms; aborting purge", POOL_DRAIN_TIMEOUT_MS)
            self.reset_cancel()
            return
        clear_cache()
        self._db.clear_thumbnails()
        self.reset_cancel()

    def _start_background_task(self, task: QRunnable) -> None:
        """Dispatch *task* to the render pool, or run it inline under the test seam."""
        if self._synchronous_workers:
            task.run()
        else:
            self._threadpool.start(task)

    def _start_purge_task(self, task: QRunnable) -> None:
        """Dispatch *task* to the purge pool, or run it inline under the test seam.

        Purge must run on its own pool: its body waits for the render pool
        to drain and unlink the cache, and a task running on a pool counts
        as active there, so draining that same pool from inside the task
        could never complete and the purge would abort on the timeout.
        """
        if self._synchronous_workers:
            task.run()
        else:
            self._purge_pool.start(task)

    def shutdown(self, timeout_ms: int = POOL_DRAIN_TIMEOUT_MS) -> None:
        """Graceful shutdown. Cancel pending tasks, wait for running ones.

        Args:
            timeout_ms: Maximum time to wait for in-flight tasks to finish
            (milliseconds). Defaults to POOL_DRAIN_TIMEOUT_MS (5 seconds).
        """
        self.cancel_pending()
        from tarragon.renderers.psd import shutdown_executor

        shutdown_executor()
        drained = self._threadpool.waitForDone(timeout_ms)
        if drained:
            self._clear_full_res_cache()
        else:
            logger.warning(
                "shutdown: thread pool did not drain within %d ms; skipping full-res cache clear", timeout_ms
            )
        logger.debug("Shutdown complete")

    def _clear_full_res_cache(self) -> None:
        """Clear the full-resolution cache tier when enabled by the setting.

        Runs after the thread pool has drained so no in-flight render can
        write to ``cache/full`` while files are being deleted.  Disk-only:
        database rows are left untouched.
        """
        clear_full_res_cache(enabled=self._settings_service.clear_full_res_on_exit.get())

    @Slot(FileInfo)
    def check_and_render(self, file_info: FileInfo) -> str:
        """Check cache. If stale or missing, render all resolutions.

        Called from the main thread for each file discovered during a scan.
        Handles three resolution tiers: thumbnail (256), preview (1024),
        and full (original resolution).

        Returns a status string for batch summary logging:
            "cached": all resolutions served from cache
            "derived": missing resolutions derived from existing cached image
            "queued": async render dispatched to thread pool
        """
        logger.debug("Called - path: %s", file_info.path)
        start = time.perf_counter()
        cached = self._db.get_thumbnail(str(file_info.path))

        # Auto-regeneration: When source file mtime or size changes,
        # the cache is considered stale and triggers a full re-render.
        # This happens automatically on folder re-scan.
        if cached and cached["mtime"] == int(file_info.mtime) and cached["size"] == file_info.size:
            has_thumbnail = cached.get("thumbnail_cache_path") and Path(cached["thumbnail_cache_path"]).exists()
            has_preview = cached.get("preview_cache_path") and Path(cached["preview_cache_path"]).exists()
            has_full = cached.get("full_cache_path") and Path(cached["full_cache_path"]).exists()

            if has_thumbnail and has_preview and has_full:
                self._emit_cached_thumbnails(file_info, cached)
                elapsed = time.perf_counter() - start
                logger.debug("completed in %.3fs: status=cached", elapsed)
                return "cached"

            result = self._derive_missing_resolutions(file_info, cached)
            elapsed = time.perf_counter() - start
            logger.debug("completed in %.3fs: status=%s", elapsed, result)
            return result

        task = _RenderAllTask(
            file_info=file_info,
            on_error=self._on_error,
            render_func=self._render_all_resolutions,
            cancel_event=self._cancel_event,
        )
        self._threadpool.start(task)
        logger.debug("Queued render for %s", file_info.path)
        elapsed = time.perf_counter() - start
        logger.debug("completed in %.3fs: status=queued", elapsed)
        return "queued"

    def invalidate_and_render(self, source_path: Path) -> None:
        """Delete cached thumbnails and re-render from source.

        Invalidates all cache files for *source_path*, then triggers a
        fresh render via :meth:`check_and_render`.

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

        invalidate_cache_files(self._db, str(source_path))

        try:
            stat = source_path.stat()
        except OSError:
            logger.warning("invalidate_and_render: source file not found: %s", source_path)
            return

        file_info = FileInfo.from_stat(path=source_path, stat_result=stat)

        self.check_and_render(file_info)

    def _emit_cached_thumbnails(self, file_info: FileInfo, cached: dict[str, Any]) -> None:
        """Emit thumbnail_ready signals for all cached resolutions."""
        cache_entries: list[tuple[str, int | None]] = [
            ("thumbnail_cache_path", RESOLUTION_THUMBNAIL),
            ("preview_cache_path", RESOLUTION_PREVIEW),
            ("full_cache_path", RESOLUTION_FULL),
        ]

        for resolution_key, resolution_size in cache_entries:
            cache_path = cached.get(resolution_key)
            if cache_path and Path(cache_path).exists():
                try:
                    # Header-only open: a full pixel decode would slow the hot emit path.
                    with Image.open(cache_path):
                        pass
                    self.thumbnail_ready.emit(str(file_info.path), resolution_size, cache_path)
                except Exception:
                    logger.warning(
                        "Corrupt cache file, skipping resolution %s: %s", resolution_size, cache_path, exc_info=True
                    )

    def _save_and_record(
        self,
        img: Image.Image,
        file_info: FileInfo,
        resolution_size: int | None,
        cache_path: Path,
    ) -> str:
        """Save *img* to cache, emit thumbnail_ready, and return the path string.

        Shared helper used by both :meth:`_derive_missing_resolutions` and
        :meth:`_render_all_resolutions` to avoid duplicating the
        save → emit → record pattern.
        """
        save_to_cache(img, cache_path, self._settings_service.cache_format.get())
        path_str = str(cache_path)
        self.thumbnail_ready.emit(str(file_info.path), resolution_size, path_str)
        return path_str

    def _derive_missing_resolutions(self, file_info: FileInfo, cached: dict[str, Any]) -> str:
        """Derive missing smaller resolutions from the largest available cached image.

        Returns a status string: "derived" or "queued".
        """
        full_path = cached.get("full_cache_path")
        preview_path = cached.get("preview_cache_path")

        source_image: Image.Image | None = None
        source_resolution: int | None = None

        if full_path and Path(full_path).exists():
            try:
                source_image = load_image(full_path)
                source_resolution = RESOLUTION_FULL
            except Exception:
                logger.warning("Failed to open cached full resolution: %s", full_path, exc_info=True)

        if source_image is None and preview_path and Path(preview_path).exists():
            try:
                source_image = load_image(preview_path)
                source_resolution = RESOLUTION_PREVIEW
            except Exception:
                logger.warning("Failed to open cached preview resolution: %s", preview_path, exc_info=True)

        if source_image is None:
            # No cached image available, render from source (async)
            task = _RenderAllTask(
                file_info=file_info,
                on_error=self._on_error,
                render_func=self._render_all_resolutions,
                cancel_event=self._cancel_event,
            )
            self._threadpool.start(task)
            logger.debug("Queued render for %s", file_info.path)
            return "queued"

        # Derive missing sizes, use per-folder UUID from DB (atomic)
        folder_path = str(file_info.path.parent)
        candidate_uuid = cached.get("cache_uuid") or generate_cache_uuid()
        cache_uuid = self._db.get_or_create_folder_uuid(folder_path, candidate_uuid)
        cache_paths = generate_cache_paths(file_info.path, cache_uuid, self._settings_service.cache_format.get())

        # Track which paths to write to DB (preserve existing, add new)
        final_thumb_path = cached.get("thumbnail_cache_path")
        final_preview_path = cached.get("preview_cache_path")
        final_full_path = cached.get("full_cache_path")

        # Save missing thumbnail
        if not cached.get("thumbnail_cache_path") or not Path(cached["thumbnail_cache_path"]).exists():
            thumb_img = resize_long_edge(source_image, RESOLUTION_THUMBNAIL)
            final_thumb_path = self._save_and_record(
                thumb_img, file_info, RESOLUTION_THUMBNAIL, cache_paths[tier_key(RESOLUTION_THUMBNAIL)]
            )

        if source_resolution == RESOLUTION_FULL and (
            not cached.get("preview_cache_path") or not Path(cached["preview_cache_path"]).exists()
        ):
            preview_img = resize_long_edge(source_image, RESOLUTION_PREVIEW)
            final_preview_path = self._save_and_record(
                preview_img, file_info, RESOLUTION_PREVIEW, cache_paths[tier_key(RESOLUTION_PREVIEW)]
            )

        self._emit_cached_thumbnails(file_info, cached)

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
        logger.debug("Called - file_info: path: %s", file_info.path)

        if self._cancel_event.is_set():
            logger.debug("cancelled before start: %s", file_info.path)
            return

        # Get or create a per-folder UUID so all images in the same folder
        # share a cache directory. Atomic insert prevents race conditions
        # when two threads process images from the same folder simultaneously.
        folder_path = str(file_info.path.parent)
        cache_uuid = self._db.get_or_create_folder_uuid(folder_path, generate_cache_uuid())
        cache_paths = generate_cache_paths(file_info.path, cache_uuid, self._settings_service.cache_format.get())

        renderer = registry.FORMAT_DISPATCH.get(file_info.extension.lower(), registry.DEFAULT_RENDERER)
        full_img = renderer(file_info.path, RESOLUTION_FULL, self._cancel_event)

        # Cancel check: after expensive render
        if self._cancel_event.is_set():
            logger.debug("cancelled after render: %s", file_info.path)
            return

        if full_img is None:
            self.error_occurred.emit(str(file_info.path), "Failed to render image")
            self.thumbnail_ready.emit(str(file_info.path), None, None)
            return

        self._save_and_record(full_img, file_info, RESOLUTION_FULL, cache_paths[tier_key(RESOLUTION_FULL)])

        smaller_sizes = derive_smaller_sizes(full_img, [RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW])

        thumb_path = None
        preview_path = None

        for size, img in smaller_sizes.items():
            # Cancel check: between resolution saves
            if self._cancel_event.is_set():
                logger.debug("cancelled during smaller sizes: %s", file_info.path)
                return

            resolution_key = tier_key(size)
            cache_path_str = self._save_and_record(img, file_info, size, cache_paths[resolution_key])
            if size == RESOLUTION_THUMBNAIL:
                thumb_path = cache_path_str
            elif size == RESOLUTION_PREVIEW:
                preview_path = cache_path_str

        # Extract and persist dominant color tags (from full resolution)
        if self._settings_service.color_tag_enabled.get():
            try:
                colors = extract_dominant_colors(
                    full_img,
                    palette_size=self._settings_service.color_tag_palette_size.get(),
                    min_share=self._settings_service.color_tag_min_share.get(),
                    neutral_s_threshold=self._settings_service.color_tag_neutral_s_threshold.get(),
                )

                tags: set[Tag] = set()
                for color in colors:
                    tags.add(self._tag_service.create_tag(color, TagSource.AUTO_COLOR))

                self._tag_service.replace_auto_color_tags(str(file_info.path), tags)
                self.tags_updated.emit()
            except (ImportError, OSError, RuntimeError, ValueError):
                logger.warning("Color tagging failed for %s", file_info.path, exc_info=True)

        # Update database with only the paths that were actually saved
        self._db.upsert_thumbnail(
            path=str(file_info.path),
            mtime=int(file_info.mtime),
            size=file_info.size,
            width=full_img.width,
            height=full_img.height,
            cache_uuid=cache_uuid,
            thumbnail_cache_path=thumb_path,
            preview_cache_path=preview_path,
            full_cache_path=str(cache_paths[tier_key(RESOLUTION_FULL)]),
        )
        elapsed = time.perf_counter() - start
        logger.debug("completed in %.3fs: %s", elapsed, file_info.path)

    def _on_error(self, file_info: FileInfo, error_message: str) -> None:
        """Handle render error."""
        self.error_occurred.emit(str(file_info.path), error_message)
        self.thumbnail_ready.emit(str(file_info.path), None, None)
