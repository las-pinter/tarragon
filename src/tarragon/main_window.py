"""Main application window with three dock panels (Library, Gallery, Preview)."""

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
from tarragon.widgets.color_filter_bar import ColorFilterBar
from tarragon.widgets.preview_panel import PreviewPanel
from tarragon.widgets.thumbnail_grid import ThumbnailGrid

logger = logging.getLogger(__name__)


class MainWindow(QMainWindow):
    """Main application window with four dockable panels.

    Docks:
        - sidebar_dock : "Library"  — left panel for library/navigation
        - grid_dock    : "Gallery"  — central panel for thumbnail gallery
        - preview_dock : "Preview"  — bottom panel for image preview
        - tags_dock    : "Tags"     — right panel for tag management

    Menu actions (current milestone):
        - File → Open Folder (wired in M3, placeholder in M2)
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
        self._create_docks()

        # ── Menu bar and actions ────────────────────────────────────────
        self._setup_actions()

        # ── Apply theme (QSS stylesheet) ────────────────────────────────
        # Cascades to all child widgets (including those created later in setup_widgets()).
        # Widget-level setStyleSheet() calls will override for specific widgets.
        self._apply_theme()

    # ── Dock Widget Creation ───────────────────────────────────────────

    def _create_docks(self) -> None:
        """Create the dock panels and attach them to this window.

        Docks:
            - sidebar_dock : "Library"  — left panel for library/navigation
            - grid_dock    : "Gallery"  — central panel for thumbnail gallery
            - preview_dock : "Preview"  — bottom panel for image preview
            - tags_dock    : "Tags"     — right panel for tag management

        Each dock is given a meaningful title and allowed to float/move
        freely using Qt's built-in docking behavior (no custom drag logic needed).
        """
        self.sidebar_dock = QDockWidget("Library", self)
        self.sidebar_dock.setAllowedAreas(Qt.DockWidgetArea.LeftDockWidgetArea | Qt.DockWidgetArea.RightDockWidgetArea)

        self.grid_dock = QDockWidget("Gallery", self)
        # Qt6 has no CenterDockWidgetArea — docks can only go at edges.
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

        # Add docks in default positions (Qt6: no center area — use Top for gallery).
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.sidebar_dock)
        self.addDockWidget(Qt.DockWidgetArea.TopDockWidgetArea, self.grid_dock)
        self.addDockWidget(Qt.DockWidgetArea.BottomDockWidgetArea, self.preview_dock)
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self.tags_dock)

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

        # Preview panel
        self.preview_panel = PreviewPanel(parent=self)
        self.preview_dock.setWidget(self.preview_panel)

        # Thumbnail model and grid
        self.thumbnail_model = ThumbnailModel(parent=self)
        self.thumbnail_grid = ThumbnailGrid(parent=self)
        self.thumbnail_grid.set_model(self.thumbnail_model)

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

        # Wire tag panel filter to re-run query
        self.tag_panel.tag_filter_changed.connect(self._on_tag_filter_changed)

        # Debounce timer for filename search (Deviation 4.5)
        self._search_timer = QTimer()
        self._search_timer.setSingleShot(True)
        self._search_timer.setInterval(300)
        self._search_timer.timeout.connect(self._run_filtered_query)

        # Wire selection signal
        self.thumbnail_grid.selection_changed.connect(self._on_selection_changed)

        # Wire double-click signal for editor launch
        self.thumbnail_grid.file_double_clicked.connect(self._on_file_double_clicked)

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
        """Load a preview image, preferring the cached master when available.

        Checks ``db.get_thumbnail(path)`` for a ``master_cache_path``.  If the
        cached file exists on disk it is opened directly (avoiding expensive
        re-compositing for PSDs).  Falls back to the original file otherwise.
        """
        from PIL import Image

        thumb_record = self._db.get_thumbnail(str(path))
        if thumb_record:
            cache_path = thumb_record.get("master_cache_path")
            if cache_path and Path(cache_path).is_file():
                return Image.open(cache_path)

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

    def _run_filtered_query(self) -> None:
        """Execute a QueryService query combining all active filters.

        Combines the current folder scope, filename search text, active
        colour-bucket set, and checked tag IDs into a single query, then
        updates the ThumbnailModel with the results.
        """
        if not hasattr(self, "_query_service"):
            return

        filename_filter = self._search_edit.text() if hasattr(self, "_search_edit") else ""
        color_tags = self.color_filter_bar.get_active_colors() if hasattr(self, "color_filter_bar") else set()
        tag_ids: set[int] = set()
        if hasattr(self, "tag_panel"):
            # Collect fully-checked tag IDs from the tag panel
            tag_ids = {
                tid for tid, cb in self.tag_panel._tag_checkboxes.items() if cb.checkState() == Qt.CheckState.Checked
            }

        results = self._query_service.query(
            folder_path=self._current_folder,
            filename_filter=filename_filter,
            tag_ids=tag_ids,
            color_tags=color_tags,
        )
        self.thumbnail_model.set_paths(results)

    # ── Menu Actions ───────────────────────────────────────────────────

    def _setup_actions(self) -> None:
        """Build the menu bar with File → Open Folder action.

        The Open Folder action is wired into the menu system here but does
        nothing yet (full folder-scanning implementation lands in M3).
        """
        menubar = self.menuBar()

        # ── File menu ───────────────────────────────────────────────────
        file_menu: QMenu = menubar.addMenu("&File")

        open_folder_action: QAction = file_menu.addAction("Open &Folder…")
        open_folder_action.setStatusTip("Open a folder to browse images")
        open_folder_action.triggered.connect(self._on_open_folder)

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

        # Scan folder for images
        file_infos = scan_folder(folder_path)
        if not file_infos:
            logger.warning(f"No images found in {folder_path}")
            return

        # Update thumbnail model — use query service if any filters are active,
        # otherwise load all paths directly.
        has_filters = (hasattr(self, "_search_edit") and self._search_edit.text()) or (
            hasattr(self, "color_filter_bar") and self.color_filter_bar.get_active_colors()
        )
        if has_filters and hasattr(self, "_query_service"):
            self._run_filtered_query()
        else:
            paths = [fi.path for fi in file_infos]
            self.thumbnail_model.set_paths(paths)

        # Update sidebar with current folder
        if hasattr(self, "sidebar_widget"):
            self.sidebar_widget.set_current_folder(str(folder_path))

        logger.info(f"Found {len(file_infos)} images in {folder_path}")
