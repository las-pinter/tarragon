"""Tests for the application entry point."""

from pathlib import Path
from typing import Any

# ── Import Tests ───────────────────────────────────────────────────────


def test_main_module_imports_cleanly() -> None:
    """main.py can be imported without side effects or errors."""
    from tarragon import main  # noqa: F401

    assert hasattr(main, "MainWindow")
    assert hasattr(main, "main")


# ── MainWindow Class Tests ─────────────────────────────────────────────


def test_main_window_has_title(qapp: Any, tmp_path: Path) -> None:  # noqa: ARG001
    """MainWindow sets a title on initialization (with services at temp paths)."""

    from tarragon.db import Database
    from tarragon.main import MainWindow
    from tarragon.settings import Settings

    settings_db = Database(tmp_path / "test_settings.db")
    settings_db.init_schema()
    settings = Settings(settings_db)
    database = Database(tmp_path / "test_main.db")
    window = MainWindow(settings=settings, database=database)
    try:
        assert window.windowTitle() == "Tarragon"
    finally:
        window.close()


def test_main_window_has_database(qapp: Any, tmp_path: Path) -> None:  # noqa: ARG001
    """MainWindow (from main.py) stores a Database reference."""

    from tarragon.db import Database
    from tarragon.main import MainWindow
    from tarragon.settings import Settings

    settings_db = Database(tmp_path / "test_settings2.db")
    settings_db.init_schema()
    settings = Settings(settings_db)
    database = Database(tmp_path / "test_main_dbref.db")
    window = MainWindow(settings=settings, database=database)
    try:
        assert hasattr(window, "_database")
        assert isinstance(window._database, Database)
    finally:
        window.close()


# ── Entry Point Tests ──────────────────────────────────────────────────


def test_main_function_exists() -> None:
    """main() function is callable."""
    from tarragon.main import main

    assert callable(main)
