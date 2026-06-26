"""Tarragon application entry point."""

import logging
import sys

from PySide6.QtWidgets import QApplication

from tarragon.app_paths import db_path, ensure_dirs
from tarragon.db import Database
from tarragon.main_window import MainWindow as _MainWindow
from tarragon.settings import Settings

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
        owned_settings: Settings | None = None
        owned_database: Database | None = None

        if settings is None:
            owned_settings = Settings(db_path())
            owned_settings.init_defaults()

        if database is None:
            owned_database = Database(db_path())
            owned_database.init_schema()

        super().__init__(settings=owned_settings or settings)

        # Store references for later milestones (M3+).
        self._database = owned_database or database

    def closeEvent(self, event: object) -> None:  # noqa: ANN001, N802
        """Gracefully shut down Settings and Database on window close."""
        if isinstance(self._settings, Settings):
            self._settings.close()
        if isinstance(self._database, Database):
            self._database.close()
        super().closeEvent(event)  # type: ignore[no-untyped-call]


def main() -> None:
    """Application entry point."""
    ensure_dirs()  # Create data directories before opening database

    app = QApplication(sys.argv)
    app.setOrganizationName("tarragon")
    app.setApplicationName("tarragon")

    window = MainWindow()
    window.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()
