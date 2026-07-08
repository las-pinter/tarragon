"""Main application window with dock panels (Library, Gallery, Preview, Log)."""

from __future__ import annotations

import logging
import sqlite3
import time
from pathlib import Path
from typing import Any

from PIL import Image
from PySide6.QtCore import Qt, QTimer
from PySide6.QtGui import QAction, QIcon
from PySide6.QtWidgets import (
    QApplication,
    QDockWidget,
    QFileDialog,
    QLineEdit,
    QMainWindow,
    QMenu,
    QVBoxLayout,
    QWidget,
)

from tarragon.db import Database
from tarragon.models import FilterState
from tarragon.scanner import FileInfo
from tarragon.services.query_service import QueryService
from tarragon.services.settings_service import SettingsService
from tarragon.services.tag_service import TagService
from tarragon.services.thumbnail_service import ThumbnailService
from tarragon.settings import Settings
from tarragon.widgets.filter_bar import FilterBar
from tarragon.widgets.gallery_info_bar import GalleryInfoBar
from tarragon.widgets.gallery_tabs import GalleryTabs
from tarragon.widgets.log_panel import LogPanel, QtLogHandler, apply_debug_level
from tarragon.widgets.preview_panel import PreviewPanel
from tarragon.widgets.thumbnail_grid import ThumbnailGrid

logger = logging.getLogger(__name__)


