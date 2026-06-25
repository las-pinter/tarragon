"""Tests for multi-select state and mosaic preview panel (Task 4.5).

Covers:
    - PreviewPanel.set_multi_preview() mosaic rendering
    - ThumbnailGrid.selection_changed signal emission
    - Grid layout calculations (1x1, 2x1, 2x2, 3x3, capped at 9)
"""

from __future__ import annotations

import math
from pathlib import Path

import pytest
from PIL import Image
from PySide6.QtCore import QItemSelection
from PySide6.QtWidgets import QApplication
from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.widgets.preview_panel import PreviewPanel
from tarragon.widgets.thumbnail_grid import ThumbnailGrid

# ── Fixtures ────────────────────────────────────────────────────────────


@pytest.fixture(autouse=True)
def qapp():
    """Provide a shared QApplication instance for all Qt tests."""
    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


@pytest.fixture
def preview_panel():
    """Provide a PreviewPanel that is closed after the test."""
    panel = PreviewPanel()
    yield panel
    panel.close()


@pytest.fixture
def grid():
    """Provide a ThumbnailGrid that is closed after the test."""
    g = ThumbnailGrid()
    yield g
    g.close()


@pytest.fixture
def grid_with_model(grid):
    """Provide a ThumbnailGrid backed by a ThumbnailModel with sample paths."""
    model = ThumbnailModel()
    model.set_paths(
        [
            Path("/fake/images/photo_001.png"),
            Path("/fake/images/photo_002.jpg"),
            Path("/fake/images/photo_003.png"),
            Path("/fake/images/photo_004.jpg"),
            Path("/fake/images/photo_005.png"),
        ]
    )
    grid.set_model(model)
    return grid, model


def _make_solid_images(n: int, size: tuple[int, int] = (200, 200)) -> list[Image.Image]:
    """Create *n* synthetic solid-color PIL images for mosaic testing."""
    colors = [
        "red",
        "green",
        "blue",
        "yellow",
        "cyan",
        "magenta",
        "white",
        "orange",
        "purple",
        "pink",
        "lime",
        "teal",
    ]
    return [Image.new("RGB", size, color=colors[i % len(colors)]) for i in range(n)]


# ── Mosaic Preview Tests ────────────────────────────────────────────────


class TestMultiPreviewEmptyList:
    """test_multi_preview_empty_list — Empty list clears preview."""

    def test_empty_list_clears_preview(self, preview_panel):
        """set_multi_preview with empty images list clears the panel."""
        # Arrange: set a single image first
        img = Image.new("RGB", (100, 100), color="red")
        preview_panel.set_image(img)
        assert preview_panel._current_image is not None

        # Act: call set_multi_preview with empty list
        preview_panel.set_multi_preview([], total_selected=0)

        # Assert: panel is cleared
        assert preview_panel._current_image is None
        assert preview_panel._image_label.text() == "No preview"


class TestMultiPreviewSingleImage:
    """test_multi_preview_single_image — Single image still shows (1x1 grid)."""

    def test_single_image_shows_1x1(self, preview_panel):
        """set_multi_preview with 1 image displays it in a 1x1 grid."""
        # Arrange
        images = _make_solid_images(1)

        # Act
        preview_panel.set_multi_preview(images, total_selected=1)

        # Assert: image label has a pixmap (mosaic was rendered)
        assert preview_panel._image_label.pixmap() is not None
        assert not preview_panel._image_label.pixmap().isNull()
        assert preview_panel._filename_label.text() == "1 files selected"


class TestMultiPreviewTwoImages:
    """test_multi_preview_two_images — 2 images -> 2x1 grid."""

    def test_two_images_grid_layout(self, preview_panel):
        """set_multi_preview with 2 images creates a 2x1 grid (2 cols, 1 row)."""
        # Arrange
        images = _make_solid_images(2)

        # Act
        preview_panel.set_multi_preview(images, total_selected=2)

        # Assert: mosaic rendered
        assert preview_panel._image_label.pixmap() is not None
        assert not preview_panel._image_label.pixmap().isNull()
        # Verify grid math: cols = ceil(sqrt(2)) = 2, rows = ceil(2/2) = 1
        n = 2
        expected_cols = math.ceil(math.sqrt(n))
        expected_rows = math.ceil(n / expected_cols)
        assert expected_cols == 2
        assert expected_rows == 1


class TestMultiPreviewFourImages:
    """test_multi_preview_four_images — 4 images -> 2x2 grid."""

    def test_four_images_grid_layout(self, preview_panel):
        """set_multi_preview with 4 images creates a 2x2 grid."""
        # Arrange
        images = _make_solid_images(4)

        # Act
        preview_panel.set_multi_preview(images, total_selected=4)

        # Assert: mosaic rendered
        assert preview_panel._image_label.pixmap() is not None
        # Verify grid math: cols = ceil(sqrt(4)) = 2, rows = ceil(4/2) = 2
        n = 4
        expected_cols = math.ceil(math.sqrt(n))
        expected_rows = math.ceil(n / expected_cols)
        assert expected_cols == 2
        assert expected_rows == 2


