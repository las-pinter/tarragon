"""Main application window with dock panels (Library, Gallery, Preview, Log)."""

from __future__ import annotations

import logging
from pathlib import Path

from PySide6.QtCore import QByteArray, Qt, QTimer
from PySide6.QtGui import QAction, QCloseEvent, QIcon
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

from tarragon.db.database import Database
from tarragon.gallery_controller import GalleryController
from tarragon.log_config import LogFormatter
from tarragon.models.filter_state import FilterState
from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.scanner import FileInfo
from tarragon.services.query_service import QueryService
from tarragon.services.settings_service import SettingsService
from tarragon.services.tag_service import TagService
from tarragon.services.thumbnail_service import ThumbnailService
from tarragon.theme.constants import SIDEBAR_WIDTH_PX
from tarragon.widgets.busy_indicator import BusyIndicator
from tarragon.widgets.filter_bar import FilterBar
from tarragon.widgets.filter_bar_color import FilterBarColor
from tarragon.widgets.filter_bar_folder import FilterBarFolder
from tarragon.widgets.filter_bar_tag import FilterBarTag
from tarragon.widgets.gallery_info_bar import GalleryInfoBar
from tarragon.widgets.gallery_tabs import GalleryTabs
from tarragon.widgets.log_panel import LogPanel, QtLogHandler, apply_debug_level
from tarragon.widgets.preview_panel import PreviewPanel
from tarragon.widgets.sidebar import SidebarWidget
from tarragon.widgets.thumbnail_grid import ThumbnailGrid

logger = logging.getLogger(__name__)


