"""Tests for the application entry point."""

from collections.abc import Generator
from pathlib import Path
from typing import Any

import pytest


@pytest.fixture(autouse=True)
def qapp() -> Generator[object, None, None]:
    """Provide a shared QApplication instance for all Qt tests."""
    from PySide6.QtWidgets import QApplication

    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


# ── Import Tests ───────────────────────────────────────────────────────


def test_main_module_imports_cleanly() -> None:
    """main.py can be imported without side effects or errors."""
    from tarragon import main  # noqa: F401

    assert hasattr(main, "MainWindow")
    assert hasattr(main, "main")


# ── MainWindow Class Tests ─────────────────────────────────────────────


def test_main_window_is_qmainwindow() -> None:
    """MainWindow is a subclass of QMainWindow."""
    from PySide6.QtWidgets import QMainWindow
    from tarragon.main import MainWindow

    assert issubclass(MainWindow, QMainWindow)


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


def test_main_window_has_docks(qapp: Any, tmp_path: Path) -> None:  # noqa: ARG001
    """MainWindow (from main.py) creates the three dock panels."""

    from tarragon.db import Database
    from tarragon.main import MainWindow
    from tarragon.settings import Settings

    settings_db = Database(tmp_path / "test_settings_docks.db")
    settings_db.init_schema()
    settings = Settings(settings_db)
    database = Database(tmp_path / "test_main_docks.db")
    window = MainWindow(settings=settings, database=database)
    try:
        assert hasattr(window, "sidebar_dock")
        assert hasattr(window, "grid_dock")
        assert hasattr(window, "preview_dock")
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


def test_main_window_default_size(qapp: Any, tmp_path: Path) -> None:  # noqa: ARG001
    """MainWindow (from main.py) opens at approximately 1200x800."""

    from tarragon.db import Database
    from tarragon.main import MainWindow
    from tarragon.settings import Settings

    settings_db = Database(tmp_path / "test_settings3.db")
    settings_db.init_schema()
    settings = Settings(settings_db)
    database = Database(tmp_path / "test_main_size.db")
    window = MainWindow(settings=settings, database=database)
    try:
        size = window.size()
        assert size.width() == 1200, f"Expected width 1200, got {size.width()}"
        assert size.height() == 800, f"Expected height 800, got {size.height()}"
    finally:
        window.close()


# ── Entry Point Tests ──────────────────────────────────────────────────


def test_main_function_exists() -> None:
    """main() function is callable."""
    from tarragon.main import main

    assert callable(main)
