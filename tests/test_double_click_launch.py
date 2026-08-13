"""Tests for ThumbnailGrid double-click launch"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from typing import Any
from unittest.mock import patch

import pytest
from PySide6.QtCore import QPointF, Qt
from PySide6.QtGui import QMouseEvent

from tarragon.db.database import Database
from tarragon.main_window import MainWindow
from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.services.tag_service import TagService
from tarragon.widgets.thumbnail_grid import ThumbnailGrid


@pytest.fixture
def grid() -> Generator[ThumbnailGrid, None, None]:
    """Provide a ThumbnailGrid that is closed after the test."""
    g = ThumbnailGrid()
    yield g
    g.close()


@pytest.fixture
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


@pytest.fixture
def db() -> Generator[Database, None, None]:
    """Provide an in-memory database with schema initialised."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


@pytest.fixture
def main_window(
    qapp: Any,
    db: Database,
    mock_settings: Any,
) -> Generator[MainWindow, None, None]:
    """Provide a MainWindow with setup_widgets called."""
    window = MainWindow(settings_service=mock_settings)
    tag_service = TagService(db)
    window.setup_widgets(db, tag_service)
    yield window
    window.close()


def _make_double_click_event(x: float, y: float) -> QMouseEvent:
    """Create a QMouseEvent simulating a double-click at (x, y)."""
    return QMouseEvent(
        QMouseEvent.Type.MouseButtonDblClick,
        QPointF(x, y),
        Qt.MouseButton.LeftButton,
        Qt.MouseButton.LeftButton,
        Qt.KeyboardModifier.NoModifier,
    )


class TestDoubleClickSignal:
    """Double-clicking a thumbnail emits the file_double_clicked signal."""

    def test_double_click_emits_signal(self, grid_with_model: tuple[ThumbnailGrid, ThumbnailModel]) -> None:
        """Double-clicking a valid item emits the signal with the file path."""
        grid, model = grid_with_model
        emitted: list[str] = []
        grid.file_double_clicked.connect(emitted.append)

        valid_index = model.index(0)
        with patch.object(grid, "indexAt", return_value=valid_index):
            event = _make_double_click_event(50, 50)
            grid.mouseDoubleClickEvent(event)

        assert len(emitted) == 1
        assert emitted[0] == str(Path("/fake/images/photo_001.png"))

    def test_double_click_on_empty_does_nothing(self, grid_with_model: tuple[ThumbnailGrid, ThumbnailModel]) -> None:
        """Double-clicking on an empty area does not emit the signal."""
        grid, _ = grid_with_model
        emitted: list[str] = []
        grid.file_double_clicked.connect(emitted.append)

        # Use a position far off-screen - indexAt returns an invalid index
        event = _make_double_click_event(-9999, -9999)
        grid.mouseDoubleClickEvent(event)

        assert len(emitted) == 0


class TestSignalWiring:
    """MainWindow wires file_double_clicked to its handler."""

    def test_main_window_connects_signal(self, main_window: MainWindow) -> None:
        """The setup_widgets method connects file_double_clicked to _on_file_double_clicked."""
        window = main_window

        assert hasattr(window, "_on_file_double_clicked")
        assert callable(window._on_file_double_clicked)

        with patch.object(window, "_on_file_double_clicked") as mock_handler:
            window.thumbnail_grid.file_double_clicked.emit("/fake/test.png")

        mock_handler.assert_called_once_with("/fake/test.png")


class TestLaunchHandler:
    """The double-click handler calls launch_editor with the correct args."""

    def test_handler_calls_launch_editor(self, main_window: MainWindow) -> None:
        """The _on_file_double_clicked handler calls launch_editor with db, path, and extension."""
        window = main_window

        with patch("tarragon.services.editors.launch_editor") as mock_launch:
            window._on_file_double_clicked("/fake/images/photo_001.png")

        mock_launch.assert_called_once()
        call_args = mock_launch.call_args
        assert call_args.args[0] is window._db
        assert call_args.args[1] == Path("/fake/images/photo_001.png")
        assert call_args.args[2] == ".png"

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
    def test_handler_extracts_extension(self, main_window: MainWindow, path_str: str, expected_ext: str) -> None:
        """The _on_file_double_clicked handler extracts the extension from the path."""
        window = main_window

        with patch("tarragon.services.editors.launch_editor") as mock_launch:
            window._on_file_double_clicked(path_str)

        mock_launch.assert_called_once()
        call_args = mock_launch.call_args
        assert call_args.args[2] == expected_ext
        assert call_args.args[1] == Path(path_str)
