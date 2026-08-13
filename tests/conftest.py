"""Pytest configuration - ensures headless operation for Qt tests."""

import os
from collections.abc import Generator
from typing import Any
from unittest.mock import MagicMock

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


@pytest.fixture
def mock_settings() -> MagicMock:
    """Mock SettingsService for tests that instantiate MainWindow."""
    mock = MagicMock()
    # Return falsy values so _restore_layout_state() skips restore
    mock.window_geometry_state.get.return_value = ""
    mock.window_layout_state.get.return_value = ""
    # Settings accessed in setup_widgets()
    mock.debug_mode.get.return_value = False
    mock.max_multi_preview.get.return_value = 9
    # Settings accessed by ThumbnailService (created in setup_widgets)
    mock.cache_format.get.return_value = "PNG"
    mock.max_psd_workers.get.return_value = 3
    return mock
