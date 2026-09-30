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
    mock.clear_full_res_on_exit.get.return_value = False
    return mock


@pytest.fixture(scope="session", autouse=True)
def synchronous_workers() -> Generator[None, None, None]:
    """Run background QRunnables inline so navigation/purge tests observe synchronous state.

    Production defaults to truly async (see ``synchronous_workers`` class
    attributes); the seam is activated here for the entire test session so
    existing tests that assert state immediately after a navigation call
    keep passing unmodified. Tests that exercise the real async path can
    disable the seam per instance.
    """
    from tarragon.gallery_controller import GalleryController
    from tarragon.services.thumbnail_service import ThumbnailService

    previous_service = ThumbnailService.synchronous_workers
    previous_controller = GalleryController.synchronous_workers
    ThumbnailService.synchronous_workers = True
    GalleryController.synchronous_workers = True
    yield
    ThumbnailService.synchronous_workers = previous_service
    GalleryController.synchronous_workers = previous_controller
