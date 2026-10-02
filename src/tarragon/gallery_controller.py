"""GalleryController — filter orchestration and selection handling for the gallery view.

Extracts the filter query pipeline (color, tag, folder, search text, scope)
and thumbnail selection → preview rendering from MainWindow, keeping the
window class focused on layout, menus, and dock management.
"""

from __future__ import annotations

import logging
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import ClassVar

from PIL import Image, ImageOps
from PySide6.QtCore import QCoreApplication, QEvent, QObject, QRunnable, QThreadPool, QTimer, Signal
from PySide6.QtWidgets import QLineEdit, QWidget

from tarragon.common import ImageInfo
from tarragon.db.common.tag import Tag, TagSource
from tarragon.db.database import Database
from tarragon.models.filter_state import FilterState
from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.renderers.cache import load_image
from tarragon.scanner import FileInfo
from tarragon.services.query_service import QueryService
from tarragon.services.tag_service import TagService
from tarragon.services.thumbnail_service import ThumbnailService
from tarragon.theme.color_buckets import ColorBucket
from tarragon.theme.constants import MULTI_PREVIEW_MAX_DEFAULT
from tarragon.widgets.filter_bar import FilterBar
from tarragon.widgets.gallery_info_bar import GalleryInfoBar
from tarragon.widgets.gallery_tabs import GalleryTabs
from tarragon.widgets.preview_panel import PreviewPanel

logger = logging.getLogger(__name__)

# Maximum time to wait for in-flight preview decodes to drain before giving up.
DECODE_POOL_DRAIN_TIMEOUT_MS = 5000


@dataclass(frozen=True)
class LoadedImage:
    """A decoded preview image plus its cache provenance.

    Replaces the former ``img._from_cache`` attribute stamp: provenance is
    carried beside the image instead of being monkeypatched onto a PIL
    Image, which is not guaranteed to preserve arbitrary attributes.
    """

    image: Image.Image
    from_cache: bool
    original_width: int | None = None
    original_height: int | None = None


def _load_preview_image_data(db: Database, path: Path) -> LoadedImage:
    """Load a preview image in a worker thread, preferring cached tiers.

    Shared by the synchronous single-selection path and the async
    multi-select decode workers.

    Cache tiers are the canonical oriented form: renderers normalise
    orientation when building caches, so cached images are returned
    as-is (``from_cache=True``) and never re-transposed.  Only the
    original-file fallback applies :func:`PIL.ImageOps.exif_transpose`
    so every non-cached load is display-oriented regardless of the path
    it is shown through.

    Returns:
        A :class:`LoadedImage` carrying the decoded image, its cache
        provenance, and the original dimensions from the DB record.
    """
    thumb_record = db.get_thumbnail(str(path))

    original_width: int | None = None
    original_height: int | None = None
    if thumb_record:
        original_width = thumb_record.get("width")
        original_height = thumb_record.get("height")

    if thumb_record:
        preview_path = thumb_record.get("preview_cache_path")
        if preview_path and Path(preview_path).is_file():
            img = load_image(preview_path)
            return LoadedImage(
                image=img,
                from_cache=True,
                original_width=original_width,
                original_height=original_height,
            )

        full_path = thumb_record.get("full_cache_path")
        if full_path and Path(full_path).is_file():
            img = load_image(full_path)
            return LoadedImage(
                image=img,
                from_cache=True,
                original_width=original_width,
                original_height=original_height,
            )

    img = load_image(path)
    oriented = ImageOps.exif_transpose(img) or img
    return LoadedImage(
        image=oriented,
        from_cache=False,
        original_width=original_width,
        original_height=original_height,
    )


class _PreviewDecodeRelay(QObject):
    """Signal carrier for decode workers (QRunnables cannot own signals)."""

    decode_ready = Signal(object, str, object, object, object, object)  # epoch, path, image, orig_w, orig_h, from_cache
    decode_failed = Signal(object, str, str)  # epoch, path, error
    decode_busy_started = Signal()
    decode_busy_finished = Signal()


class _DecodePreviewTask(QRunnable):
    """Decodes a single full-size preview image off the GUI thread."""

    def __init__(
        self,
        db: Database,
        path: Path,
        relay: _PreviewDecodeRelay,
        epoch: int,
        cancel_event: threading.Event,
    ) -> None:
        super().__init__()
        self._db = db
        self._path = path
        self._relay = relay
        self._epoch = epoch
        self._cancel_event = cancel_event

    def run(self) -> None:
        """Decode and report; the GUI discards results whose epoch is stale."""
        if self._cancel_event.is_set():
            return
        try:
            loaded = _load_preview_image_data(self._db, self._path)
            self._relay.decode_ready.emit(
                self._epoch,
                str(self._path),
                loaded.image,
                loaded.original_width,
                loaded.original_height,
                loaded.from_cache,
            )
        except Exception as exc:
            self._relay.decode_failed.emit(self._epoch, str(self._path), str(exc))