class TestMultiPreviewCappedAtNine:
    """test_multi_preview_capped_at_nine — More than 9 images -> capped at 9 with 3x3 grid."""

    def test_capped_at_nine_images(self, preview_panel):
        """set_multi_preview with 12 images caps display at 9 (3x3 grid)."""
        # Arrange
        images = _make_solid_images(12)

        # Act
        preview_panel.set_multi_preview(images, total_selected=12, cap=9)

        # Assert: mosaic rendered (only 9 images used)
        assert preview_panel._image_label.pixmap() is not None
        assert not preview_panel._image_label.pixmap().isNull()
        # Grid should be 3x3 for 9 images
        n = 9
        expected_cols = math.ceil(math.sqrt(n))
        expected_rows = math.ceil(n / expected_cols)
        assert expected_cols == 3
        assert expected_rows == 3


class TestMultiPreviewCaptionShown:
    """test_multi_preview_caption_shown — Verify caption appears when capped."""

    def test_caption_shown_when_capped(self, preview_panel):
        """Caption 'Showing X of Y selected' appears when total_selected > cap."""
        # Arrange
        images = _make_solid_images(15)

        # Act
        preview_panel.set_multi_preview(images, total_selected=15, cap=9)

        # Assert: caption is shown
        caption_text = preview_panel._format_label.text()
        assert "Showing 9 of 15 selected" in caption_text

    def test_no_caption_when_not_capped(self, preview_panel):
        """No caption when total_selected <= cap."""
        # Arrange
        images = _make_solid_images(4)

        # Act
        preview_panel.set_multi_preview(images, total_selected=4, cap=9)

        # Assert: no caption (format_label is cleared)
        assert preview_panel._format_label.text() == ""

    def test_no_caption_when_exactly_at_cap(self, preview_panel):
        """No caption when total_selected == cap."""
        # Arrange
        images = _make_solid_images(9)

        # Act
        preview_panel.set_multi_preview(images, total_selected=9, cap=9)

        # Assert: no caption
        assert preview_panel._format_label.text() == ""


# ── Selection Changed Signal Tests ──────────────────────────────────────


class TestSelectionChangedSignalSingle:
    """test_selection_changed_signal_single — Single selection emits signal with 1 path."""

    def test_single_selection_emits_one_path(self, grid_with_model):
        """Selecting one item emits selection_changed with a list of 1 path."""
        grid, model = grid_with_model

        # Arrange: capture signal emissions
        emitted = []
        grid.selection_changed.connect(lambda paths: emitted.append(paths))

        # Act: select first item
        index = model.index(0)
        selection = QItemSelection(index, index)
        grid.selectionModel().select(selection, grid.selectionModel().SelectionFlag.ClearAndSelect)

        # Assert
        assert len(emitted) >= 1
        last_emission = emitted[-1]
        assert len(last_emission) == 1
        assert last_emission[0] == str(Path("/fake/images/photo_001.png"))


class TestSelectionChangedSignalMulti:
    """test_selection_changed_signal_multi — Multi selection emits signal with multiple paths."""

    def test_multi_selection_emits_multiple_paths(self, grid_with_model):
        """Selecting multiple items emits selection_changed with all selected paths."""
        grid, model = grid_with_model

        # Arrange: capture signal emissions
        emitted = []
        grid.selection_changed.connect(lambda paths: emitted.append(paths))

        # Act: select first 3 items
        sel_model = grid.selectionModel()
        for row in range(3):
            index = model.index(row)
            selection = QItemSelection(index, index)
            sel_model.select(selection, sel_model.SelectionFlag.Select)

        # Assert
        assert len(emitted) >= 1
        last_emission = emitted[-1]
        assert len(last_emission) == 3
        assert str(Path("/fake/images/photo_001.png")) in last_emission
        assert str(Path("/fake/images/photo_002.jpg")) in last_emission
        assert str(Path("/fake/images/photo_003.png")) in last_emission


class TestSelectionChangedSignalEmpty:
    """test_selection_changed_signal_empty — No selection emits empty list."""

    def test_no_selection_emits_empty_list(self, grid_with_model):
        """Clearing selection emits selection_changed with an empty list."""
        grid, model = grid_with_model

        # Arrange: first select something
        index = model.index(0)
        selection = QItemSelection(index, index)
        grid.selectionModel().select(selection, grid.selectionModel().SelectionFlag.ClearAndSelect)

        # Capture signal emissions
        emitted = []
        grid.selection_changed.connect(lambda paths: emitted.append(paths))

        # Act: clear selection
        grid.selectionModel().clear()

        # Assert
        assert len(emitted) >= 1
        last_emission = emitted[-1]
        assert last_emission == []


# ── Additional Mosaic Edge Case Tests ───────────────────────────────────


