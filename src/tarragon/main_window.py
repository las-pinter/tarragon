"""Main application window with dock panels (Library, Gallery, Preview, Tags, Log)."""

from __future__ import annotations

import logging
from pathlib import Path

from PySide6.QtCore import Qt, QTimer
from PySide6.QtGui import QAction
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
from tarragon.services.query_service import QueryService
from tarragon.services.tag_service import TagService
from tarragon.services.thumbnail_service import ThumbnailService
from tarragon.thumbnail import _cache_file_path
from tarragon.widgets.color_filter_bar import ColorFilterBar
from tarragon.widgets.log_panel import LogPanel, QtLogHandler
from tarragon.widgets.preview_panel import PreviewPanel
from tarragon.widgets.thumbnail_grid import ThumbnailGrid

logger = logging.getLogger(__name__)


class MainWindow(QMainWindow):
    """Main application window with five dockable panels.

    Docks:
        - sidebar_dock : "Library"  — left panel (top) for library/navigation
        - grid_dock    : "Gallery"  — central panel for thumbnail gallery
        - preview_dock : "Preview"  — right panel for image preview
        - tags_dock    : "Tags"     — left panel (bottom, below Library) for tag management
        - log_dock     : "Log"      — bottom panel for application log output

    Menu actions (current milestone):
        - File → Open Folder (wired in M3, placeholder in M2)
        - View → Toggle visibility of each dock panel
    """

    DEFAULT_WIDTH = 1200
    DEFAULT_HEIGHT = 800

    def __init__(self, settings: object | None = None) -> None:
        """Initialize the main window.

        Args:
            settings: Optional Settings instance for configuration access.
                      Stored as an attribute; not used until later milestones.
        """
        super().__init__()
        self._settings = settings
        self.setWindowTitle("Tarragon")
        self.resize(self.DEFAULT_WIDTH, self.DEFAULT_HEIGHT)

        # ── Dock panels (created before actions so they're valid widgets) ──
        self.sidebar_dock: QDockWidget
        self.grid_dock: QDockWidget
        self.preview_dock: QDockWidget
        self.tags_dock: QDockWidget
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
            +----------+                   |          |
            | Tags     |                   |          |
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

        self.tags_dock = QDockWidget("Tags", self)
        self.tags_dock.setAllowedAreas(Qt.DockWidgetArea.LeftDockWidgetArea | Qt.DockWidgetArea.RightDockWidgetArea)

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

        # 3. Split Library vertically - Tags goes below Library
        self.splitDockWidget(self.sidebar_dock, self.tags_dock, Qt.Orientation.Vertical)

        # 4. Preview on the right
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self.preview_dock)

        # 5. Log at the bottom (hidden by default)
        self.addDockWidget(Qt.DockWidgetArea.BottomDockWidgetArea, self.log_dock)

    # ── Widget Setup ─────────────────────────────────────────────────

    def setup_widgets(self, db: Database, tag_service: TagService) -> None:
        """Create and wire the main content widgets into dock panels.

        Args:
            db: Database instance for sidebar favorites.
            tag_service: TagService instance for tag panel.
        """
        from tarragon.models.thumbnail_model import ThumbnailModel
        from tarragon.widgets.sidebar import SidebarWidget
        from tarragon.widgets.tag_panel import TagPanel

        # Store db for editor launch and preview cache lookups
        self._db = db

        # Query service for filtered gallery queries
        self._query_service = QueryService(db)
        self._current_folder: str = ""

        # Sidebar
        self.sidebar_widget = SidebarWidget(db, parent=self)
        self.sidebar_dock.setWidget(self.sidebar_widget)

        # Connect sidebar signals
        self.sidebar_widget.folder_navigated.connect(self._on_folder_navigated)
        self.sidebar_widget.favorite_clicked.connect(self._on_favorite_clicked)

        # Preview panel
        self.preview_panel = PreviewPanel(parent=self)
        self.preview_dock.setWidget(self.preview_panel)

        # Thumbnail model and grid
        self.thumbnail_model = ThumbnailModel(parent=self)
        self.thumbnail_grid = ThumbnailGrid(parent=self)
        self.thumbnail_grid.set_model(self.thumbnail_model)

        # Create thumbnail service (skip if settings is None, e.g. in tests)
        if self._settings is not None:
            self._thumbnail_service = ThumbnailService(db, self._settings, parent=self)
            self._thumbnail_service.thumbnailReady.connect(self._on_thumbnail_ready)
            self._thumbnail_service.errorOccurred.connect(self._on_thumbnail_error)
            # Auto-color tags from thumbnail rendering should refresh the tag panel
            self._thumbnail_service.tagsUpdated.connect(tag_service.tagsChanged.emit)

        # ── Search box (Deviation 4.5) ─────────────────────────────────
        self._search_edit = QLineEdit()
        self._search_edit.setPlaceholderText("Search filenames…")
        self._search_edit.setClearButtonEnabled(True)
        self._search_edit.textChanged.connect(self._on_search_text_changed)

        # ── Color filter bar (Deviation 4.1) ───────────────────────────
        self.color_filter_bar = ColorFilterBar(parent=self)
        self.color_filter_bar.color_filter_changed.connect(self._on_color_filter_changed)

        # Gallery container: search + color filter + grid stacked vertically
        gallery_container = QWidget()
        gallery_layout = QVBoxLayout(gallery_container)
        gallery_layout.setContentsMargins(0, 0, 0, 0)
        gallery_layout.addWidget(self._search_edit)
        gallery_layout.addWidget(self.color_filter_bar)
        gallery_layout.addWidget(self.thumbnail_grid, stretch=1)
        self.grid_dock.setWidget(gallery_container)

        # Tag panel — added to dedicated Tags dock (Deviation 4.4)
        self.tag_panel = TagPanel(tag_service, parent=self)
        self.tags_dock.setWidget(self.tag_panel)

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
        root_logger.setLevel(logging.DEBUG if self._settings and self._settings.get("debug_mode") else logging.INFO)

        # Wire tag panel filter to re-run query
        self.tag_panel.tag_filter_changed.connect(self._on_tag_filter_changed)
        self.tag_panel.scope_changed.connect(self._on_scope_changed)

        # Debounce timer for filename search (Deviation 4.5)
        self._search_timer = QTimer()
        self._search_timer.setSingleShot(True)
        self._search_timer.setInterval(300)
        self._search_timer.timeout.connect(self._run_filtered_query)

        # Wire selection signal
        self.thumbnail_grid.selection_changed.connect(self._on_selection_changed)

        # Wire double-click signal for editor launch
        self.thumbnail_grid.file_double_clicked.connect(self._on_file_double_clicked)

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

        Updates the preview panel (single image or mosaic) and tag panel
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
                from PIL import Image

                img = self._load_preview_image(path)
                self.preview_panel.set_image(img, path)
            except Exception:
                self.preview_panel.clear()
        else:
            # Multi-select — load multiple images for mosaic
            # Read cap from settings instead of hardcoding (Deviation 4.2)
            cap = self._settings.get("max_multi_preview") if self._settings else 9
            if cap is None:
                cap = 9
            images_to_show = min(len(paths), cap)
            images = []
            for p in paths[:images_to_show]:
                try:
                    from PIL import Image

                    img = self._load_preview_image(Path(p))
                    images.append(img)
                except Exception:
                    pass
            self.preview_panel.set_multi_preview(images, len(paths), cap)

        # Update tag panel
        if hasattr(self, "tag_panel"):
            self.tag_panel.set_selection(paths)

    def _load_preview_image(self, path: Path) -> "Image.Image":
        """Load a preview image, preferring the 1024px cached preview when available.

        Checks ``db.get_thumbnail(path)`` for a ``preview_cache_path`` (1024px).
        If the cached file exists on disk it is opened directly (good quality,
        fast).  Falls back to ``full_cache_path``, then to the original file.

        Images loaded from cache are marked with ``_from_cache = True`` so that
        ``PreviewPanel.set_image()`` can skip EXIF recovery from the original
        file (the cache already has correct orientation).
        """
        from PIL import Image

        thumb_record = self._db.get_thumbnail(str(path))
        if thumb_record:
            # Try 1024px preview first (good quality, fast)
            preview_path = thumb_record.get("preview_cache_path")
            if preview_path and Path(preview_path).is_file():
                img = Image.open(preview_path)
                img._from_cache = True  # type: ignore[attr-defined]
                return img

            # Fallback: full resolution cache
            full_path = thumb_record.get("full_cache_path")
            if full_path and Path(full_path).is_file():
                img = Image.open(full_path)
                img._from_cache = True  # type: ignore[attr-defined]
                return img

        # Fallback: open the original file directly
        return Image.open(path)

    def _on_file_double_clicked(self, path: str) -> None:
        """Handle double-click on a thumbnail — launch external editor."""
        from tarragon.editors import launch_editor

        file_path = Path(path)
        extension = file_path.suffix
        launch_editor(self._db, file_path, extension)

    # ── Filtered Query Helpers ─────────────────────────────────────────

    def _on_search_text_changed(self, text: str) -> None:
        """Restart the debounce timer when the search text changes."""
        self._search_timer.start()

    def _on_color_filter_changed(self, color_tags: set[str]) -> None:
        """Re-run the filtered query when color filter swatches change."""
        self._run_filtered_query()

    def _on_tag_filter_changed(self, tag_ids: set[int]) -> None:
        """Re-run the filtered query when tag filter checkboxes change."""
        self._run_filtered_query()

    def _on_scope_changed(self, is_global: bool) -> None:  # noqa: FBT001
        """Re-run the filtered query when the Global/Local toggle changes."""
        self._run_filtered_query()

    def _run_filtered_query(self) -> None:
        """Execute a QueryService query combining all active filters.

        Combines the current folder scope, filename search text, active
        colour-bucket set, and checked tag IDs into a single query, then
        updates the ThumbnailModel with the results.

        In global scope mode (tag panel toggle), the folder constraint is
        removed so results span the entire database.

        If no folder is currently selected (``_current_folder`` is empty)
        and we are NOT in global mode, the method returns without modifying
        the model to avoid clearing the gallery.

        Race-condition guard: if the filtered query returns empty but the
        folder contains files, we fall back to showing unfiltered results
        and log a warning.
        """
        if not hasattr(self, "_query_service"):
            return

        # Determine folder scope based on global/local toggle
        is_global = hasattr(self, "tag_panel") and self.tag_panel.is_global_scope()

        # Don't clear the gallery if no folder is selected and not in global mode
        if not self._current_folder and not is_global:
            return

        folder_path = "" if is_global else self._current_folder

        filename_filter = self._search_edit.text() if hasattr(self, "_search_edit") else ""
        color_tags = self.color_filter_bar.get_active_colors() if hasattr(self, "color_filter_bar") else set()
        tag_ids: set[int] = set()
        if hasattr(self, "tag_panel"):
            # Collect fully-checked tag IDs from the tag panel
            tag_ids = {
                tid for tid, cb in self.tag_panel._tag_checkboxes.items() if cb.checkState() == Qt.CheckState.Checked
            }

        results = self._query_service.query(
            folder_path=folder_path,
            filename_filter=filename_filter,
            tag_ids=tag_ids,
            color_tags=color_tags,
        )

        # Race-condition guard: if filters are active but query returned empty,
        # and we know files exist in the folder, fall back to unfiltered results
        has_filters = bool(filename_filter or color_tags or tag_ids)
        if has_filters and not results and self._current_folder:
            thumbnails_in_folder = self._db.list_thumbnails_for_folder(self._current_folder)
            if thumbnails_in_folder:
                logger.warning(
                    "Filtered query returned 0 results but %d thumbnails exist in %s — "
                    "falling back to unfiltered results (possible race condition)",
                    len(thumbnails_in_folder),
                    self._current_folder,
                )
                results = [Path(t["path"]) for t in thumbnails_in_folder]

        self.thumbnail_model.set_paths(results)

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
        for dock in (self.sidebar_dock, self.grid_dock, self.preview_dock, self.tags_dock, self.log_dock):
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

        if self._settings is None:
            return
        dialog = SettingsDialog(self._settings, parent=self)
        if dialog.exec():
            # Settings were saved, apply changes
            # Update debug logging level
            debug_mode = self._settings.get("debug_mode")
            logging.getLogger().setLevel(logging.DEBUG if debug_mode else logging.INFO)
            # Apply cache dir change immediately
            new_cache = self._settings.get("cache_dir")
            set_cache_dir(Path(new_cache) if new_cache else None)

    def _apply_theme(self) -> None:
        """Load and apply the QSS stylesheet from theme/app.qss."""
        from tarragon.theme.loader import ThemeLoader

        loader = ThemeLoader()
        qss_content = loader.load_qss()
        QApplication.instance().setStyleSheet(qss_content)
        logger.debug("Theme applied successfully")

    def _on_open_folder(self) -> None:
        """Callback for File → Open Folder — scan folder and populate grid."""
        from tarragon.scanner import scan_folder

        folder = QFileDialog.getExistingDirectory(self, "Open Folder", "")
        if not folder:
            return

        folder_path = Path(folder)
        logger.info(f"Scanning folder: {folder_path}")

        # Store current folder for filtered queries
        self._current_folder = str(folder_path)

        # Update tag panel folder scope for local counts
        if hasattr(self, "tag_panel"):
            self.tag_panel.set_folder_path(str(folder_path))

        # Scan folder for images
        file_infos = scan_folder(folder_path)
        if not file_infos:
            logger.warning(f"No images found in {folder_path}")
            return

        # Update thumbnail model — use query service if any filters are active,
        # otherwise load all paths directly.
        has_filters = (
            (hasattr(self, "_search_edit") and self._search_edit.text())
            or (hasattr(self, "color_filter_bar") and self.color_filter_bar.get_active_colors())
            or (hasattr(self, "tag_panel") and self.tag_panel.has_active_filters())
        )
        if has_filters and hasattr(self, "_query_service"):
            self._run_filtered_query()
        else:
            paths = [fi.path for fi in file_infos]
            self.thumbnail_model.set_paths(paths)

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

    def _on_folder_navigated(self, folder_path: str) -> None:
        """Handle folder selection from sidebar tree."""
        from tarragon.scanner import scan_folder

        folder = Path(folder_path)
        if not folder.exists() or not folder.is_dir():
            return

        self._current_folder = str(folder)

        # Update tag panel folder scope for local counts
        if hasattr(self, "tag_panel"):
            self.tag_panel.set_folder_path(str(folder))

        file_infos = scan_folder(folder)

        if not file_infos:
            self.thumbnail_model.set_paths([])
            return

        # Check if any filters are active — if so, apply them instead of showing all
        has_filters = (
            (hasattr(self, "_search_edit") and self._search_edit.text())
            or (hasattr(self, "color_filter_bar") and self.color_filter_bar.get_active_colors())
            or (hasattr(self, "tag_panel") and self.tag_panel.has_active_filters())
        )
        if has_filters and hasattr(self, "_query_service"):
            self._run_filtered_query()
        else:
            # Update thumbnail model with all paths
            paths = [fi.path for fi in file_infos]
            self.thumbnail_model.set_paths(paths)

        # Dispatch thumbnail renders
        if hasattr(self, "_thumbnail_service"):
            statuses: dict[str, int] = {"cached": 0, "queued": 0, "derived": 0}
            for fi in file_infos:
                status = self._thumbnail_service.check_and_render(fi)
                statuses[status] = statuses.get(status, 0) + 1
            logger.info(
                "Processed %d images in %s: %d queued for render, %d already cached, %d derived from existing",
                len(file_infos),
                folder,
                statuses["queued"],
                statuses["cached"],
                statuses["derived"],
            )

        # Update sidebar
        if hasattr(self, "sidebar_widget"):
            self.sidebar_widget.set_current_folder(str(folder))

    def _on_favorite_clicked(self, folder_path: str) -> None:
        """Handle favorite folder selection."""
        self._on_folder_navigated(folder_path)