class MainWindow(QMainWindow):
    """Main application window with four dockable panels.

    Docks:
        - sidebar_dock : "Library"  — left panel for library/navigation
        - grid_dock    : "Gallery"  — central panel for thumbnail gallery
        - preview_dock : "Preview"  — right panel for image preview + tag management
        - log_dock     : "Log"      — bottom panel for application log output

    Menu actions:
        - File → Open Folder — opens a folder dialog, then navigates and scans the chosen folder
        - View → Toggle visibility of each dock panel

    Gallery filter orchestration and selection handling are delegated to
    :class:`~tarragon.gallery_controller.GalleryController`.
    """

    DEFAULT_WIDTH = 1200
    DEFAULT_HEIGHT = 800

    def __init__(self, settings_service: SettingsService) -> None:
        """Initialize the main window.

        Args:
            settings: Optional Settings instance for configuration access.
                       Stored as an attribute; not used until later milestones.
        """
        super().__init__()
        self._settings_service = settings_service
        self.setWindowTitle("Tarragon")
        self.resize(self.DEFAULT_WIDTH, self.DEFAULT_HEIGHT)

        # ── Attributes created later in setup_widgets() ─────────────────
        # Initialized to None so methods can safely check `is not None`
        # even if called before setup_widgets() has run.
        self._db: Database | None = None
        self._tag_service: TagService | None = None
        self._query_service: QueryService | None = None
        self._filter_state: FilterState | None = None
        self._thumbnail_service: ThumbnailService | None = None
        self._search_edit: QLineEdit | None = None
        self._current_folder_value: str = ""
        self._gallery_controller: GalleryController | None = None
        self.filter_bar: FilterBar | None = None
        self.filter_bar_color: FilterBarColor | None = None
        self.filter_bar_tag: FilterBarTag | None = None
        self.filter_bar_folder: FilterBarFolder | None = None
        self._gallery_tabs: GalleryTabs | None = None
        self._gallery_info_bar: GalleryInfoBar | None = None
        self.thumbnail_model: ThumbnailModel | None = None
        self.thumbnail_grid: ThumbnailGrid | None = None
        self.sidebar_widget: SidebarWidget | None = None
        self.preview_panel: PreviewPanel | None = None
        self.log_panel: LogPanel | None = None
        self._log_handler: QtLogHandler | None = None
        self._search_timer: QTimer | None = None
        self._busy_indicator: BusyIndicator | None = None
        self._busy_flags: dict[str, bool] = {"scan": False, "purge": False, "decode": False}
        self._active_scan_token: int | None = None

        # ── Dock panels (created before actions so they're valid widgets) ──
        self.sidebar_dock: QDockWidget
        self.grid_dock: QDockWidget
        self.preview_dock: QDockWidget
        self.log_dock: QDockWidget
        self._create_docks()

        # Restore the user's last dock arrangement, if one was saved.
        # Tracked so setup_widgets() knows whether to apply the
        # first-run default (Log panel hidden) or leave the restored
        # visibility alone.
        self._layout_restored: bool = self._restore_layout_state()

        # ── Menu bar and actions ────────────────────────────────────────
        self._setup_actions()

        # ── Apply theme (QSS stylesheet) ────────────────────────────────
        # Cascades to all child widgets (including those created later in setup_widgets()).
        # Widget-level setStyleSheet() calls will override for specific widgets.
        self._apply_theme()

    # ── Backward-Compatible Properties ──────────────────────────────────

    @property
    def _current_folder(self) -> str:  # noqa: N802
        """Current folder path — delegates to GalleryController when available."""
        if self._gallery_controller is not None:
            return self._gallery_controller.current_folder
        return self._current_folder_value

    @_current_folder.setter
    def _current_folder(self, value: str) -> None:  # noqa: N802
        self._current_folder_value = value
        if self._gallery_controller is not None:
            self._gallery_controller.current_folder = value

    # ── Dock Widget Creation ───────────────────────────────────────────

    def _create_docks(self) -> None:
        """Create and arrange dock widgets.

        Layout::

            +----------+-------------------+----------+
            | Library  |                   | Preview  |
            |          |     Gallery       |          |
            |          |                   |          |
            +----------+-------------------+----------+
            |                Log                      |
            +------------------------------------------+

        All four panels — including Gallery — are true QDockWidgets
        participating in the dock area, so the user has full freedom to
        drag, resize, float, or tab them together. QMainWindow always
        requires *some* central widget, so we give it an invisible,
        zero-size placeholder and let dock splits cover the whole window.

        The Log dock is added to the bottom area but hidden by default
        on first run (see setup_widgets(); a previously-saved layout
        overrides this).
        """
        # Enable dock nesting - required for split layouts
        self.setDockNestingEnabled(True)

        # QMainWindow needs a central widget to exist, but we don't want it
        # to occupy real estate or behave specially — Gallery should be a
        # normal dock like everything else, not locked into the fixed
        # central slot.
        placeholder = QWidget()
        placeholder.setMaximumSize(0, 0)
        self.setCentralWidget(placeholder)

        # Create docks — all areas allowed on all four, for full rearrangement
        # freedom. objectName is required (not just the title) for
        # QMainWindow.saveState()/restoreState() to reliably identify each
        # dock across sessions.
        self.sidebar_dock = QDockWidget("Library", self)
        self.sidebar_dock.setObjectName("sidebar_dock")
        self.sidebar_dock.setAllowedAreas(Qt.DockWidgetArea.AllDockWidgetAreas)

        self.grid_dock = QDockWidget("Gallery", self)
        self.grid_dock.setObjectName("grid_dock")
        self.grid_dock.setAllowedAreas(Qt.DockWidgetArea.AllDockWidgetAreas)

        self.preview_dock = QDockWidget("Preview", self)
        self.preview_dock.setObjectName("preview_dock")
        self.preview_dock.setAllowedAreas(Qt.DockWidgetArea.AllDockWidgetAreas)

        self.log_dock = QDockWidget("Log", self)
        self.log_dock.setObjectName("log_dock")
        self.log_dock.setAllowedAreas(Qt.DockWidgetArea.AllDockWidgetAreas)

        # Seed the initial arrangement. Since the central widget has no
        # size, these dock areas span the full window.
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.sidebar_dock)
        self.splitDockWidget(self.sidebar_dock, self.grid_dock, Qt.Orientation.Horizontal)
        self.splitDockWidget(self.grid_dock, self.preview_dock, Qt.Orientation.Horizontal)
        self.splitDockWidget(self.grid_dock, self.log_dock, Qt.Orientation.Vertical)

        # Set initial dock sizes — preview panel should be roughly half the
        # remaining width after the sidebar, matching the gallery area.
        sidebar_width = SIDEBAR_WIDTH_PX
        preview_width = (self.width() - sidebar_width) // 2  # 490 at 1200px
        self.resizeDocks(
            [self.sidebar_dock, self.preview_dock],
            [sidebar_width, preview_width],
            Qt.Orientation.Horizontal,
        )

        log_height = self.height() // 4  # 25% of window height
        self.resizeDocks([self.log_dock], [log_height], Qt.Orientation.Vertical)

    # ── Layout Persistence ───────────────────────────────────────────────

    def _restore_layout_state(self) -> bool:
        """Restore a previously-saved window geometry and dock arrangement.

        Geometry (size, position, maximized/"windowed fullscreen" state)
        and dock layout are two separate QByteArrays in Qt, restored
        independently here. Geometry is restored on a best-effort basis;
        only dock-layout success is reported back, since that's what
        setup_widgets() needs to decide the Log panel's default visibility.

        Returns:
            True if a saved dock layout was found and applied, False
            otherwise (first run, corrupt data, or no settings service
            available).
        """
        encoded_geometry = self._settings_service.window_geometry_state.get()
        if encoded_geometry:
            try:
                geometry_raw = QByteArray.fromBase64(encoded_geometry.encode("ascii"))
            except (ValueError, UnicodeEncodeError):
                logger.warning("Corrupt window_geometry_state value; ignoring", exc_info=True)
            else:
                if not self.restoreGeometry(geometry_raw):
                    logger.warning("Failed to restore window geometry (incompatible or corrupt state)")

        encoded_layout = self._settings_service.window_layout_state.get()
        if not encoded_layout:
            return False
        try:
            layout_raw = QByteArray.fromBase64(encoded_layout.encode("ascii"))
        except (ValueError, UnicodeEncodeError):
            logger.warning("Corrupt window_layout_state value; ignoring", exc_info=True)
            return False
        if not self.restoreState(layout_raw):
            logger.warning("Failed to restore dock layout (incompatible or corrupt state)")
            return False
        return True

    def _save_layout_state(self) -> None:
        """Persist the current window geometry and dock arrangement."""
        geometry_raw: QByteArray = self.saveGeometry()
        encoded_geometry = bytes(geometry_raw.toBase64().data()).decode("ascii")
        self._settings_service.window_geometry_state.set(encoded_geometry)

        layout_raw: QByteArray = self.saveState()
        encoded_layout = bytes(layout_raw.toBase64().data()).decode("ascii")
        self._settings_service.window_layout_state.set(encoded_layout)

    def closeEvent(self, event: QCloseEvent) -> None:  # noqa: N802
        """Save the dock layout and drain background workers before closing.

        Subclasses (e.g. the application MainWindow in main.py) that
        override closeEvent for their own teardown should call
        super().closeEvent(event) so this still runs.
        """
        # Persist layout state first so it survives even if a drain below
        # blocks. Then drain background workers so a late worker emit can
        # never hit a deleted signal source at teardown ("Signal source has
        # been deleted" noise). Both shutdowns are idempotent, so the app
        # subclass in main.py calling them again is harmless.
        self._save_layout_state()
        if self._thumbnail_service is not None:
            self._thumbnail_service.shutdown()
        if self._gallery_controller is not None:
            self._gallery_controller.shutdown()
        super().closeEvent(event)

    # ── Widget Setup ─────────────────────────────────────────────────

    def setup_widgets(self, db: Database, tag_service: TagService) -> None:
        """Create and wire the main content widgets into dock panels.

        Args:
            db: Database instance for sidebar favorites.
            tag_service: TagService instance for tag management in preview panel.
        """
        # Store db for editor launch and preview cache lookups
        self._db = db
        self._tag_service = tag_service

        # Query service for filtered gallery queries
        self._query_service = QueryService(db)
        self._filter_state = FilterState()
        self._current_folder_value = ""

        # Sidebar
        self.sidebar_widget = SidebarWidget(db, parent=self)
        self.sidebar_dock.setWidget(self.sidebar_widget)

        # Connect sidebar signals
        self.sidebar_widget.folder_navigated.connect(self._on_folder_navigated)
        self.sidebar_widget.favorite_clicked.connect(self._on_favorite_clicked)

        # Preview panel (with tag management)
        self.preview_panel = PreviewPanel(settings_service=self._settings_service, tag_service=tag_service, parent=self)
        self.preview_dock.setWidget(self.preview_panel)

        # Thumbnail model and grid
        self.thumbnail_model = ThumbnailModel(parent=self)
        self.thumbnail_grid = ThumbnailGrid(parent=self)
        self.thumbnail_grid.set_model(self.thumbnail_model)

        # Create thumbnail service (skip if settings_service is None, e.g. in tests)
        self._thumbnail_service = ThumbnailService(db, self._settings_service, self._tag_service, parent=self)
        self._thumbnail_service.thumbnail_ready.connect(self._on_thumbnail_ready)
        self._thumbnail_service.error_occurred.connect(self._on_thumbnail_error)
        # Auto-color tags from thumbnail rendering should refresh the tag panel
        self._thumbnail_service.tags_updated.connect(tag_service.tags_changed.emit)
        self._thumbnail_service.scan_started.connect(self._on_scan_started)
        self._thumbnail_service.scan_completed.connect(self._on_folder_scanned)
        self._thumbnail_service.scan_finished.connect(self._on_scan_finished)
        self._thumbnail_service.purge_started.connect(self._on_purge_started)
        self._thumbnail_service.cache_purged.connect(self._on_cache_purged)

        # ── Search box (Deviation 4.5) ─────────────────────────────────
        self._search_edit = QLineEdit()
        self._search_edit.setObjectName("searchEdit")
        self._search_edit.setPlaceholderText("Search files and tags")
        self._search_edit.setClearButtonEnabled(True)

        # Search icon on the left side (Tabler-style magnifying glass)
        _search_icon_path = str(Path(__file__).parent / "theme" / "icons" / "search.svg")
        search_action = QAction(self._search_edit)
        search_action.setIcon(QIcon(_search_icon_path))
        self._search_edit.addAction(search_action, QLineEdit.ActionPosition.LeadingPosition)

        # ── Combined filter bar (colour + tag + folder filters) ────
        self.filter_bar = FilterBar(tag_service, db, parent=self)
        # Backward-compatible references for existing code and tests
        self.filter_bar_color = self.filter_bar.filter_bar_color
        self.filter_bar_tag = self.filter_bar.filter_bar_tag
        self.filter_bar_folder = self.filter_bar.filter_bar_folder

        # ── Gallery tabs (Folder / All Images) ─────────────────────────
        self._gallery_tabs = GalleryTabs(parent=self)

        # ── Gallery info bar (folder name + active filter pill) ────────
        self._gallery_info_bar = GalleryInfoBar(parent=self)

        # Gallery container: tabs + search + info bar + filter bar + grid stacked vertically
        gallery_container = QWidget()
        gallery_layout = QVBoxLayout(gallery_container)
        gallery_layout.setContentsMargins(0, 0, 0, 0)
        gallery_layout.addWidget(self._gallery_tabs)  # FIRST!
        gallery_layout.addWidget(self._search_edit)
        gallery_layout.addWidget(self._gallery_info_bar)

        # ── Busy indicator strip (folder scan / purge / decode status) ──
        self._busy_indicator = BusyIndicator(parent=gallery_container)
        gallery_layout.addWidget(self._busy_indicator)

        gallery_layout.addWidget(self.filter_bar)
        gallery_layout.addWidget(self.thumbnail_grid, stretch=1)
        self.grid_dock.setWidget(gallery_container)

        # Log panel — application log output in dedicated dock
        self.log_panel = LogPanel(settings_service=self._settings_service, parent=self)
        self.log_dock.setWidget(self.log_panel)
        # Hidden by default on first run only — a restored layout already
        # encodes whatever visibility the user last left it in.
        if not self._layout_restored:
            self.log_dock.hide()

        # Set up logging handler to route Python logs into the log panel
        self._log_handler = QtLogHandler(self.log_panel)
        self._log_handler.setFormatter(LogFormatter())
        root_logger = logging.getLogger()
        root_logger.addHandler(self._log_handler)
        apply_debug_level(self._settings_service.debug_mode.get())

        # Debounce timer for filename search (Deviation 4.5)
        self._search_timer = QTimer()
        self._search_timer.setSingleShot(True)
        self._search_timer.setInterval(300)

        # ── Gallery Controller — filter orchestration & selection ───
        # Compute multi-select cap from settings
        multi_preview_cap = self._settings_service.max_multi_preview.get()

        self._gallery_controller = GalleryController(
            query_service=self._query_service,
            filter_state=self._filter_state,
            thumbnail_model=self.thumbnail_model,
            gallery_tabs=self._gallery_tabs,
            gallery_info_bar=self._gallery_info_bar,
            filter_bar=self.filter_bar,
            search_edit=self._search_edit,
            search_timer=self._search_timer,
            preview_panel=self.preview_panel,
            tag_service=tag_service,
            db=db,
            thumbnail_service=self._thumbnail_service,
            max_multi_preview=multi_preview_cap,
        )

        # Wire selection signal to the controller
        self.thumbnail_grid.selection_changed.connect(self._gallery_controller.on_selection_changed)

        # Multi-select decode busy state drives the busy indicator
        self._gallery_controller.decode_relay.decode_busy_started.connect(self._on_decode_busy_started)
        self._gallery_controller.decode_relay.decode_busy_finished.connect(self._on_decode_busy_finished)

        # Wire double-click signal for editor launch
        self.thumbnail_grid.file_double_clicked.connect(self._on_file_double_clicked)

        # Wire regenerate signal for manual thumbnail regeneration
        self.thumbnail_grid.regenerate_requested.connect(self._on_regenerate_requested)

    # ── Thumbnail Service Callbacks ─────────────────────────────────

    def _on_thumbnail_ready(self, source_path: str, resolution_size: int | None, cache_path: str | None) -> None:
        """Handle thumbnail render complete — update model with cache path.

        Parameters
        ----------
        source_path:
            The original source file path.
        resolution_size:
            The resolution tier (see the TIER_KEYS table in
            ``tarragon.renderers.cache``; None for full resolution).
        cache_path:
            The cache file path (str or None).
        """
        if cache_path is None:
            return
        # Update model with the emitted cache path
        if self.thumbnail_model is not None:
            self.thumbnail_model.set_thumbnail(source_path, Path(cache_path), resolution=resolution_size)

    def _on_thumbnail_error(self, source_path: str, error_message: str) -> None:
        """Handle thumbnail render error."""
        logger.error("Thumbnail render failed for %s: %s", source_path, error_message)

    # ── Delegation Wrappers (backward compat for tests / internal use) ─

    def _run_filtered_query(self) -> None:
        """Execute a filtered query — delegates to GalleryController."""
        if self._gallery_controller is not None:
            self._gallery_controller.run_filtered_query()

    def _update_gallery_info_bar(self) -> None:
        """Update the gallery info bar — delegates to GalleryController."""
        if self._gallery_controller is not None:
            self._gallery_controller.update_gallery_info_bar()

    # ── Action Handlers (stay in MainWindow) ────────────────────────

    def _on_file_double_clicked(self, path: str) -> None:
        """Handle double-click on a thumbnail — launch external editor."""
        from tarragon.services.editors import launch_editor

        file_path = Path(path)
        extension = file_path.suffix
        if self._db is not None:
            launch_editor(self._db, file_path, extension)

    def _on_regenerate_requested(self, file_paths: list[str]) -> None:
        """Handle 'Regenerate Thumbnail' context menu action for one or more files."""
        if self._thumbnail_service is None:
            return
        logger.info("Regenerating thumbnails for %d file(s)", len(file_paths))
        for file_path in file_paths:
            path = Path(file_path)
            logger.info("Regenerating thumbnail for %s", path.name)
            self._thumbnail_service.invalidate_and_render(path)

    def _on_cache_purge_requested(self) -> None:
        """Handle a cache purge request from the settings dialog.

        Queues the purge worker; re-navigation happens in
        :meth:`_on_cache_purged` when the worker reports completion.
        """
        if self._thumbnail_service is None:
            return
        self._thumbnail_service.purge_cache()

    # ── Menu Actions ───────────────────────────────────────────────────

    def _setup_actions(self) -> None:
        """Build the menu bar with File, View, and Settings menus.

        The Open Folder action opens a folder dialog and kicks off a full
        folder scan through the thumbnail service (_on_open_folder).
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

        view_menu.addSeparator()
        reset_action = view_menu.addAction("Reset Window Layout")
        reset_action.triggered.connect(self._reset_window_layout)

        # ── Settings menu ─────────────────────────────────────────────
        settings_menu: QMenu = menubar.addMenu("&Settings")
        preferences_action: QAction = settings_menu.addAction("&Preferences...")
        preferences_action.triggered.connect(self._show_preferences)

    def _show_preferences(self) -> None:
        """Open the preferences dialog."""
        from tarragon.app_paths import set_cache_dir
        from tarragon.widgets.settings_dialog import SettingsDialog

        dialog = SettingsDialog(self._settings_service, parent=self)
        dialog.cache_purge_requested.connect(self._on_cache_purge_requested)
        if dialog.exec():
            # Settings were saved, apply changes
            # Update debug logging level
            apply_debug_level(self._settings_service.debug_mode.get())
            # Apply cache dir change immediately
            new_cache = self._settings_service.cache_dir.get()
            set_cache_dir(Path(new_cache) if new_cache else None)

    def _apply_theme(self) -> None:
        """Generate and apply the QSS stylesheet from design tokens."""
        from tarragon.theme.qss_generator import generate_qss

        qss_content = generate_qss()
        app = QApplication.instance()
        if isinstance(app, QApplication):
            app.setStyleSheet(qss_content)
        logger.debug("Theme applied successfully")

    # ── Folder Navigation ──────────────────────────────────────────────

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
        """Start an async folder scan; the completion slot updates the UI.

        The scan worker populates DB stubs before emitting results, so
        filtered queries return immediately once the model updates.
        """
        if self._thumbnail_service is None:
            return
        logger.info("Scanning folder: %s", folder_path)
        self._current_folder = str(folder_path)
        self._thumbnail_service.request_folder_scan(folder_path)

    def _on_scan_started(self, token: int, folder_path: str) -> None:
        """Track the newest scan token so stale scan results are discarded."""
        logger.debug("Scan started - token: %d, folder: %s", token, folder_path)
        self._active_scan_token = token
        self._set_busy("scan", True)

    def _on_folder_scanned(self, token: int, file_infos: list[FileInfo]) -> None:
        """Apply scan results to the gallery (model, sidebar, filter bars).

        DB stubs were inserted by the scan worker before this signal, so
        filtered queries already see the new folder.
        """
        if token != self._active_scan_token:
            logger.debug("Stale scan completed ignored - token: %d", token)
            return

        if not file_infos:
            logger.warning("No images found in %s", self._current_folder)
            if self.thumbnail_model is not None:
                self.thumbnail_model.set_paths([])
            return

        has_filters = self._filter_state is not None and not self._filter_state.is_empty()
        if has_filters and self._query_service is not None:
            self._run_filtered_query()
        else:
            paths = [fi.path for fi in file_infos]
            if self.thumbnail_model is not None:
                self.thumbnail_model.set_paths(paths)
            self._update_gallery_info_bar()

        # Update sidebar with current folder
        if self.sidebar_widget is not None:
            self.sidebar_widget.set_current_folder(str(self._current_folder))

        # Refresh folder dropdown in the filter bar (new folders may have been scanned)
        if self.filter_bar_folder is not None:
            self.filter_bar_folder.refresh_folders()

    def _on_scan_finished(self, token: int) -> None:
        """Clear the busy flag when the active scan exits (success or abort)."""
        if token == self._active_scan_token:
            self._active_scan_token = None
            self._set_busy("scan", False)

    def _on_purge_started(self) -> None:
        """Show the busy indicator while the purge worker drains and unlinks."""
        self._set_busy("purge", True)

    def _on_cache_purged(self) -> None:
        """Re-navigate after the purge worker finishes so a fresh scan repopulates the cache."""
        self._set_busy("purge", False)
        if self._current_folder:
            self._navigate_to_folder(Path(self._current_folder))
        else:
            self._run_filtered_query()

    def _on_decode_busy_started(self) -> None:
        """Show the busy indicator while multi-select previews decode."""
        self._set_busy("decode", True)

    def _on_decode_busy_finished(self) -> None:
        """Hide the busy indicator when all multi-select previews decoded."""
        self._set_busy("decode", False)

    def _set_busy(self, key: str, busy: bool) -> None:
        """Show the busy indicator whenever any background operation is active."""
        self._busy_flags[key] = busy
        if self._busy_indicator is None:
            return
        now_busy = any(self._busy_flags.values())
        if now_busy and not self._busy_indicator.is_busy:
            self._busy_indicator.begin_op()
        elif not now_busy and self._busy_indicator.is_busy:
            self._busy_indicator.end_op()

    def _on_favorite_clicked(self, folder_path: str) -> None:
        """Handle favorite folder selection."""
        self._on_folder_navigated(folder_path)

    def _reset_window_layout(self) -> None:
        """Reset all dock widgets to their initial positions and sizes"""
        self._settings_service.window_layout_state.set(None)
        self._settings_service.window_geometry_state.set(None)

        # Re-dock everything to default positions
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.sidebar_dock)
        self.splitDockWidget(self.sidebar_dock, self.grid_dock, Qt.Orientation.Horizontal)
        self.splitDockWidget(self.grid_dock, self.preview_dock, Qt.Orientation.Horizontal)
        self.splitDockWidget(self.grid_dock, self.log_dock, Qt.Orientation.Vertical)

        self.log_dock.hide()