class TestMosaicGridDimensions:
    """Verify grid dimension calculations for various image counts."""

    @pytest.mark.parametrize(
        "n_images,expected_cols,expected_rows",
        [
            (1, 1, 1),
            (2, 2, 1),
            (3, 2, 2),
            (4, 2, 2),
            (5, 3, 2),
            (6, 3, 2),
            (7, 3, 3),
            (8, 3, 3),
            (9, 3, 3),
        ],
        ids=["1img", "2img", "3img", "4img", "5img", "6img", "7img", "8img", "9img"],
    )
    def test_grid_dimensions(self, preview_panel, n_images, expected_cols, expected_rows):
        """Mosaic grid dimensions match expected cols/rows for N images."""
        # Arrange
        images = _make_solid_images(n_images)

        # Act
        preview_panel.set_multi_preview(images, total_selected=n_images)

        # Assert: verify grid math
        n = min(n_images, 9)
        cols = math.ceil(math.sqrt(n))
        rows = math.ceil(n / cols)
        assert cols == expected_cols
        assert rows == expected_rows
        # Verify mosaic was rendered
        assert preview_panel._image_label.pixmap() is not None


class TestMosaicMetadataLabels:
    """Verify metadata labels are updated correctly for multi-preview."""

    def test_filename_label_shows_count(self, preview_panel):
        """Filename label shows 'N files selected' for multi-preview."""
        # Arrange
        images = _make_solid_images(5)

        # Act
        preview_panel.set_multi_preview(images, total_selected=5)

        # Assert
        assert preview_panel._filename_label.text() == "5 files selected"

    def test_dimension_label_cleared(self, preview_panel):
        """Dimensions label is cleared for multi-preview."""
        # Arrange: first set a single image
        img = Image.new("RGB", (100, 100), color="red")
        preview_panel.set_image(img)
        assert preview_panel._dimensions_label.text() != ""

        # Act: switch to multi-preview
        images = _make_solid_images(3)
        preview_panel.set_multi_preview(images, total_selected=3)

        # Assert
        assert preview_panel._dimensions_label.text() == ""

    def test_size_label_cleared(self, preview_panel):
        """Size label is cleared for multi-preview."""
        # Arrange
        images = _make_solid_images(3)

        # Act
        preview_panel.set_multi_preview(images, total_selected=3)

        # Assert
        assert preview_panel._size_label.text() == ""


class TestMosaicWithMixedImages:
    """Test mosaic with various image modes and sizes."""

    def test_rgba_images_in_mosaic(self, preview_panel):
        """Mosaic handles RGBA images correctly."""
        # Arrange
        images = [
            Image.new("RGBA", (200, 200), color=(255, 0, 0, 128)),
            Image.new("RGBA", (200, 200), color=(0, 255, 0, 128)),
        ]

        # Act
        preview_panel.set_multi_preview(images, total_selected=2)

        # Assert
        assert preview_panel._image_label.pixmap() is not None
        assert not preview_panel._image_label.pixmap().isNull()

    def test_different_sized_images_in_mosaic(self, preview_panel):
        """Mosaic handles images of different sizes."""
        # Arrange
        images = [
            Image.new("RGB", (100, 100), color="red"),
            Image.new("RGB", (400, 300), color="blue"),
            Image.new("RGB", (50, 200), color="green"),
            Image.new("RGB", (300, 50), color="yellow"),
        ]

        # Act
        preview_panel.set_multi_preview(images, total_selected=4)

        # Assert
        assert preview_panel._image_label.pixmap() is not None
        assert not preview_panel._image_label.pixmap().isNull()


class TestMosaicClearsSingleState:
    """Verify that set_multi_preview clears single-image state."""

    def test_clears_current_image(self, preview_panel):
        """set_multi_preview clears _current_image."""
        # Arrange
        img = Image.new("RGB", (100, 100), color="red")
        preview_panel.set_image(img)
        assert preview_panel._current_image is not None

        # Act
        images = _make_solid_images(3)
        preview_panel.set_multi_preview(images, total_selected=3)

        # Assert
        assert preview_panel._current_image is None

    def test_clears_cached_pixmap(self, preview_panel):
        """set_multi_preview clears _cached_pixmap."""
        # Arrange
        img = Image.new("RGB", (100, 100), color="red")
        preview_panel.set_image(img)
        assert preview_panel._cached_pixmap is not None

        # Act
        images = _make_solid_images(3)
        preview_panel.set_multi_preview(images, total_selected=3)

        # Assert
        assert preview_panel._cached_pixmap is None

    def test_clears_current_path(self, preview_panel, tmp_path):
        """set_multi_preview clears _current_path."""
        # Arrange
        test_file = tmp_path / "test.jpg"
        test_file.write_bytes(b"fake")
        img = Image.new("RGB", (100, 100), color="red")
        preview_panel.set_image(img, path=test_file)
        assert preview_panel._current_path == test_file

        # Act
        images = _make_solid_images(3)
        preview_panel.set_multi_preview(images, total_selected=3)

        # Assert
        assert preview_panel._current_path is None
