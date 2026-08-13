"""Tests for Main"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import tarragon.main as main_module
from tarragon.db.database import Database
from tarragon.main import MainWindow, main


class TestMainModule:
    """The main module imports cleanly."""

    def test_main_module_imports_cleanly(self) -> None:
        """The main module imports without side effects or errors."""
        assert hasattr(main_module, "MainWindow")
        assert hasattr(main_module, "main")


class TestMainWindow:
    """MainWindow initializes with title and database."""

    def test_main_window_has_title(self, qapp: Any, tmp_path: Path) -> None:
        """MainWindow sets a title on initialization."""
        database = Database(tmp_path / "test_main.db")
        window = MainWindow(database=database)
        try:
            assert window.windowTitle() == "Tarragon"
        finally:
            window.close()

    def test_main_window_has_database(self, qapp: Any, tmp_path: Path) -> None:
        """MainWindow stores a Database reference."""
        database = Database(tmp_path / "test_main_dbref.db")
        window = MainWindow(database=database)
        try:
            assert hasattr(window, "_database")
            assert isinstance(window._database, Database)
        finally:
            window.close()


class TestEntryPoint:
    """The main() entry point exists."""

    def test_main_function_exists(self) -> None:
        """The main() function is callable."""
        assert callable(main)
