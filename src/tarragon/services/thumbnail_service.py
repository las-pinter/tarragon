"""Thumbnail service — async orchestration for thumbnail generation and caching."""

from __future__ import annotations

from pathlib import Path

from PIL import Image
from PySide6.QtCore import QObject, QRunnable, QThreadPool, Signal, Slot

from tarragon.db import Database
from tarragon.scanner import FileInfo
from tarragon.settings import Settings
from tarragon.thumbnail import (
    _cache_file_path,
    render_plain_image,
    render_psd_image,
    save_to_cache,
)


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
    ) -> None:
        super().__init__()
        self._file_info = file_info
        self._cache_format = cache_format
        self._on_done = on_done
        self._on_error = on_error
        self._cache_path: Path | None = None

    def run(self) -> None:
        """Execute PSD render in worker thread."""
        try:
            img = render_psd_image(self._file_info.path)
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

    thumbnailReady = Signal(str, object)  # noqa: N815 — Qt signal follows camelCase convention
    thumbnailsUpdated = Signal(list)  # noqa: N815 — Qt signal follows camelCase convention
    errorOccurred = Signal(str, str)  # noqa: N815 — Qt signal follows camelCase convention

    def __init__(self, db: Database, settings: Settings, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._db = db
        self._settings = settings
        self._threadpool = QThreadPool()
        self._cache_format: str = self._settings.get("cache_format")  # "png" or "jpeg"

    def set_cache_format(self, fmt: str) -> None:
        """Update the cache format setting."""
        if fmt not in ("png", "jpeg"):
            raise ValueError(f"Unknown cache_format: {fmt!r}. Expected one of ['png', 'jpeg']")
        self._cache_format = fmt

    @Slot(FileInfo)
    def check_and_render(self, file_info: FileInfo) -> None:
        """Check cache; if stale or missing, dispatch render.

        Called from the main thread for each file discovered during a scan.
        """
        cached = self._db.get_thumbnail(str(file_info.path))
        if cached is not None:
            # Check if cache is still valid
            if cached["mtime"] == int(file_info.mtime) and cached["size"] == file_info.size:
                cache_path = Path(cached["master_cache_path"]) if cached.get("master_cache_path") else None
                if cache_path is not None and cache_path.exists():
                    try:
                        img = Image.open(cache_path)
                        self.thumbnailReady.emit(str(file_info.path), img)
                        return
                    except (OSError, ValueError):
                        pass  # Corrupt cache file — fall through to re-render

        # Cache miss or invalid — dispatch render
        if file_info.extension.lower() in {".psd", ".psb"}:
            self._render_psd(file_info)
        else:
            self._render_plain(file_info)

    def _render_plain(self, file_info: FileInfo) -> None:
        """Dispatch plain image render to QThreadPool."""
        task = _RenderTask(
            file_info=file_info,
            cache_format=self._cache_format,
            on_done=self._on_done,
            on_error=self._on_error,
        )
        if not self._threadpool.start(task):
            self._on_error(file_info, "Render queue full — thumbnail skipped")

    def _render_psd(self, file_info: FileInfo) -> None:
        """Dispatch PSD/PSB render via ProcessPoolExecutor (blocking in worker thread).

        Since render_psd_image() uses ProcessPoolExecutor internally and is
        already off the main thread, we run it in the QThreadPool to keep
        the main thread free.
        """
        task = _RenderPSDTask(
            file_info=file_info,
            cache_format=self._cache_format,
            on_done=self._on_done,
            on_error=self._on_error,
        )
        if not self._threadpool.start(task):
            self._on_error(file_info, "Render queue full — thumbnail skipped")

    def _on_done(self, file_info: FileInfo, img: Image.Image | None, cache_path: Path | None) -> None:
        """Handle completion of a render — persist to DB and emit thumbnailReady."""
        if img is not None and cache_path is not None:
            try:
                self._db.upsert_thumbnail(
                    path=str(file_info.path),
                    mtime=int(file_info.mtime),
                    size=file_info.size,
                    width=img.width,
                    height=img.height,
                    master_cache_path=str(cache_path),
                )
            except Exception:
                pass  # Render succeeded — don't lose the thumbnail
        self.thumbnailReady.emit(str(file_info.path), img)

    def _on_error(self, file_info: FileInfo, error_message: str) -> None:
        """Handle render error."""
        self.errorOccurred.emit(str(file_info.path), error_message)
        self.thumbnailReady.emit(str(file_info.path), None)
