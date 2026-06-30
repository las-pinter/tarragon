"""Thumbnail service — async orchestration for thumbnail generation and caching."""

from __future__ import annotations

import logging
import time
from pathlib import Path

from PIL import Image
from PySide6.QtCore import QObject, QRunnable, QThreadPool, Signal, Slot

logger = logging.getLogger(__name__)

from tarragon.db import Database
from tarragon.scanner import FileInfo
from tarragon.settings import Settings
from tarragon.thumbnail import (
    _cache_file_path,
    _get_executor,
    derive_smaller_sizes,
    generate_cache_paths,
    generate_cache_uuid,
    render_plain_image,
    render_psd_image,
    save_to_cache,
)


class _RenderAllTask(QRunnable):
    """Runs multi-resolution rendering in a QThreadPool worker thread."""

    def __init__(
        self,
        file_info: FileInfo,
        on_done: callable,
        on_error: callable,
        render_func: callable,
    ) -> None:
        super().__init__()
        self._file_info = file_info
        self._on_done = on_done
        self._on_error = on_error
        self._render_func = render_func

    def run(self) -> None:
        """Execute render in worker thread."""
        try:
            self._render_func(self._file_info)
            self._on_done(self._file_info)
        except Exception as exc:
            self._on_error(self._file_info, str(exc))


class _RenderTask(QRunnable):
    """Runs a single plain-image render in a QThreadPool worker thread."""

    def __init__(
        self,
        file_info: FileInfo,
        cache_format: str,
        on_done: callable,
        on_error: callable,
    ) -> None:
        super().__init__()
        self._file_info = file_info
        self._cache_format = cache_format
        self._on_done = on_done
        self._on_error = on_error
        self._cache_path: Path | None = None

    def run(self) -> None:
        """Execute render in worker thread."""
        try:
            img = render_plain_image(self._file_info.path)
            if img is not None:
                self._cache_path, _ = _cache_file_path(self._file_info.path, self._cache_format)
                save_to_cache(img, self._cache_path, self._cache_format)
            self._on_done(self._file_info, img, self._cache_path)
        except Exception as exc:
            self._on_error(self._file_info, str(exc))


class _RenderPSDTask(QRunnable):
    """Runs PSD rendering in a QThreadPool worker thread."""

    def __init__(
        self,
        file_info: FileInfo,
        cache_format: str,
        on_done: callable,
        on_error: callable,
        large_canvas_threshold_mp: float,
        tile_grid_x: int,
        tile_grid_y: int,
    ) -> None:
        super().__init__()
        self._file_info = file_info
        self._cache_format = cache_format
        self._on_done = on_done
        self._on_error = on_error
        self._large_canvas_threshold_mp = large_canvas_threshold_mp
        self._tile_grid_x = tile_grid_x
        self._tile_grid_y = tile_grid_y
        self._cache_path: Path | None = None

    def run(self) -> None:
        """Execute PSD render in worker thread."""
        try:
            img = render_psd_image(
                self._file_info.path,
                self._large_canvas_threshold_mp,
                self._tile_grid_x,
                self._tile_grid_y,
            )
            if img is not None:
                self._cache_path, _ = _cache_file_path(self._file_info.path, self._cache_format)
                save_to_cache(img, self._cache_path, self._cache_format)
            self._on_done(self._file_info, img, self._cache_path)
        except Exception as exc:
            self._on_error(self._file_info, str(exc))


