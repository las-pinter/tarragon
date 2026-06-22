"""Main application window with three dock panels (Library, Gallery, Preview)."""

from __future__ import annotations

import logging

from PySide6.QtCore import Qt
from PySide6.QtGui import QAction
from PySide6.QtWidgets import QDockWidget, QMainWindow, QMenu

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
