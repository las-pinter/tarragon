"""Pytest configuration — ensures headless operation for Qt tests."""

import os
from typing import Any, Generator

import pytest

# Must be set BEFORE any Qt imports or QApplication creation
os.environ["QT_QPA_PLATFORM"] = "offscreen"


@pytest.fixture(scope="session", autouse=True)
def qapp() -> Generator[Any, None, None]:
    """Shared QApplication for entire test session."""
    from PySide6.QtWidgets import QApplication

    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app