class GalleryController:
    """Controller for gallery filter orchestration and thumbnail selection.

    Manages the filter query pipeline (color, tag, folder, search text,
    scope) and handles thumbnail selection changes to update the preview
    panel.  All filter state mutations and query execution flow through
    this class; the MainWindow only retains layout and widget wiring.

    Args:
        query_service: Service for composing and executing filtered queries.
        filter_state: Mutable filter state shared with the UI widgets.
        thumbnail_model: Model backing the thumbnail grid view.
        gallery_tabs: Tab widget for folder / global scope switching.
        gallery_info_bar: Info bar showing folder name and active filter count.
        filter_bar: Combined filter bar (color + tag + folder chips).
        search_edit: Search text input for filename filtering.
        search_timer: Debounce timer for search input (connected internally).
        preview_panel: Preview panel for displaying selected images.
        tag_service: Service for tag CRUD operations.
        db: Database instance for preview-image cache lookups.
        thumbnail_service: Optional thumbnail service for cache dispatch.
        max_multi_preview: Maximum images shown in multi-select mosaic.
    """

    # Test seam: when True, decode workers run inline on the calling thread
    # so tests observe fully synchronous state (production is async).
    synchronous_workers: ClassVar[bool] = False

    def __init__(
        self,
        *,
        query_service: QueryService,
        filter_state: FilterState,
        thumbnail_model: ThumbnailModel,
        gallery_tabs: GalleryTabs,
        gallery_info_bar: GalleryInfoBar,
        filter_bar: FilterBar,
        search_edit: QLineEdit,
        search_timer: QTimer,
        preview_panel: PreviewPanel,
        tag_service: TagService,
        db: Database,
        thumbnail_service: ThumbnailService | None = None,
        max_multi_preview: int = MULTI_PREVIEW_MAX_DEFAULT,
        synchronous_workers: bool | None = None,
    ) -> None:
        self._query_service = query_service
        self._filter_state = filter_state
        self._thumbnail_model = thumbnail_model
        self._gallery_tabs = gallery_tabs
        self._gallery_info_bar = gallery_info_bar
        self._filter_bar = filter_bar
        self._search_edit = search_edit
        self._search_timer = search_timer
        # Debounce timer for preview tag changes (coalesces render bursts).
        self._tags_query_timer = QTimer()
        self._tags_query_timer.setSingleShot(True)
        self._tags_query_timer.setInterval(300)
        self._preview_panel = preview_panel
        self._tag_service = tag_service
        self._db = db
        self._thumbnail_service = thumbnail_service
        self._max_multi_preview = max_multi_preview
        self._synchronous_workers = (
            GalleryController.synchronous_workers if synchronous_workers is None else synchronous_workers
        )
        self._disposed = False

        # Async multi-select preview decode pipeline.
        self.decode_relay = _PreviewDecodeRelay()
        self._decode_pool = QThreadPool()
        self._decode_cancel_event = threading.Event()
        self._decode_epoch = 0
        self._decode_pending = 0
        self.decode_relay.decode_ready.connect(self._on_preview_decoded)
        self.decode_relay.decode_failed.connect(self._on_preview_decode_failed)

        # Current folder for local-scope queries ("" means nothing selected).
        self.current_folder: str = ""

        # ── Wire internal signal connections ────────────────────────
        self._search_edit.textChanged.connect(self.on_search_text_changed)
        self._search_timer.timeout.connect(self.run_filtered_query)
        self._tags_query_timer.timeout.connect(self.run_filtered_query)
        self._filter_bar.color_filter_changed.connect(self.on_color_filter_changed)
        self._filter_bar.tag_filter_changed.connect(self.on_tag_filter_changed)
        self._filter_bar.folder_filter_changed.connect(self.on_folder_filter_changed)
        self._gallery_tabs.scope_changed.connect(self.on_scope_changed)
        self._preview_panel.tags_changed.connect(self.on_preview_tags_changed)

    # ── Filter Handlers ────────────────────────────────────────────

    def on_search_text_changed(self, text: str) -> None:
        """Restart the debounce timer when the search text changes."""
        logger.debug("Called - text: %r", text)
        self._filter_state.filename_filter = text
        self._search_timer.start()

    def on_color_filter_changed(self, color_tags: set[ColorBucket]) -> None:
        """Re-run the filtered query when color filter swatches change."""
        logger.debug("Called - color_tags %s", color_tags)
        # Color-bucket values convert to AUTO_COLOR Tag objects here. The id is
        # a don't-care for the color branch, which only reads get_name().
        self._filter_state.color_tags = {
            Tag(id=0, name=bucket.value, source=TagSource.AUTO_COLOR) for bucket in color_tags
        }
        self.run_filtered_query()

    def on_tag_filter_changed(self, tags: set[Tag]) -> None:
        """Re-run the filtered query when tag filter checkboxes change."""
        logger.debug("Called - tags: %s", tags)
        self._filter_state.tags = tags
        self.run_filtered_query()

    def on_folder_filter_changed(self, folder_paths: set[str]) -> None:
        """Re-run the filtered query when the folder chip selection changes."""
        logger.debug("Called - folder_paths: %s", folder_paths)
        self._filter_state.folder_filters = set(folder_paths)
        self.run_filtered_query()

    def on_scope_changed(self, is_global: bool) -> None:  # noqa: FBT001
        """Handle gallery tab scope change.

        Updates the filter bar scope and re-runs the filtered query.
        """
        logger.debug("Called - is_global: %b", is_global)
        self._filter_bar.set_scope(is_global)
        self.run_filtered_query()
        # Ensure info bar reflects new scope label even if query returned early
        self.update_gallery_info_bar()

    # ── Query Execution ────────────────────────────────────────────

    def run_filtered_query(self) -> None:
        """Execute a QueryService query combining all active filters.

        Combines the current folder scope, filename search text, active
        color-bucket set, and checked tag IDs into a single query, then
        updates the ThumbnailModel with the results.

        In global scope mode (gallery tabs set to All Images), the folder
        constraint is removed so results span the entire database.

        If no folder is currently selected (``current_folder`` is empty)
        and we are NOT in global mode, the method returns without modifying
        the model to avoid clearing the gallery.
        """

        logger.debug("Called")
        start = time.perf_counter()

        # Determine browser scope based on gallery tabs
        is_global = self._gallery_tabs.is_global_scope()

        # Don't clear the gallery if no folder is selected and not in global mode
        if not self.current_folder and not is_global:
            return

        # In global mode, use the folder filter dropdown selection (may be empty set for all)
        # In local mode, use the currently navigated folder
        if is_global:
            folder_filters = self._filter_state.folder_filters
        else:
            folder_filters = {self.current_folder} if self.current_folder else set()

        filename_filter = self._filter_state.filename_filter
        color_tags = self._filter_state.color_tags
        tags = self._filter_state.tags

        results = self._query_service.query(
            folder_filters=folder_filters,
            filename_filter=filename_filter,
            tags=tags,
            color_tags=color_tags,
        )
        elapsed = time.perf_counter() - start
        logger.debug(
            "folders: %s, filename: %r, colors: %s, tags: %s -> %d results in %.3fs",
            folder_filters,
            filename_filter,
            color_tags,
            tags,
            len(results),
            elapsed,
        )

        self._thumbnail_model.set_paths(results)

        # Update gallery info bar with new file count
        self.update_gallery_info_bar()

        # Dispatch thumbnail renders for cache population
        if self._thumbnail_service is not None:
            for path in results:
                # Skip if already cached in model
                if str(path) in self._thumbnail_model._thumbnails:  # noqa: SLF001
                    continue
                try:
                    stat = path.stat()
                    fi = FileInfo.from_stat(path=path, stat_result=stat)
                    self._thumbnail_service.check_and_render(fi)
                except OSError:
                    logger.debug("Could not stat path: %s", path)

    # ── Gallery Info Bar ───────────────────────────────────────────

    def update_gallery_info_bar(self) -> None:
        """Update the gallery info bar with current folder name, file count, and filter count."""
        is_global = self._gallery_tabs.is_global_scope()

        # Determine folder display name
        if is_global:
            folder_name = "All Images"
        elif self.current_folder:
            folder_name = Path(self.current_folder).name or self.current_folder
        else:
            folder_name = ""

        # File count from the model
        file_count = self._thumbnail_model.rowCount()

        self._gallery_info_bar.set_folder_info(folder_name, file_count)

        # Active filter count from the shared FilterState.  Folder filters
        # only constrain the query in global scope — in local scope the
        # navigated folder replaces them, so exclude them from the pill.
        self._gallery_info_bar.set_active_filter_count(self.active_filter_count(is_global=is_global))

    def active_filter_count(self, *, is_global: bool) -> int:
        """Return the count of filters active in the current scope.

        Wraps :meth:`FilterState.active_count` so the displayed pill
        matches the query semantics: folder filters are counted only in
        global scope, because local scope replaces them with the
        navigated ``current_folder``.

        Args:
            is_global: ``True`` when the "All Images" tab is active.
        """
        active_count = self._filter_state.active_count()
        if not is_global:
            active_count -= len(self._filter_state.folder_filters)
        return active_count

    # ── Selection Handling ─────────────────────────────────────────

    def on_selection_changed(self, paths: list[str]) -> None:
        """Handle thumbnail grid selection changes.

        Updates the preview panel (single image or mosaic) and tag display
        based on the current selection. Multi-select previews are decoded
        off the GUI thread so the mosaic fills in as images land.
        """
        logger.debug("Called - paths: %s", paths)
        if len(paths) == 0:
            self._cancel_decode_batch()
            self._preview_panel.clear()
        elif len(paths) == 1:
            # Single selection stays synchronous: one image is cheap to load.
            self._cancel_decode_batch()
            path = Path(paths[0])
            try:
                loaded = self._load_preview_image(path)
                self._preview_panel.set_image(
                    ImageInfo(
                        loaded.image,
                        path,
                        loaded.original_width,
                        loaded.original_height,
                        loaded.from_cache,
                    )
                )
            except Exception:
                logger.warning("Failed to load preview for %s", path, exc_info=True)
                self._preview_panel.clear()
        else:
            self._begin_multi_decode(paths)

        # Update tags in preview panel
        self.update_preview_tags(paths)

    def _begin_multi_decode(self, paths: list[str]) -> None:
        """Decode each selected image off the GUI thread, one QRunnable per image.

        Respects the multi-preview cap; a selection change mid-decode
        discards stale results via the epoch guard in the completion slots.
        """
        self._cancel_decode_batch()
        selected = [Path(p) for p in paths[: self._max_multi_preview]]
        self._preview_panel.begin_multi_preview()
        self._decode_pending = len(selected)
        if self._decode_pending > 0:
            self.decode_relay.decode_busy_started.emit()
        for path in selected:
            task = _DecodePreviewTask(
                db=self._db,
                path=path,
                relay=self.decode_relay,
                epoch=self._decode_epoch,
                cancel_event=self._decode_cancel_event,
            )
            self._start_decode_task(task)

    def _start_decode_task(self, task: _DecodePreviewTask) -> None:
        """Dispatch *task* to the decode pool, or run it inline under the test seam."""
        if self._synchronous_workers:
            task.run()
        else:
            self._decode_pool.start(task)

    def _cancel_decode_batch(self) -> None:
        """Abort any in-flight decode batch; stale results are discarded by epoch."""
        self._decode_epoch += 1
        self._decode_cancel_event.set()
        self._decode_pool.clear()
        self._decode_cancel_event.clear()
        if self._decode_pending > 0:
            self._decode_pending = 0
            self.decode_relay.decode_busy_finished.emit()

    def shutdown(self, timeout_ms: int = DECODE_POOL_DRAIN_TIMEOUT_MS) -> None:
        """Shut down the preview decode pool (idempotent).

        Cancels any pending decode batch and waits for in-flight decodes
        to finish, so a late worker can never emit ``decode_ready`` after
        the window's receivers are gone. Safe to call more than once; the
        base MainWindow closeEvent and the application subclass in main.py
        may both invoke it on the way out.
        """
        self._cancel_decode_batch()
        self._decode_pool.waitForDone(timeout_ms)

    def dispose(self) -> None:
        """Tear down this controller's runtime resources (idempotent).

        Lifecycle teardown for tests and embedders: drains the preview
        decode pool, detaches this controller's decode relay slots
        (``decode_ready``/``decode_failed`` handlers connected in
        ``__init__``), and releases the widget tree this controller
        constructed. The relay object itself and any connections made by
        other owners (e.g. the MainWindow busy indicator) are left intact.
        Safe to call more than once; the first call does the work and
        later calls are no-ops.

        The MainWindow base closeEvent already calls :meth:`shutdown`
        for the production path; tests and embedders that construct a
        controller directly should call this instead of reaching into
        the individual widgets.
        """
        if self._disposed:
            return
        self._disposed = True
        self.shutdown()
        # Detach the decode relay slots this controller connected in
        # __init__. The former loop relied on QObject.receivers() with bare
        # signal names, which returns 0 for connected signals unless the
        # index-encoded form is used, so the disconnect never fired; an
        # explicit per-slot disconnect is unambiguous and verifiable.
        self.decode_relay.decode_ready.disconnect(self._on_preview_decoded)
        self.decode_relay.decode_failed.disconnect(self._on_preview_decode_failed)
        self._search_timer.stop()
        self._tags_query_timer.stop()
        self._dispose_widget(self._preview_panel)
        self._dispose_widget(self._gallery_tabs)
        self._dispose_widget(self._gallery_info_bar)
        self._dispose_widget(self._filter_bar)
        self._dispose_widget(self._search_edit)

    @staticmethod
    def _dispose_widget(widget: QWidget | None) -> None:
        """Close and schedule deletion for one owned widget (idempotent).

        ``close()`` alone leaves a parentless widget installed as a
        top-level window, and the widget's internal signal closures keep
        the C++ tree alive even after every Python reference is dropped
        (PySide wrappers are not cyclic-gc tracked).  Scheduling deletion
        actually destroys the C++ side, which releases those closures, so
        the whole tree becomes collectible once the deferred deletes are
        processed.

        The deferred delete is delivered for *this widget only*.  Flushing
        every pending DeferredDelete in the process would also deliver
        stale events left by objects whose Python wrappers were already
        garbage-collected; the C++ object behind such an event may already
        be gone, so Qt would delete a dangling pointer (SIGSEGV).

        Only :class:`~PySide6.QtWidgets.QWidget` instances are touched:
        a controller wired with plain QObjects or lightweight test
        doubles must no-op safely.
        """
        if widget is None or not isinstance(widget, QWidget):
            return
        widget.close()
        widget.deleteLater()
        QCoreApplication.sendPostedEvents(widget, QEvent.Type.DeferredDelete)

    def _on_preview_decoded(
        self,
        epoch: int,
        path_str: str,
        img: Image.Image,
        orig_w: int | None,
        orig_h: int | None,
        from_cache: bool,
    ) -> None:
        """Add one decoded image to the mosaic, discarding stale selections."""
        if epoch != self._decode_epoch:
            return
        # Completion accounting runs BEFORE the panel mutation so a raise in
        # add_multi_preview_image can never leave the pending counter stuck
        # and decode_busy_finished is never skipped.
        self._decode_pending -= 1
        if self._decode_pending <= 0:
            self.decode_relay.decode_busy_finished.emit()
        self._preview_panel.add_multi_preview_image(ImageInfo(img, Path(path_str), orig_w, orig_h, from_cache))

    def _on_preview_decode_failed(self, epoch: int, path_str: str, error_message: str) -> None:
        """Log a decode failure and count it toward batch completion."""
        if epoch != self._decode_epoch:
            return
        logger.debug("Failed to load preview for multi-select: %s (%s)", path_str, error_message)
        self._decode_pending -= 1
        if self._decode_pending <= 0:
            # A batch with zero successful decodes must not leave the
            # "Loading previews..." placeholder stuck forever.
            self._preview_panel.show_empty_mosaic_state()
            self.decode_relay.decode_busy_finished.emit()

    def update_preview_tags(self, paths: list[str]) -> None:
        """Fetch and display tags for the current selection in the preview panel.

        For single selection, shows that file's tags.
        For multi-selection, shows the union of all files' tags with tri-state opacity.
        For no selection, clears tags.

        Args:
            paths: Currently selected file paths.
        """
        logger.debug("Called - paths: %s", paths)
        if not paths:
            self._preview_panel.set_tags(set(), selected_paths=[])
            return

        if len(paths) == 1:
            tags = self._tag_service.get_tags_for_file(paths[0])
            self._preview_panel.set_tags(tags, selected_paths=paths)
        else:
            # Multi-selection: get union of all tags
            union_tags = self._preview_panel.get_union_tags(paths)
            self._preview_panel.set_tags(union_tags, selected_paths=paths)

    def on_preview_tags_changed(self) -> None:
        """Restart the debounce timer when preview tags change externally."""
        # Tag edits cannot change results unless a tag or color tag filter is active.
        if not self._filter_state.tags and not self._filter_state.color_tags:
            logger.debug("Skipped - no tag filter active")
            return
        logger.debug("Called - scheduling re-query")
        self._tags_query_timer.start()

    # ── Preview Image Loading ──────────────────────────────────────

    def _load_preview_image(self, path: Path) -> LoadedImage:
        """Load a preview image synchronously (single selection).

        Delegates to the shared worker-safe loader, which prefers the
        preview-tier cached image when available and applies EXIF
        orientation to the original-file fallback.
        """
        return _load_preview_image_data(self._db, path)
