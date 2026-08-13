"""Tests for Main"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from tarragon.db.database import Database
from tarragon.main import MainWindow


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