class MainWindow(QMainWindow):
    """Main application window with four dockable panels.

    Docks:
        - sidebar_dock : "Library"  — left panel for library/navigation
        - grid_dock    : "Gallery"  — central panel for thumbnail gallery
        - preview_dock : "Preview"  — right panel for image preview + tag management
        - log_dock     : "Log"      — bottom panel for application log output

    Menu actions (current milestone):
        - File → Open Folder (wired in M3, placeholder in M2)
        - View → Toggle visibility of each dock panel
    """

    DEFAULT_WIDTH = 1200
    DEFAULT_HEIGHT = 800

    def __init__(self, settings: Settings | None = None) -> None:
        """Initialize the main window.

        Args:
            settings: Optional Settings instance for configuration access.
                       Stored as an attribute; not used until later milestones.
        """
        super().__init__()
        self._settings = settings
        self._settings_service = SettingsService(settings) if settings else None
        self.setWindowTitle("Tarragon")
        self.resize(self.DEFAULT_WIDTH, self.DEFAULT_HEIGHT)

        # ── Dock panels (created before actions so they're valid widgets) ──
        self.sidebar_dock: QDockWidget
        self.grid_dock: QDockWidget
        self.preview_dock: QDockWidget
        self.log_dock: QDockWidget
        self._create_docks()

        # ── Menu bar and actions ────────────────────────────────────────
        self._setup_actions()

        # ── Apply theme (QSS stylesheet) ────────────────────────────────
        # Cascades to all child widgets (including those created later in setup_widgets()).
        # Widget-level setStyleSheet() calls will override for specific widgets.
        self._apply_theme()

    # ── Dock Widget Creation ───────────────────────────────────────────

    def _create_docks(self) -> None:
        """Create and arrange dock widgets.

        Layout::

            +----------+-------------------+----------+
            | Library  |                   | Preview  |
            |          |     Gallery       |          |
            |          |                   |          |
            |          |                   |          |
            +----------+-------------------+----------+

        The Log dock is added to the bottom area but hidden by default.
        """
        # Enable dock nesting - required for split layouts
        self.setDockNestingEnabled(True)

        # Create docks
        self.sidebar_dock = QDockWidget("Library", self)
        self.sidebar_dock.setAllowedAreas(Qt.DockWidgetArea.LeftDockWidgetArea | Qt.DockWidgetArea.RightDockWidgetArea)

        self.grid_dock = QDockWidget("Gallery", self)
        self.grid_dock.setAllowedAreas(
            Qt.DockWidgetArea.LeftDockWidgetArea
            | Qt.DockWidgetArea.RightDockWidgetArea
            | Qt.DockWidgetArea.TopDockWidgetArea
            | Qt.DockWidgetArea.BottomDockWidgetArea
        )

        self.preview_dock = QDockWidget("Preview", self)
        self.preview_dock.setAllowedAreas(Qt.DockWidgetArea.LeftDockWidgetArea | Qt.DockWidgetArea.RightDockWidgetArea)

        self.log_dock = QDockWidget("Log", self)
        self.log_dock.setAllowedAreas(
            Qt.DockWidgetArea.LeftDockWidgetArea
            | Qt.DockWidgetArea.RightDockWidgetArea
            | Qt.DockWidgetArea.BottomDockWidgetArea
        )

        # Add docks in the correct order for the desired layout:
        # 1. Gallery as the CENTRAL widget — fills the middle of the window
        self.setCentralWidget(self.grid_dock)

        # 2. Library on the left
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.sidebar_dock)

        # 3. Preview on the right
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self.preview_dock)

        # 4. Log at the bottom (hidden by default)
        self.addDockWidget(Qt.DockWidgetArea.BottomDockWidgetArea, self.log_dock)

        # Set initial dock sizes — preview panel should be roughly half the
        # remaining width after the sidebar, matching the gallery area.
        sidebar_width = 220
        preview_width = (self.width() - sidebar_width) // 2  # 490 at 1200px
        self.resizeDocks(
            [self.sidebar_dock, self.preview_dock],
            [sidebar_width, preview_width],
            Qt.Orientation.Horizontal,
        )

    # ── Widget Setup ─────────────────────────────────────────────────

    def setup_widgets(self, db: Database, tag_service: TagService) -> None:
        """Create and wire the main content widgets into dock panels.

        Args:
            db: Database instance for sidebar favorites.
            tag_service: TagService instance for tag management in preview panel.
        """
        from tarragon.models.thumbnail_model import ThumbnailModel
        from tarragon.widgets.sidebar import SidebarWidget

        # Store db for editor launch and preview cache lookups
        self._db = db
        self._tag_service = tag_service

        # Query service for filtered gallery queries
        self._query_service = QueryService(db)
        self._filter_state = FilterState()
        self._current_folder: str = ""

        # Sidebar
        self.sidebar_widget = SidebarWidget(db, parent=self)
        self.sidebar_dock.setWidget(self.sidebar_widget)

        # Connect sidebar signals
        self.sidebar_widget.folder_navigated.connect(self._on_folder_navigated)
        self.sidebar_widget.favorite_clicked.connect(self._on_favorite_clicked)

        # Preview panel (wiv tag management)
        self.preview_panel = PreviewPanel(tag_service=tag_service, parent=self)
        self.preview_dock.setWidget(self.preview_panel)
        # When tags change via preview panel, refresh the gallery query
        self.preview_panel.tags_changed.connect(self._on_preview_tags_changed)

        # Thumbnail model and grid
        self.thumbnail_model = ThumbnailModel(parent=self)
        self.thumbnail_grid = ThumbnailGrid(parent=self)
        self.thumbnail_grid.set_model(self.thumbnail_model)

        # Create thumbnail service (skip if settings_service is None, e.g. in tests)
        if self._settings_service is not None:
            self._thumbnail_service = ThumbnailService(db, self._settings_service, parent=self)
            self._thumbnail_service.thumbnailReady.connect(self._on_thumbnail_ready)
            self._thumbnail_service.errorOccurred.connect(self._on_thumbnail_error)
            # Auto-color tags from thumbnail rendering should refresh the tag panel
            self._thumbnail_service.tagsUpdated.connect(tag_service.tagsChanged.emit)

        # ── Search box (Deviation 4.5) ─────────────────────────────────
        self._search_edit = QLineEdit()
        self._search_edit.setObjectName("searchEdit")
        self._search_edit.setPlaceholderText("Search files and tags")
        self._search_edit.setClearButtonEnabled(True)
        self._search_edit.textChanged.connect(self._on_search_text_changed)

        # Search icon on the left side (Tabler-style magnifying glass)
        _search_icon_path = str(Path(__file__).parent / "theme" / "icons" / "search.svg")
        search_action = QAction(self._search_edit)
        search_action.setIcon(QIcon(_search_icon_path))
        self._search_edit.addAction(search_action, QLineEdit.ActionPosition.LeadingPosition)

        # ── Combined filter bar (colour + tag + folder filters) ────
        self.filter_bar = FilterBar(tag_service, db, parent=self)
        # Backward-compatible references for existing code and tests
        self.color_filter_bar = self.filter_bar.color_filter_bar
        self.tag_filter_bar = self.filter_bar.tag_filter_bar
        self.filter_bar.color_filter_changed.connect(self._on_color_filter_changed)
        self.filter_bar.tag_filter_changed.connect(self._on_tag_filter_changed)
        self.filter_bar.folder_filter_changed.connect(self._on_folder_filter_changed)

        # ── Gallery tabs (Folder / All Images) ─────────────────────────
        self._gallery_tabs = GalleryTabs(parent=self)
        self._gallery_tabs.scope_changed.connect(self._on_scope_changed)

        # ── Gallery info bar (folder name + active filter pill) ────────
        self._gallery_info_bar = GalleryInfoBar(parent=self)

        # Gallery container: tabs + search + info bar + filter bar + grid stacked vertically
        gallery_container = QWidget()
        gallery_layout = QVBoxLayout(gallery_container)
        gallery_layout.setContentsMargins(0, 0, 0, 0)
        gallery_layout.addWidget(self._gallery_tabs)  # FIRST!
        gallery_layout.addWidget(self._search_edit)
        gallery_layout.addWidget(self._gallery_info_bar)
        gallery_layout.addWidget(self.filter_bar)
        gallery_layout.addWidget(self.thumbnail_grid, stretch=1)
        self.grid_dock.setWidget(gallery_container)

        # Log panel — application log output in dedicated dock
        self.log_panel = LogPanel(parent=self)
        self.log_dock.setWidget(self.log_panel)
        self.log_dock.hide()

        # Set up logging handler to route Python logs into the log panel
        self._log_handler = QtLogHandler(self.log_panel)
        self._log_handler.setFormatter(
            logging.Formatter("[%(asctime)s] %(levelname)-8s %(name)s: %(message)s", datefmt="%H:%M:%S")
        )
        root_logger = logging.getLogger()
        root_logger.addHandler(self._log_handler)
        apply_debug_level(self._settings_service.get_debug_mode() if self._settings_service else False)

        # Wire gallery tabs scope to tag panel and re-run query
        # (signal connected above when _gallery_tabs was created)

        # Debounce timer for filename search (Deviation 4.5)
        self._search_timer = QTimer()
        self._search_timer.setSingleShot(True)
        self._search_timer.setInterval(300)
        self._search_timer.timeout.connect(self._run_filtered_query)

        # Wire selection signal
        self.thumbnail_grid.selection_changed.connect(self._on_selection_changed)

        # Wire double-click signal for editor launch
        self.thumbnail_grid.file_double_clicked.connect(self._on_file_double_clicked)

        # Wire regenerate signal for manual thumbnail regeneration
        self.thumbnail_grid.regenerate_requested.connect(self._on_regenerate_requested)

    def _on_thumbnail_ready(
        self, source_path: str, img: object, resolution_size: int | None, cache_path: str | None
    ) -> None:
        """Handle thumbnail render complete — update model with cache path.

        Parameters
        ----------
        source_path:
            The original source file path.
        img:
            The rendered PIL Image (or None on failure).
        resolution_size:
            The resolution tier (256, 1024, or None for full resolution).
        cache_path:
            The cache file path (str or None).
        """
        if img is None or cache_path is None:
            return
        # Update model with the emitted cache path
        self.thumbnail_model.set_thumbnail(source_path, Path(cache_path), resolution=resolution_size)

    def _on_thumbnail_error(self, source_path: str, error_message: str) -> None:
        """Handle thumbnail render error."""
        logger.error("Thumbnail render failed for %s: %s", source_path, error_message)

    def _on_selection_changed(self, paths: list[str]) -> None:
        """Handle thumbnail grid selection changes.

        Updates the preview panel (single image or mosaic) and tag display
        based on the current selection.

        For single selections, prefers the cached master image (Deviation 1.3)
        to avoid re-compositing PSDs on every click.
        """
        if len(paths) == 0:
            self.preview_panel.clear()
        elif len(paths) == 1:
            # Single selection — load image and show
            path = Path(paths[0])
            try:
                img, orig_w, orig_h = self._load_preview_image(path)
                self.preview_panel.set_image(
                    img,
                    path,
                    original_width=orig_w,
                    original_height=orig_h,
                )
            except Exception:
                logger.warning("Failed to load preview for %s", path, exc_info=True)
                self.preview_panel.clear()
        else:
            # Multi-select — load multiple images for mosaic
            # Read cap from settings instead of hardcoding (Deviation 4.2)
            cap = self._settings_service.get_max_multi_preview() if self._settings_service else 9
            if cap is None:
                cap = 9
            images_to_show = min(len(paths), cap)
            images = []
            for p in paths[:images_to_show]:
                try:
                    img, _orig_w, _orig_h = self._load_preview_image(Path(p))
                    images.append(img)
                except Exception:
                    logger.debug("Failed to load preview for multi-select: %s", p, exc_info=True)
            self.preview_panel.set_multi_preview(images, len(paths), cap)

        # Update tags in preview panel
        self._update_preview_tags(paths)

    def _update_preview_tags(self, paths: list[str]) -> None:
        """Fetch and display tags for the current selection in the preview panel.

        For single selection, shows that file's tags.
        For multi-selection, shows the union of all files' tags with tri-state opacity.
        For no selection, clears tags.

        Args:
            paths: Currently selected file paths.
        """
        if not paths:
            self.preview_panel.set_tags([], selected_paths=[])
            return

        if len(paths) == 1:
            tags = self._tag_service.get_tags_for_file(paths[0])
            self.preview_panel.set_tags(tags, selected_paths=paths)
        else:
            # Multi-selection: get union of all tags
            union_tags = self.preview_panel.get_union_tags(paths)
            self.preview_panel.set_tags(union_tags, selected_paths=paths)

    def _on_preview_tags_changed(self) -> None:
        """Handle tag changes from the preview panel — refresh gallery query."""
        logger.debug("Tags changed via preview panel, refreshing gallery")
        self._run_filtered_query()

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

    def _on_file_double_clicked(self, path: str) -> None:
        """Handle double-click on a thumbnail — launch external editor."""
        from tarragon.editors import launch_editor

        file_path = Path(path)
        extension = file_path.suffix
        launch_editor(self._db, file_path, extension)

    def _on_regenerate_requested(self, file_path: str) -> None:
        """Handle 'Regenerate Thumbnail' context menu action."""
        path = Path(file_path)
        if hasattr(self, "_thumbnail_service"):
            logger.info("Regenerating thumbnail for %s", path.name)
            self._thumbnail_service.invalidate_and_render(path)

    # ── Filtered Query Helpers ─────────────────────────────────────────

    def _update_gallery_info_bar(self) -> None:
        """Update the gallery info bar with current folder name, file count, and filter count."""
        if not hasattr(self, "_gallery_info_bar"):
            return

        is_global = hasattr(self, "_gallery_tabs") and self._gallery_tabs.is_global_scope()

        # Determine folder display name
        if is_global:
            folder_name = "All Images"
        elif self._current_folder:
            folder_name = Path(self._current_folder).name or self._current_folder
        else:
            folder_name = ""

        # File count from the model
        file_count = self.thumbnail_model.rowCount() if hasattr(self, "thumbnail_model") else 0

        self._gallery_info_bar.set_folder_info(folder_name, file_count)

        # Active filter count: tag_ids + color_tags + folder_filters + filename_filter
        active_count = (
            len(self._filter_state.tag_ids)
            + len(self._filter_state.color_tags)
            + len(self._filter_state.folder_filters)
            + (1 if self._filter_state.filename_filter else 0)
        )
        self._gallery_info_bar.set_active_filter_count(active_count)

    def _on_search_text_changed(self, text: str) -> None:
        """Restart the debounce timer when the search text changes."""
        logger.debug("Search text changed: %r", text)
        self._filter_state.filename_filter = text
        self._search_timer.start()

    def _on_color_filter_changed(self, color_tags: set[str]) -> None:
        """Re-run the filtered query when color filter swatches change."""
        logger.debug("Color filter changed: %s", color_tags)
        self._filter_state.color_tags = set(color_tags)
        self._run_filtered_query()

    def _on_tag_filter_changed(self, tag_ids: set[int]) -> None:
        """Re-run the filtered query when tag filter checkboxes change."""
        logger.debug("Tag filter changed: %s", tag_ids)
        self._filter_state.tag_ids = set(tag_ids)
        self._run_filtered_query()

    def _on_folder_filter_changed(self, folder_paths: set[str]) -> None:
        """Re-run the filtered query when the folder chip selection changes."""
        logger.debug("Folder filter changed: %s", folder_paths)
        self._filter_state.folder_filters = set(folder_paths)
        self._run_filtered_query()

    def _on_scope_changed(self, is_global: bool) -> None:  # noqa: FBT001
        """Handle gallery tab scope change.

        Updates the filter bar scope and re-runs the filtered query.
        """
        logger.debug("Scope changed: %s", "global" if is_global else "local")
        self.filter_bar.set_scope(is_global)
        self._run_filtered_query()
        # Ensure info bar reflects new scope label even if query returned early
        self._update_gallery_info_bar()

    def _run_filtered_query(self) -> None:
        """Execute a QueryService query combining all active filters.

        Combines the current folder scope, filename search text, active
        color-bucket set, and checked tag IDs into a single query, then
        updates the ThumbnailModel with the results.

        In global scope mode (gallery tabs set to All Images), the folder
        constraint is removed so results span the entire database.

        If no folder is currently selected (``_current_folder`` is empty)
        and we are NOT in global mode, the method returns without modifying
        the model to avoid clearing the gallery.
        """
        if not hasattr(self, "_query_service"):
            return

        start = time.perf_counter()

        # Determine browser scope based on gallery tabs
        is_global = hasattr(self, "_gallery_tabs") and self._gallery_tabs.is_global_scope()

        # Don't clear the gallery if no folder is selected and not in global mode
        if not self._current_folder and not is_global:
            return

        # In global mode, use the folder filter dropdown selection (may be empty set for all)
        # In local mode, use the currently navigated folder
        if is_global:
            folder_filters = self._filter_state.folder_filters
        else:
            folder_filters = {self._current_folder} if self._current_folder else set()

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

        self.thumbnail_model.set_paths(results)

        # Update gallery info bar with new file count
        self._update_gallery_info_bar()

        # Dispatch thumbnail renders for cache population
        if hasattr(self, "_thumbnail_service"):
            for path in results:
                # Skip if already cached in model
                if str(path) in self.thumbnail_model._thumbnails:
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

    # ── Menu Actions ───────────────────────────────────────────────────

    def _setup_actions(self) -> None:
        """Build the menu bar with File and View menus.

        The Open Folder action is wired into the menu system here but does
        nothing yet (full folder-scanning implementation lands in M3).
        The View menu provides toggle actions for all dock panels.
        """
        menubar = self.menuBar()

        # ── File menu ───────────────────────────────────────────────────
        file_menu: QMenu = menubar.addMenu("&File")

        open_folder_action: QAction = file_menu.addAction("Open &Folder…")
        open_folder_action.setStatusTip("Open a folder to browse images")
        open_folder_action.triggered.connect(self._on_open_folder)

        # ── View menu ──────────────────────────────────────────────────
        view_menu: QMenu = menubar.addMenu("&View")

        # Add toggle actions for each dock widget
        for dock in (self.sidebar_dock, self.grid_dock, self.preview_dock, self.log_dock):
            action = dock.toggleViewAction()
            view_menu.addAction(action)

        # ── Settings menu ─────────────────────────────────────────────
        settings_menu: QMenu = menubar.addMenu("&Settings")
        preferences_action: QAction = settings_menu.addAction("&Preferences...")
        preferences_action.triggered.connect(self._show_preferences)

    def _show_preferences(self) -> None:
        """Open the preferences dialog."""
        from tarragon.app_paths import set_cache_dir
        from tarragon.widgets.settings_dialog import SettingsDialog

        if self._settings_service is None:
            return
        dialog = SettingsDialog(self._settings_service, parent=self)
        if dialog.exec():
            # Settings were saved, apply changes
            # Update debug logging level
            apply_debug_level(self._settings_service.get_debug_mode())
            # Apply cache dir change immediately
            new_cache = self._settings_service.get_cache_dir()
            set_cache_dir(Path(new_cache) if new_cache else None)

    def _apply_theme(self) -> None:
        """Generate and apply the QSS stylesheet from design tokens."""
        from tarragon.theme.loader import load_and_generate_qss

        qss_content = load_and_generate_qss()
        app = QApplication.instance()
        if isinstance(app, QApplication):
            app.setStyleSheet(qss_content)
        logger.debug("Theme applied successfully")

    def _on_open_folder(self) -> None:
        """Callback for File → Open Folder — scan folder and populate grid."""
        folder = QFileDialog.getExistingDirectory(self, "Open Folder", "")
        if not folder:
            return
        self._navigate_to_folder(Path(folder))

    def _on_folder_navigated(self, folder_path: str) -> None:
        """Handle folder selection from sidebar tree."""
        folder = Path(folder_path)
        if not folder.exists() or not folder.is_dir():
            return
        self._navigate_to_folder(folder)

    def _navigate_to_folder(self, folder_path: Path) -> None:
        """Shared folder navigation: scan, update grid, render thumbnails, update sidebar.

        Both ``_on_open_folder`` and ``_on_folder_navigated`` delegate here
        after obtaining / validating the target path.
        """
        from tarragon.scanner import scan_folder

        logger.info("Scanning folder: %s", folder_path)

        # Cancel pending thumbnail generation from the previous folder
        # before scanning the new one, then reset the cancel flag.
        if hasattr(self, "_thumbnail_service"):
            self._thumbnail_service.cancel_pending()
            self._thumbnail_service.reset_cancel()

        # Store current folder for filtered queries
        self._current_folder = str(folder_path)

        # Scan folder for images
        file_infos = scan_folder(folder_path)
        if not file_infos:
            logger.warning("No images found in %s", folder_path)
            self.thumbnail_model.set_paths([])
            return

        # Populate database immediately so filtered queries return results
        # before async thumbnail rendering completes (fixes zero-results bug)
        if hasattr(self, "_db"):
            stubs = [(str(fi.path), int(fi.mtime), fi.size) for fi in file_infos]
            try:
                self._db.bulk_upsert_stubs(stubs)
            except sqlite3.Error:
                logger.warning("Failed to populate DB stubs for %s", folder_path, exc_info=True)

        # Update thumbnail model — use query service if any filters are active,
        # otherwise load all paths directly.
        has_filters = (
            (hasattr(self, "_search_edit") and self._search_edit.text())
            or (hasattr(self, "color_filter_bar") and self.color_filter_bar.get_active_colors())
            or (hasattr(self, "tag_filter_bar") and self.tag_filter_bar.has_active_filters())
        )
        if has_filters and hasattr(self, "_query_service"):
            self._run_filtered_query()
        else:
            paths = [fi.path for fi in file_infos]
            self.thumbnail_model.set_paths(paths)
            # Update info bar for unfiltered folder view
            self._update_gallery_info_bar()

        # Dispatch thumbnail renders
        if hasattr(self, "_thumbnail_service"):
            statuses: dict[str, int] = {"cached": 0, "queued": 0, "derived": 0}
            for fi in file_infos:
                status = self._thumbnail_service.check_and_render(fi)
                statuses[status] = statuses.get(status, 0) + 1
            logger.info(
                "Processed %d images in %s: %d queued for render, %d already cached, %d derived from existing",
                len(file_infos),
                folder_path,
                statuses["queued"],
                statuses["cached"],
                statuses["derived"],
            )

        # Update sidebar with current folder
        if hasattr(self, "sidebar_widget"):
            self.sidebar_widget.set_current_folder(str(folder_path))

        # Refresh folder dropdown in the filter bar (new folders may have been scanned)
        if hasattr(self, "filter_bar"):
            self.filter_bar.refresh_folders()

    def _on_favorite_clicked(self, folder_path: str) -> None:
        """Handle favorite folder selection."""
        self._on_folder_navigated(folder_path)
