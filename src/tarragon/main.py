"""Tarragon application entry point."""

import logging
import sys

from PySide6.QtGui import QColor, QPalette
from PySide6.QtWidgets import QApplication

from tarragon.app_paths import db_path, ensure_dirs
from tarragon.db import Database
from tarragon.main_window import MainWindow as _MainWindow
from tarragon.services.tag_service import TagService
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
        owned_database: Database | None = None

        if database is None:
            owned_database = Database(db_path())

        # Resolve to owned or caller-provided instance.
        self._database = owned_database or database

        # Ensure schema exists — idempotent; safe for caller-provided databases too.
        self._database.init_schema()

        owned_settings: Settings | None = None
        if settings is None:
            owned_settings = Settings(self._database)
            owned_settings.init_defaults()

        super().__init__(settings=owned_settings or settings)

        # Wire up content widgets (thumbnail grid, sidebar, preview, tags).
        tag_service = TagService(self._database)
        self.setup_widgets(self._database, tag_service)

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
    palette.setColor(QPalette.ColorRole.Window, QColor("#16151A"))
    palette.setColor(QPalette.ColorRole.WindowText, QColor("#ece9f2"))
    palette.setColor(QPalette.ColorRole.Base, QColor("#1c1b22"))
    palette.setColor(QPalette.ColorRole.AlternateBase, QColor("#211f29"))
    palette.setColor(QPalette.ColorRole.ToolTipBase, QColor("#211f29"))
    palette.setColor(QPalette.ColorRole.ToolTipText, QColor("#ece9f2"))
    palette.setColor(QPalette.ColorRole.Text, QColor("#ece9f2"))
    palette.setColor(QPalette.ColorRole.Button, QColor("#211f29"))
    palette.setColor(QPalette.ColorRole.ButtonText, QColor("#ece9f2"))
    palette.setColor(QPalette.ColorRole.BrightText, QColor("#F0997B"))  # coral_strong
    palette.setColor(QPalette.ColorRole.Highlight, QColor("#D85A30"))  # coral_muted
    palette.setColor(QPalette.ColorRole.HighlightedText, QColor("#ece9f2"))
    palette.setColor(QPalette.ColorRole.Link, QColor("#FAC775"))  # amber_accent

    # Disabled color group — for disabled widgets
    disabled_text = QColor("#74707B")  # text_tertiary
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.WindowText, disabled_text)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.Text, disabled_text)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.ButtonText, disabled_text)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.HighlightedText, disabled_text)
    palette.setColor(
        QPalette.ColorGroup.Disabled, QPalette.ColorRole.Highlight, QColor("#4A3A35")
    )  # muted disabled selection

    # Additional roles for completeness
    # Derived: tuned dark separator between bg_primary (#16151A) and bg_secondary (#1c1b22).
    # Update if either token changes.
    palette.setColor(QPalette.ColorRole.Mid, QColor("#1E1D23"))
    palette.setColor(QPalette.ColorRole.LinkVisited, QColor("#A09CA3"))  # text_secondary
    palette.setColor(QPalette.ColorRole.PlaceholderText, QColor("#74707B"))  # text_tertiary
    app.setPalette(palette)

    window = MainWindow()
    window.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()
