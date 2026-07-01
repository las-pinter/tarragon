"""Tests for double-click launch flow integration (Task 6.2).

Verifies that double-clicking a thumbnail in the grid emits a signal
carrying the file path, and that MainWindow wires this signal to
launch_editor via the mediator pattern.
"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from typing import Any
from unittest.mock import patch

import pytest
from PySide6.QtCore import QPointF, Qt
from PySide6.QtGui import QMouseEvent
from PySide6.QtWidgets import QApplication
from tarragon.db import Database
from tarragon.main_window import MainWindow
from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.widgets.thumbnail_grid import ThumbnailGrid

# ── Fixtures ─────────────────────────────────────────────────────


@pytest.fixture(autouse=True)
def qapp() -> Generator[object, None, None]:
    """Provide a shared QApplication instance for all Qt tests."""
    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


@pytest.fixture()
def grid() -> Generator[ThumbnailGrid, None, None]:
    """Provide a ThumbnailGrid that is closed after the test."""
    g = ThumbnailGrid()
    yield g
    g.close()


@pytest.fixture()
def grid_with_model(grid: ThumbnailGrid) -> tuple[ThumbnailGrid, ThumbnailModel]:
    """Provide a ThumbnailGrid backed by a ThumbnailModel with sample paths."""
    model = ThumbnailModel()
    model.set_paths(
        [
            Path("/fake/images/photo_001.png"),
            Path("/fake/images/photo_002.jpg"),
            Path("/fake/images/layer_comp.psd"),
        ]
    )
    grid.set_model(model)
    return grid, model


@pytest.fixture()
def db() -> Generator[Database, None, None]:
    """Provide an in-memory database with schema initialised."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


@pytest.fixture()
def main_window(qapp: Any, db: Database) -> Generator[MainWindow, None, None]:  # noqa: ARG001
    """Provide a MainWindow with setup_widgets called."""
    from tarragon.services.tag_service import TagService

    window = MainWindow()
    tag_service = TagService(db)
    window.setup_widgets(db, tag_service)
    yield window
    window.close()


# ── Helper ───────────────────────────────────────────────────────


def _make_double_click_event(x: float, y: float) -> QMouseEvent:
    """Create a QMouseEvent simulating a double-click at (x, y)."""
    return QMouseEvent(
        QMouseEvent.Type.MouseButtonDblClick,
        QPointF(x, y),
        Qt.MouseButton.LeftButton,
        Qt.MouseButton.LeftButton,
        Qt.KeyboardModifier.NoModifier,
    )


# ── Test 1: Double-click emits signal with path ─────────────────


def test_double_click_emits_signal(grid_with_model: tuple[ThumbnailGrid, ThumbnailModel]) -> None:
    """Double-clicking on a valid item emits file_double_clicked with the file path."""
    grid, model = grid_with_model
    emitted: list[str] = []
    grid.file_double_clicked.connect(emitted.append)

    # Mock indexAt to return a valid index pointing to row 0
    valid_index = model.index(0)
    with patch.object(grid, "indexAt", return_value=valid_index):
        event = _make_double_click_event(50, 50)
        grid.mouseDoubleClickEvent(event)

    # Assert: signal emitted with the correct path
    assert len(emitted) == 1
    assert emitted[0] == "/fake/images/photo_001.png"


# ── Test 2: Double-click on empty area does nothing ─────────────


def test_double_click_on_empty_does_nothing(grid_with_model: tuple[ThumbnailGrid, ThumbnailModel]) -> None:
    """Double-clicking on an empty area (invalid index) does not emit the signal."""
    grid, _ = grid_with_model
    emitted: list[str] = []
    grid.file_double_clicked.connect(emitted.append)

    # Use a position far off-screen — indexAt returns invalid index
    event = _make_double_click_event(-9999, -9999)
    grid.mouseDoubleClickEvent(event)

    # Assert: no signal emitted
    assert len(emitted) == 0


# ── Test 3: MainWindow connects signal to handler ───────────────


def test_main_window_connects_signal(main_window: MainWindow) -> None:
    """MainWindow.setup_widgets connects file_double_clicked to _on_file_double_clicked."""
    window = main_window

    # Assert: the handler method exists
    assert hasattr(window, "_on_file_double_clicked")
    assert callable(window._on_file_double_clicked)

    # Assert: the signal is connected by emitting and checking the handler is called
    with patch.object(window, "_on_file_double_clicked") as mock_handler:
        window.thumbnail_grid.file_double_clicked.emit("/fake/test.png")

    mock_handler.assert_called_once_with("/fake/test.png")


# ── Test 4: Handler calls launch_editor with correct args ───────


def test_handler_calls_launch_editor(main_window: MainWindow) -> None:
    """_on_file_double_clicked calls launch_editor with db, path, and extension."""
    window = main_window

    with patch("tarragon.editors.launch_editor") as mock_launch:
        window._on_file_double_clicked("/fake/images/photo_001.png")

    mock_launch.assert_called_once()
    call_args = mock_launch.call_args
    # Positional args: (db, file_path, extension)
    assert call_args.args[0] is window._db
    assert call_args.args[1] == Path("/fake/images/photo_001.png")
    assert call_args.args[2] == ".png"


# ── Test 5: Handler correctly extracts file extension ───────────


@pytest.mark.parametrize(
    "path_str, expected_ext",
    [
        pytest.param("/fake/photo.psd", ".psd", id="psd"),
        pytest.param("/fake/photo.PSB", ".PSB", id="psb_upper"),
        pytest.param("/fake/photo.jpg", ".jpg", id="jpg"),
        pytest.param("/fake/no_extension", "", id="no_extension"),
        pytest.param("/fake/archive.tar.gz", ".gz", id="double_ext"),
    ],
)
def test_handler_extracts_extension(main_window: MainWindow, path_str: str, expected_ext: str) -> None:
    """_on_file_double_clicked correctly extracts the file extension from the path."""
    window = main_window

    with patch("tarragon.editors.launch_editor") as mock_launch:
        window._on_file_double_clicked(path_str)

    mock_launch.assert_called_once()
    call_args = mock_launch.call_args
    assert call_args.args[2] == expected_ext
    assert call_args.args[1] == Path(path_str)
