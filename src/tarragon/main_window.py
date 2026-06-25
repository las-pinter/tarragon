"""Main application window with three dock panels (Library, Gallery, Preview)."""

from __future__ import annotations

import logging
from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtGui import QAction
from PySide6.QtWidgets import QDockWidget, QMainWindow, QMenu

from tarragon.db import Database
from tarragon.services.tag_service import TagService
from tarragon.widgets.preview_panel import PreviewPanel
from tarragon.widgets.thumbnail_grid import ThumbnailGrid

logger = logging.getLogger(__name__)


class MainWindow(QMainWindow):
    """Main application window with three dockable panels.

    Docks:
        - sidebar_dock : "Library"  — left panel for library/navigation
        - grid_dock    : "Gallery"  — central panel for thumbnail gallery
        - preview_dock : "Preview"  — right/bottom panel for image preview

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

    # ── Dock Widget Creation ───────────────────────────────────────────

    def _create_docks(self) -> None:
        """Create the three dock panels and attach them to this window.

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

        # Add docks in default positions (Qt6: no center area — use Top for gallery).
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.sidebar_dock)
        self.addDockWidget(Qt.DockWidgetArea.TopDockWidgetArea, self.grid_dock)
        self.addDockWidget(Qt.DockWidgetArea.BottomDockWidgetArea, self.preview_dock)

    # ── Widget Setup ─────────────────────────────────────────────────

    def setup_widgets(self, db: Database, tag_service: TagService) -> None:
        """Create and wire the main content widgets into dock panels.

        Args:
            db: Database instance for sidebar favorites.
            tag_service: TagService instance for tag panel.
        """
        from tarragon.widgets.sidebar import SidebarWidget
        from tarragon.widgets.tag_panel import TagPanel

        # Sidebar
        self.sidebar_widget = SidebarWidget(db, parent=self)
        self.sidebar_dock.setWidget(self.sidebar_widget)

        # Preview panel
        self.preview_panel = PreviewPanel(parent=self)
        self.preview_dock.setWidget(self.preview_panel)

        # Thumbnail grid
        self.thumbnail_grid = ThumbnailGrid(parent=self)
        self.grid_dock.setWidget(self.thumbnail_grid)

        # Tag panel (stored for selection updates)
        self.tag_panel = TagPanel(tag_service, parent=self)

        # Wire selection signal
        self.thumbnail_grid.selection_changed.connect(self._on_selection_changed)

    def _on_selection_changed(self, paths: list[str]) -> None:
        """Handle thumbnail grid selection changes.

        Updates the preview panel (single image or mosaic) and tag panel
        based on the current selection.
        """
        if len(paths) == 0:
            self.preview_panel.clear()
        elif len(paths) == 1:
            # Single selection — load image and show
            path = Path(paths[0])
            try:
                from PIL import Image

                img = Image.open(path)
                self.preview_panel.set_image(img, path)
            except Exception:
                self.preview_panel.clear()
        else:
            # Multi-select — load multiple images for mosaic
            cap = 9
            images_to_show = min(len(paths), cap)
            images = []
            for p in paths[:images_to_show]:
                try:
                    from PIL import Image

                    img = Image.open(p)
                    images.append(img)
                except Exception:
                    pass
            self.preview_panel.set_multi_preview(images, len(paths), cap)

        # Update tag panel
        if hasattr(self, "tag_panel"):
            self.tag_panel.set_selection(paths)

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
        # TODO(M3): Connect this action to the folder-scanning logic.
        # open_folder_action.triggered.connect(self._on_open_folder)

    def _apply_theme(self) -> None:
        """Load and apply the QSS stylesheet from theme/app.qss."""
        from tarragon.theme.loader import ThemeLoader

        loader = ThemeLoader()
        qss_content = loader.load_qss()
        self.setStyleSheet(qss_content)
        logger.debug("Theme applied successfully")

    def _on_open_folder(self) -> None:
        """Callback for File → Open Folder (placeholder — wired in M3)."""
        logger.info("Open Folder action triggered")
        # TODO(M3): Implement folder selection and scan logic.
