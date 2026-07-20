"""Tarragon application entry point."""

import logging
import os
import sys

from PySide6.QtGui import QCloseEvent
from PySide6.QtWidgets import QApplication

from tarragon.app_paths import db_path, ensure_dirs
from tarragon.db import Database
from tarragon.main_window import MainWindow as _MainWindow
from tarragon.migrations import MigrationRunner
from tarragon.services.settings_service import SettingsService
from tarragon.services.tag_service import TagService
from tarragon.settings import Settings
from tarragon.theme.colors import create_palette

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
        # Lazy construction, only create what the caller didn't provide.
        owned_database: Database | None = None

        if database is None:
            owned_database = Database(db_path())

        # Resolve to owned or caller-provided instance.
        self._database = owned_database or database
        assert self._database is not None  # At least one branch always provides a Database.

        # Ensure schema exists and version tracking is bootstrapped.
        MigrationRunner(self._database).run()

        # Clean up folder_cache_uuids entries whose source folders no longer exist.
        stale_count = self._database.cleanup_stale_folder_uuids()
        if stale_count:
            logger.info("Cleaned up %d stale folder cache UUID entries", stale_count)

        owned_settings: Settings | None = None
        if settings is None:
            owned_settings = Settings(self._database)
            owned_settings.init_defaults()

        resolved_settings = owned_settings or settings

        # Configure custom cache directory if set
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
        # Shut down thumbnail service first. Cancels pending work and
        # forcefully terminates any stuck worker processes.
        if self._thumbnail_service is not None:
            self._thumbnail_service.shutdown()
        # super().closeEvent() saves the dock layout via the settings
        # service, which writes through to the database. It must run
        # before the database connection below is closed.
        super().closeEvent(event)
        if isinstance(self._settings, Settings):
            self._settings.close()
        if isinstance(self._database, Database):
            self._database.close()
        # Force immediate exit. Bypasses atexit handlers that may hang
        # on ProcessPoolExecutor worker processes stuck in system calls.
        # Skip this during testing to allow pytest to clean up properly.
        if "pytest" not in sys.modules:
            os._exit(0)


def main() -> None:
    """Application entry point."""
    ensure_dirs()

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

    palette = create_palette()
    app.setPalette(palette)

    window = MainWindow()
    window.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()
