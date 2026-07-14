"""GalleryController — filter orchestration and selection handling for the gallery view.

Extracts the filter query pipeline (color, tag, folder, search text, scope)
and thumbnail selection → preview rendering from MainWindow, keeping the
window class focused on layout, menus, and dock management.
"""

from __future__ import annotations

import logging
import time
from pathlib import Path

from PIL import Image
from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QLineEdit

from tarragon.db import Database
from tarragon.models.filter_state import FilterState
from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.scanner import FileInfo
from tarragon.services.query_service import QueryService
from tarragon.services.tag_service import TagService
from tarragon.services.thumbnail_service import ThumbnailService
from tarragon.theme.layout import MULTI_PREVIEW_MAX_DEFAULT
from tarragon.widgets.filter_bar import FilterBar
from tarragon.widgets.gallery_info_bar import GalleryInfoBar
from tarragon.widgets.gallery_tabs import GalleryTabs
from tarragon.widgets.preview_panel import PreviewPanel

logger = logging.getLogger(__name__)


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
    ) -> None:
        self._query_service = query_service
        self._filter_state = filter_state
        self._thumbnail_model = thumbnail_model
        self._gallery_tabs = gallery_tabs
        self._gallery_info_bar = gallery_info_bar
        self._filter_bar = filter_bar
        self._search_edit = search_edit
        self._search_timer = search_timer
        self._preview_panel = preview_panel
        self._tag_service = tag_service
        self._db = db
        self._thumbnail_service = thumbnail_service
        self._max_multi_preview = max_multi_preview

        # Current folder for local-scope queries ("" means nothing selected).
        self.current_folder: str = ""

        # ── Wire internal signal connections ────────────────────────
        self._search_edit.textChanged.connect(self.on_search_text_changed)
        self._search_timer.timeout.connect(self.run_filtered_query)
        self._filter_bar.color_filter_changed.connect(self.on_color_filter_changed)
        self._filter_bar.tag_filter_changed.connect(self.on_tag_filter_changed)
        self._filter_bar.folder_filter_changed.connect(self.on_folder_filter_changed)
        self._gallery_tabs.scope_changed.connect(self.on_scope_changed)
        self._preview_panel.tags_changed.connect(self.on_preview_tags_changed)

    # ── Filter Handlers ────────────────────────────────────────────

    def on_search_text_changed(self, text: str) -> None:
        """Restart the debounce timer when the search text changes."""
        logger.debug("Search text changed: %r", text)
        self._filter_state.filename_filter = text
        self._search_timer.start()

    def on_color_filter_changed(self, color_tags: set[str]) -> None:
        """Re-run the filtered query when color filter swatches change."""
        logger.debug("Color filter changed: %s", color_tags)
        self._filter_state.color_tags = set(color_tags)
        self.run_filtered_query()

    def on_tag_filter_changed(self, tag_ids: set[int]) -> None:
        """Re-run the filtered query when tag filter checkboxes change."""
        logger.debug("Tag filter changed: %s", tag_ids)
        self._filter_state.tag_ids = set(tag_ids)
        self.run_filtered_query()

    def on_folder_filter_changed(self, folder_paths: set[str]) -> None:
        """Re-run the filtered query when the folder chip selection changes."""
        logger.debug("Folder filter changed: %s", folder_paths)
        self._filter_state.folder_filters = set(folder_paths)
        self.run_filtered_query()

    def on_scope_changed(self, is_global: bool) -> None:  # noqa: FBT001
        """Handle gallery tab scope change.

        Updates the filter bar scope and re-runs the filtered query.
        """
        logger.debug("Scope changed: %s", "global" if is_global else "local")
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
        if self._query_service is None:
            return

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
        tag_ids = self._filter_state.tag_ids

        results = self._query_service.query(
            folder_filters=folder_filters,
            filename_filter=filename_filter,
            tag_ids=tag_ids,
            color_tags=color_tags,
        )
        elapsed = time.perf_counter() - start
        logger.debug(
            "Filtered query: folders=%s, filename=%r, colors=%s, tags=%s → %d results in %.3fs",
            folder_filters,
            filename_filter,
            color_tags,
            tag_ids,
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
                    fi = FileInfo(
                        path=path,
                        mtime=stat.st_mtime,
                        size=stat.st_size,
                        extension=path.suffix.lower(),
                    )
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

        # Active filter count: tag_ids + color_tags + folder_filters + filename_filter
        active_count = (
            len(self._filter_state.tag_ids)
            + len(self._filter_state.color_tags)
            + len(self._filter_state.folder_filters)
            + (1 if self._filter_state.filename_filter else 0)
        )
        self._gallery_info_bar.set_active_filter_count(active_count)

    # ── Selection Handling ─────────────────────────────────────────

    def on_selection_changed(self, paths: list[str]) -> None:
        """Handle thumbnail grid selection changes.

        Updates the preview panel (single image or mosaic) and tag display
        based on the current selection.

        For single selections, prefers the cached master image (Deviation 1.3)
        to avoid re-compositing PSDs on every click.
        """
        if len(paths) == 0:
            self._preview_panel.clear()
        elif len(paths) == 1:
            # Single selection — load image and show
            path = Path(paths[0])
            try:
                img, orig_w, orig_h = self._load_preview_image(path)
                self._preview_panel.set_image(
                    img,
                    path,
                    original_width=orig_w,
                    original_height=orig_h,
                )
            except Exception:
                logger.warning("Failed to load preview for %s", path, exc_info=True)
                self._preview_panel.clear()
        else:
            # Multi-select — load multiple images for mosaic
            images_to_show = min(len(paths), self._max_multi_preview)
            images: list[Image.Image] = []
            for p in paths[:images_to_show]:
                try:
                    img, _orig_w, _orig_h = self._load_preview_image(Path(p))
                    images.append(img)
                except Exception:
                    logger.debug("Failed to load preview for multi-select: %s", p, exc_info=True)
            self._preview_panel.set_multi_preview(images, len(paths), self._max_multi_preview)

        # Update tags in preview panel
        self.update_preview_tags(paths)

    def update_preview_tags(self, paths: list[str]) -> None:
        """Fetch and display tags for the current selection in the preview panel.

        For single selection, shows that file's tags.
        For multi-selection, shows the union of all files' tags with tri-state opacity.
        For no selection, clears tags.

        Args:
            paths: Currently selected file paths.
        """
        if not paths:
            self._preview_panel.set_tags([], selected_paths=[])
            return

        if len(paths) == 1:
            tags = self._tag_service.get_tags_for_file(paths[0])
            self._preview_panel.set_tags(tags, selected_paths=paths)
        else:
            # Multi-selection: get union of all tags
            union_tags = self._preview_panel.get_union_tags(paths)
            self._preview_panel.set_tags(union_tags, selected_paths=paths)

    def on_preview_tags_changed(self) -> None:
        """Handle tag changes from the preview panel — refresh gallery query."""
        logger.debug("Tags changed via preview panel, refreshing gallery")
        self.run_filtered_query()

    # ── Preview Image Loading ──────────────────────────────────────

    def _load_preview_image(self, path: Path) -> tuple[Image.Image, int | None, int | None]:
        """Load a preview image, preferring the 1024px cached preview when available.

        Checks ``db.get_thumbnail(path)`` for a ``preview_cache_path`` (1024px).
        If the cached file exists on disk it is opened directly (good quality,
        fast).  Falls back to ``full_cache_path``, then to the original file.

        Images loaded from cache are marked with ``_from_cache = True`` so that
        ``PreviewPanel.set_image()`` can skip EXIF recovery from the original
        file (the cache already has correct orientation).

        Returns:
            A tuple of ``(image, original_width, original_height)``.
            ``original_width`` and ``original_height`` are extracted from the
            database record when available, so the preview panel can display
            the true dimensions even when showing a downscaled thumbnail.
        """
        thumb_record = self._db.get_thumbnail(str(path))

        # Extract original dimensions from DB record (if available)
        original_width: int | None = None
        original_height: int | None = None
        if thumb_record:
            original_width = thumb_record.get("width")
            original_height = thumb_record.get("height")

        if thumb_record:
            # Try 1024px preview first (good quality, fast)
            preview_path = thumb_record.get("preview_cache_path")
            if preview_path and Path(preview_path).is_file():
                img = Image.open(preview_path)
                img._from_cache = True
                return img, original_width, original_height

            # Fallback: full resolution cache
            full_path = thumb_record.get("full_cache_path")
            if full_path and Path(full_path).is_file():
                img = Image.open(full_path)
                img._from_cache = True
                return img, original_width, original_height

        # Fallback: open the original file directly
        return Image.open(path), original_width, original_height