class ThumbnailService(QObject):
    """Coordinates thumbnail generation, caching, and UI signal emission.

    Owns the QThreadPool for plain image renders and delegates PSD/PSB
    compositing to the module-level ProcessPoolExecutor shared singleton.
    """

    thumbnailReady = Signal(str, object, object, object)  # noqa: N815 — (path, image, resolution_size, cache_path)
    thumbnailsUpdated = Signal(list)  # noqa: N815 — Qt signal follows camelCase convention
    errorOccurred = Signal(str, str)  # noqa: N815 — Qt signal follows camelCase convention
    tagsUpdated = Signal()  # noqa: N815 — emitted after auto-color tags are persisted

    def __init__(self, db: Database, settings: Settings, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._db = db
        self._settings = settings
        self._threadpool = QThreadPool()
        self._cache_format: str = self._settings.get("cache_format")  # "png" or "jpeg"

        # Pre-initialize the shared PSD ProcessPoolExecutor with the
        # user-configured worker count (falls back to RAM-adaptive default
        # when the setting is absent / None).
        max_psd_workers = int(self._settings.get("max_psd_workers"))
        _get_executor(max_workers=max_psd_workers)

    def set_cache_format(self, fmt: str) -> None:
        """Update the cache format setting."""
        if fmt not in ("png", "jpeg"):
            raise ValueError(f"Unknown cache_format: {fmt!r}. Expected one of ['png', 'jpeg']")
        self._cache_format = fmt

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
            on_done=self._on_render_all_done,
            on_error=self._on_error,
            render_func=self._render_all_resolutions,
        )
        self._threadpool.start(task)
        logger.debug("Queued render for %s", file_info.path)
        elapsed = time.perf_counter() - start
        logger.debug("check_and_render completed in %.3fs: status=queued", elapsed)
        return "queued"

    def _on_render_all_done(self, file_info: FileInfo) -> None:
        """Handle completion of multi-resolution render (called from main thread)."""
        # Signal emission and DB upsert are handled inside _render_all_resolutions
        pass

    def _emit_cached_thumbnails(self, file_info: FileInfo, cached: dict) -> None:
        """Emit thumbnailReady signals for all cached resolutions."""
        for resolution_key, resolution_size in [
            ("thumbnail_cache_path", 256),
            ("preview_cache_path", 1024),
            ("full_cache_path", None),
        ]:
            cache_path = cached.get(resolution_key)
            if cache_path and Path(cache_path).exists():
                try:
                    img = Image.open(cache_path)
                    self.thumbnailReady.emit(str(file_info.path), img, resolution_size, cache_path)
                except Exception:
                    logger.warning("Corrupt cache file, skipping resolution %s: %s", resolution_size, cache_path, exc_info=True)

    def _derive_missing_resolutions(self, file_info: FileInfo, cached: dict) -> str:
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
                source_resolution = None  # Full resolution
            except Exception:
                logger.warning("Failed to open cached full resolution: %s", full_path, exc_info=True)

        if source_image is None and preview_path and Path(preview_path).exists():
            try:
                source_image = Image.open(preview_path)
                source_resolution = 1024
            except Exception:
                logger.warning("Failed to open cached preview resolution: %s", preview_path, exc_info=True)

        if source_image is None:
            # No cached image available — render from source (async)
            task = _RenderAllTask(
                file_info=file_info,
                on_done=self._on_render_all_done,
                on_error=self._on_error,
                render_func=self._render_all_resolutions,
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

        # Save missing thumbnail (256)
        if not cached.get("thumbnail_cache_path") or not Path(cached["thumbnail_cache_path"]).exists():
            if max(source_image.size) > 256:
                thumb_img = source_image.copy()
                thumb_img.thumbnail((256, 256), Image.LANCZOS)
            else:
                # Source is smaller than 256 — include as-is
                thumb_img = source_image.copy()
            save_to_cache(thumb_img, cache_paths["256"], "png")
            final_thumb_path = str(cache_paths["256"])
            self.thumbnailReady.emit(str(file_info.path), thumb_img, 256, final_thumb_path)

        # Save missing preview (1024) — only if source is full resolution
        if source_resolution is None and (
            not cached.get("preview_cache_path") or not Path(cached["preview_cache_path"]).exists()
        ):
            if max(source_image.size) > 1024:
                preview_img = source_image.copy()
                preview_img.thumbnail((1024, 1024), Image.LANCZOS)
            else:
                # Source is smaller than 1024 — include as-is
                preview_img = source_image.copy()
            save_to_cache(preview_img, cache_paths["1024"], "png")
            final_preview_path = str(cache_paths["1024"])
            self.thumbnailReady.emit(str(file_info.path), preview_img, 1024, final_preview_path)

        # Emit signals for already-cached resolutions (BUG #3 fix)
        if cached.get("thumbnail_cache_path") and Path(cached["thumbnail_cache_path"]).exists():
            try:
                img = Image.open(cached["thumbnail_cache_path"])
                self.thumbnailReady.emit(str(file_info.path), img, 256, cached["thumbnail_cache_path"])
            except Exception:
                logger.warning("Failed to emit cached thumbnail (256): %s", cached["thumbnail_cache_path"], exc_info=True)
        if cached.get("preview_cache_path") and Path(cached["preview_cache_path"]).exists():
            try:
                img = Image.open(cached["preview_cache_path"])
                self.thumbnailReady.emit(str(file_info.path), img, 1024, cached["preview_cache_path"])
            except Exception:
                logger.warning("Failed to emit cached preview (1024): %s", cached["preview_cache_path"], exc_info=True)
        if cached.get("full_cache_path") and Path(cached["full_cache_path"]).exists():
            try:
                img = Image.open(cached["full_cache_path"])
                self.thumbnailReady.emit(str(file_info.path), img, None, cached["full_cache_path"])
            except Exception:
                logger.warning("Failed to emit cached full resolution: %s", cached["full_cache_path"], exc_info=True)

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
        """Render all three resolutions from the source file."""
        start = time.perf_counter()
        logger.debug("_render_all_resolutions: %s", file_info.path)
        # Get or create a per-folder UUID so all images in the same folder
        # share a cache directory.  Atomic insert prevents race conditions
        # when two threads process images from the same folder simultaneously.
        folder_path = str(file_info.path.parent)
        cache_uuid = self._db.get_or_create_folder_uuid(folder_path, generate_cache_uuid())
        cache_paths = generate_cache_paths(file_info.path, cache_uuid)

        # Render full resolution first
        if file_info.extension.lower() in {".psd", ".psb"}:
            threshold = self._settings.get("large_canvas_threshold_mp")
            grid_str = self._settings.get("tile_grid_size")  # e.g. "2x2"
            grid_x, grid_y = (int(d) for d in grid_str.split("x"))
            full_img = render_psd_image(
                file_info.path,
                threshold,
                grid_x,
                grid_y,
                target_size=None,
            )
        else:
            full_img = render_plain_image(file_info.path, target_size=None)

        if full_img is None:
            self.errorOccurred.emit(str(file_info.path), "Failed to render image")
            self.thumbnailReady.emit(str(file_info.path), None, None, None)
            return

        # Save full resolution
        save_to_cache(full_img, cache_paths["full"], "png")
        self.thumbnailReady.emit(str(file_info.path), full_img, None, str(cache_paths["full"]))

        # Derive and save smaller resolutions
        smaller_sizes = derive_smaller_sizes(full_img, [256, 1024])

        # Track which paths were actually saved (BUG #4 fix)
        thumb_path = None
        preview_path = None

        for size, img in smaller_sizes.items():
            resolution_key = "256" if size == 256 else "1024"
            save_to_cache(img, cache_paths[resolution_key], "png")
            cache_path_str = str(cache_paths[resolution_key])
            if size == 256:
                thumb_path = cache_path_str
            elif size == 1024:
                preview_path = cache_path_str
            self.thumbnailReady.emit(str(file_info.path), img, size, cache_path_str)

        # Extract and persist dominant color tags (from full resolution)
        if self._settings.get("color_tag_enabled"):
            try:
                from tarragon.color_tagger import extract_dominant_color_tags

                tags = extract_dominant_color_tags(
                    full_img,
                    palette_size=self._settings.get("color_tag_palette_size"),
                    min_share=self._settings.get("color_tag_min_share"),
                    neutral_s_threshold=self._settings.get("color_tag_neutral_s_threshold"),
                )
                self._db.replace_auto_color_tags(str(file_info.path), tags)
                self.tagsUpdated.emit()
            except Exception:
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

    def _render_plain(self, file_info: FileInfo) -> None:
        """Dispatch plain image render to QThreadPool."""
        task = _RenderTask(
            file_info=file_info,
            cache_format=self._cache_format,
            on_done=self._on_done,
            on_error=self._on_error,
        )
        self._threadpool.start(task)
        logger.debug("Queued plain render for %s", file_info.path)

    def _render_psd(self, file_info: FileInfo) -> None:
        """Dispatch PSD/PSB render via ProcessPoolExecutor (blocking in worker thread).

        Since render_psd_image() uses ProcessPoolExecutor internally and is
        already off the main thread, we run it in the QThreadPool to keep
        the main thread free.
        """
        threshold = self._settings.get("large_canvas_threshold_mp")
        grid_str = self._settings.get("tile_grid_size")  # e.g. "2x2"
        grid_x, grid_y = (int(d) for d in grid_str.split("x"))
        task = _RenderPSDTask(
            file_info=file_info,
            cache_format=self._cache_format,
            on_done=self._on_done,
            on_error=self._on_error,
            large_canvas_threshold_mp=threshold,
            tile_grid_x=grid_x,
            tile_grid_y=grid_y,
        )
        self._threadpool.start(task)
        logger.debug("Queued PSD render for %s", file_info.path)

    def _on_done(self, file_info: FileInfo, img: Image.Image | None, cache_path: Path | None) -> None:
        """Handle completion of a legacy render task — emit thumbnailReady.

        Note: The legacy _RenderTask/_RenderPSDTask path emits at full
        resolution (resolution_size=None).  The new multi-resolution path
        (_render_all_resolutions) handles its own signal emission.
        """
        # Emit with resolution_size=None (full resolution) for legacy tasks
        cache_path_str = str(cache_path) if cache_path else None
        self.thumbnailReady.emit(str(file_info.path), img, None, cache_path_str)

    def _on_error(self, file_info: FileInfo, error_message: str) -> None:
        """Handle render error."""
        self.errorOccurred.emit(str(file_info.path), error_message)
        self.thumbnailReady.emit(str(file_info.path), None, None, None)
