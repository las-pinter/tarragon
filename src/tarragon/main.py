"""Tarragon application entry point."""

import logging
import os
import sys

from PySide6.QtGui import QCloseEvent, QPalette
from PySide6.QtWidgets import QApplication

from tarragon.app_paths import db_path, ensure_dirs
from tarragon.db import Database
from tarragon.main_window import MainWindow as _MainWindow
from tarragon.services.settings_service import SettingsService
from tarragon.services.tag_service import TagService
from tarragon.settings import Settings
from tarragon.theme.colors import (
    AMBER_ACCENT,
    BG_PRIMARY,
    BG_SECONDARY,
    BG_TERTIARY,
    CORAL_MUTED,
    CORAL_STRONG,
    HIGHLIGHT_DISABLED,
    SEPARATOR,
    TEXT_PRIMARY,
    TEXT_SECONDARY,
    TEXT_TERTIARY,
)

logger = logging.getLogger(__name__)


class MainWindow(_MainWindow):
    """Application main window wired with Settings and Database.

    Extends the base ``_MainWindow`` to inject application-level services
    (Settings, Database) into the UI layer during construction.
    """

    def __init__(self, settings: Settings | None = None, database: Database | None = None) -> None:
        """Initialize MainWindow with optional Settings and Database instances.

        Args:
            settings: Application settings repository.  Auto-created if omitted.
            database: Image catalog database connection.  Auto-created if omitted.
        """
        # Lazy construction — only create what the caller didn't provide.
        owned_database: Database | None = None

        if database is None:
            owned_database = Database(db_path())

        # Resolve to owned or caller-provided instance.
        self._database = owned_database or database
        assert self._database is not None  # At least one branch always provides a Database.

        # Ensure schema exists — idempotent; safe for caller-provided databases too.
        self._database.init_schema()

        # Clean up folder_cache_uuids entries whose source folders no longer exist.
        stale_count = self._database.cleanup_stale_folder_uuids()
        if stale_count:
            logger.info("Cleaned up %d stale folder cache UUID entries", stale_count)

        owned_settings: Settings | None = None
        if settings is None:
            owned_settings = Settings(self._database)
            owned_settings.init_defaults()

        resolved_settings = owned_settings or settings

        # Configure custom cache directory if set (via SettingsService for validated access)
        if resolved_settings is not None:
            settings_service = SettingsService(resolved_settings)
            custom_cache = settings_service.get_cache_dir()
            if custom_cache:
                from pathlib import Path

                from tarragon.app_paths import set_cache_dir

                set_cache_dir(Path(custom_cache))

        super().__init__(settings=resolved_settings)

        # Wire up content widgets (thumbnail grid, sidebar, preview, tags).
        tag_service = TagService(self._database)
        self.setup_widgets(self._database, tag_service)

    def closeEvent(self, event: QCloseEvent) -> None:  # noqa: N802
        """Shut down all services and exit immediately.

        Uses os._exit(0) after cleanup to ensure immediate termination,
        bypassing atexit handlers that may hang on stuck worker processes.
        """
        # Shut down thumbnail service first — cancels pending work and
        # forcefully terminates any stuck worker processes.
        if hasattr(self, "_thumbnail_service"):
            self._thumbnail_service.shutdown()
        if isinstance(self._settings, Settings):
            self._settings.close()
        if isinstance(self._database, Database):
            self._database.close()
        super().closeEvent(event)
        # Force immediate exit — bypasses atexit handlers that may hang
        # on ProcessPoolExecutor worker processes stuck in system calls.
        # Skip this during testing to allow pytest to clean up properly.
        if "pytest" not in sys.modules:
            os._exit(0)


def main() -> None:
    """Application entry point."""
    ensure_dirs()  # Create data directories before opening database

    logging.basicConfig(
        level=logging.INFO,
        format="[%(asctime)s] %(levelname)-8s %(name)s: %(message)s",
        datefmt="%H:%M:%S",
    )

    # Suppress noisy third-party debug logging
    logging.getLogger("PIL").setLevel(logging.WARNING)

    app = QApplication(sys.argv)
    app.setOrganizationName("tarragon")
    app.setApplicationName("tarragon")
    app.setStyle("Fusion")

    # Set dark palette for Fusion style (matches design tokens)
    palette = QPalette()
    palette.setColor(QPalette.ColorRole.Window, BG_PRIMARY)
    palette.setColor(QPalette.ColorRole.WindowText, TEXT_PRIMARY)
    palette.setColor(QPalette.ColorRole.Base, BG_SECONDARY)
    palette.setColor(QPalette.ColorRole.AlternateBase, BG_TERTIARY)
    palette.setColor(QPalette.ColorRole.ToolTipBase, BG_TERTIARY)
    palette.setColor(QPalette.ColorRole.ToolTipText, TEXT_PRIMARY)
    palette.setColor(QPalette.ColorRole.Text, TEXT_PRIMARY)
    palette.setColor(QPalette.ColorRole.Button, BG_TERTIARY)
    palette.setColor(QPalette.ColorRole.ButtonText, TEXT_PRIMARY)
    palette.setColor(QPalette.ColorRole.BrightText, CORAL_STRONG)
    palette.setColor(QPalette.ColorRole.Highlight, CORAL_MUTED)
    palette.setColor(QPalette.ColorRole.HighlightedText, TEXT_PRIMARY)
    palette.setColor(QPalette.ColorRole.Link, AMBER_ACCENT)

    # Disabled color group — for disabled widgets
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.WindowText, TEXT_TERTIARY)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.Text, TEXT_TERTIARY)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.ButtonText, TEXT_TERTIARY)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.HighlightedText, TEXT_TERTIARY)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.Highlight, HIGHLIGHT_DISABLED)

    # Additional roles for completeness
    palette.setColor(QPalette.ColorRole.Mid, SEPARATOR)
    palette.setColor(QPalette.ColorRole.LinkVisited, TEXT_SECONDARY)
    palette.setColor(QPalette.ColorRole.PlaceholderText, TEXT_TERTIARY)
    app.setPalette(palette)

    window = MainWindow()
    window.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()
