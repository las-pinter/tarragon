"""Tests for PreviewPanel"""

from __future__ import annotations

import gc
from collections.abc import Generator
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import pytest
from PIL import Image
from PySide6.QtCore import QSize, Qt
from PySide6.QtGui import QResizeEvent
from PySide6.QtWidgets import (
    QGraphicsOpacityEffect,
    QLabel,
    QLineEdit,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from tarragon.common import ImageInfo
from tarragon.db.common.tag import Tag, TagSource
from tarragon.db.database import Database
from tarragon.services.settings_service import SettingsService
from tarragon.theme.color_buckets import BUCKET_COLORS
from tarragon.widgets.flow_layout import FlowLayout
from tarragon.widgets.metadata import SizeMeta
from tarragon.widgets.preview_panel import PreviewPanel


@pytest.fixture
def sample_image() -> Image.Image:
    """Create a sample PIL Image for testing."""
    return Image.new("RGB", (800, 600), color="red")


@pytest.fixture
def sample_rgba_image() -> Image.Image:
    """Create a sample RGBA PIL Image for testing."""
    return Image.new("RGBA", (1024, 768), color=(0, 255, 0, 128))


@pytest.fixture
def settings_service() -> SettingsService:
    """SettingsService backed by in-memory database."""
    db = Database(Path(":memory:"))
    db.init_schema()
    return SettingsService(db)


@pytest.fixture
def preview_panel(settings_service: SettingsService) -> Generator[PreviewPanel, None, None]:
    """Provide a PreviewPanel that is closed after the test."""
    panel = PreviewPanel(settings_service)
    yield panel
    panel.close()


@pytest.fixture
def mock_tag_service() -> MagicMock:
    """Create a mock TagService for testing tag management."""
    # Create a mock that has the tags_changed signal
    service = MagicMock()
    service.tags_changed = MagicMock()
    service.get_tags_for_file.return_value = []
    service.get_all_tags.return_value = []
    service.get_file_tag_ids_batch.return_value = {}
    service.get_tag_name.return_value = None
    return service


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


def _make_image_info(
    img: Image.Image,
    path: Path | None = None,
) -> ImageInfo:
    """Wrap a PIL Image in an ImageInfo NamedTuple for test use."""
    return ImageInfo(image=img, path=path, width=None, height=None)


def _make_image_infos(
    images: list[Image.Image],
) -> list[ImageInfo]:
    """Wrap a list of PIL Images in ImageInfo NamedTuples (no paths)."""
    return [ImageInfo(image=img, path=None, width=None, height=None) for img in images]


def _meta_text(panel: PreviewPanel, meta_name: str) -> str:
    """Get the current text of a metadata label by grid name.

    Args:
        panel: The PreviewPanel under test.
        meta_name: One of "File", "Dimensions", "Size", "Format".
    """
    return panel._metadata_grid._metadata_dict[meta_name].get_label_widget().text()


def _solid_image_infos(n: int, size: tuple[int, int] = (200, 200)) -> list[ImageInfo]:
    """Create *n* ImageInfo objects with solid-color images (no paths)."""
    return _make_image_infos(_make_solid_images(n, size))


class TestInstantiation:
    """PreviewPanel construction and basic structure."""

    def test_preview_panel_is_qwidget(self) -> None:
        """PreviewPanel is a QWidget subclass."""
        assert issubclass(PreviewPanel, QWidget)

    def test_preview_panel_instantiation(self, settings_service: SettingsService) -> None:
        """PreviewPanel can be created without errors."""
        panel = PreviewPanel(settings_service)
        try:
            assert panel is not None
            assert isinstance(panel, QWidget)
        finally:
            panel.close()

    def test_preview_panel_has_layout(self, settings_service: SettingsService) -> None:
        """PreviewPanel has a QVBoxLayout."""
        panel = PreviewPanel(settings_service)
        try:
            layout = panel.layout()
            assert isinstance(layout, QVBoxLayout)
        finally:
            panel.close()

    def test_preview_panel_has_image_label(self, settings_service: SettingsService) -> None:
        """PreviewPanel has an image QLabel."""
        panel = PreviewPanel(settings_service)
        try:
            assert hasattr(panel, "_image_label")
            assert isinstance(panel._image_label, QLabel)
        finally:
            panel.close()

    def test_image_label_size_policy_is_ignored(self, settings_service: SettingsService) -> None:
        """Image label size policy is Ignored/Ignored to prevent resize loop."""
        panel = PreviewPanel(settings_service)
        try:
            policy = panel._image_label.sizePolicy()
            assert policy.horizontalPolicy() == QSizePolicy.Policy.Ignored
            assert policy.verticalPolicy() == QSizePolicy.Policy.Ignored
        finally:
            panel.close()

    def test_preview_panel_initial_state(self, settings_service: SettingsService) -> None:
        """PreviewPanel starts with no image and empty metadata."""
        panel = PreviewPanel(settings_service)
        try:
            assert panel._image_label.text() == "No preview"
            assert _meta_text(panel, "File") == ""
            assert _meta_text(panel, "Dimensions") == ""
        finally:
            panel.close()


class TestSetImage:
    """The set_image() method displays images and metadata."""

    def test_set_image_rgb(self, sample_image: Any, settings_service: SettingsService) -> None:
        """The set_image() method displays an RGB image."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_image(_make_image_info(sample_image))
            assert panel._image_label.pixmap() is not None
            assert _meta_text(panel, "Dimensions") == "800 x 600"
        finally:
            panel.close()

    def test_set_image_rgba(self, sample_rgba_image: Any, settings_service: SettingsService) -> None:
        """The set_image() method displays an RGBA image."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_image(_make_image_info(sample_rgba_image))
            assert panel._image_label.pixmap() is not None
            assert _meta_text(panel, "Dimensions") == "1024 x 768"
        finally:
            panel.close()

    def test_set_image_with_path(self, sample_image: Any, tmp_path: Any, settings_service: SettingsService) -> None:
        """The set_image() method displays metadata when a path is provided."""
        # Create a dummy file
        test_file = tmp_path / "test_image.jpg"
        test_file.write_bytes(b"fake image data")

        panel = PreviewPanel(settings_service)
        try:
            panel.set_image(_make_image_info(sample_image, path=test_file))
            assert _meta_text(panel, "File") == "test_image.jpg"
            assert "Size:" in _meta_text(panel, "Size")
            assert _meta_text(panel, "Format") == "JPG"
        finally:
            panel.close()

    def test_set_image_without_path(self, sample_image: Any, settings_service: SettingsService) -> None:
        """The set_image() method works without a path (shows 'Unknown')."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_image(_make_image_info(sample_image))
            assert _meta_text(panel, "File") == "Unknown"
            assert _meta_text(panel, "Size") == "Unknown"
        finally:
            panel.close()


class TestClear:
    """The clear() method resets the panel to its initial state."""

    def test_clear_resets_state(self, sample_image: Any, settings_service: SettingsService) -> None:
        """The clear() method resets the panel to its initial state."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_image(_make_image_info(sample_image))

            panel.clear()
            assert panel._image_label.text() == "No preview"
            assert _meta_text(panel, "File") == ""
        finally:
            panel.close()


class TestMetadataFormatting:
    """The _format_size() function formats byte sizes across all ranges."""

    @pytest.mark.parametrize(
        "size_bytes,expected",
        [
            (0, "0 B"),
            (1, "1 B"),
            (500, "500 B"),
            (1024, "1.0 KB"),
            (1536, "1.5 KB"),
            (2 * 1024 * 1024, "2.0 MB"),
            (3 * 1024 * 1024 * 1024, "3.0 GB"),
            (5 * 1024**4, "5.0 TB"),
        ],
    )
    def test_format_size_parametrized(self, size_bytes: int, expected: str) -> None:
        """The _format_size() function formats all byte ranges correctly."""
        assert SizeMeta.format_size(size_bytes) == expected


class TestPilToQImage:
    """The _to_qimage() function converts common PIL modes to QImage."""

    def test_to_qimage_rgb(self, sample_image: Any) -> None:
        """The _to_qimage() function converts an RGB image correctly."""
        qimage = PreviewPanel._to_qimage(sample_image)
        assert qimage.width() == 800
        assert qimage.height() == 600

    def test_to_qimage_rgba(self, sample_rgba_image: Any) -> None:
        """The _to_qimage() function converts an RGBA image correctly."""
        qimage = PreviewPanel._to_qimage(sample_rgba_image)
        assert qimage.width() == 1024
        assert qimage.height() == 768

    def test_to_qimage_grayscale(self, qapp: Any) -> None:
        """The _to_qimage() function converts a grayscale image to RGB."""
        gray_image = Image.new("L", (400, 300), color=128)
        qimage = PreviewPanel._to_qimage(gray_image)
        assert qimage.width() == 400
        assert qimage.height() == 300


class TestSetImageBoundarySizes:
    """The set_image() method handles extreme image dimensions."""

    def test_set_image_1x1_pixel(self, settings_service: SettingsService) -> None:
        """The set_image() method handles a 1x1 pixel image without crashing."""
        panel = PreviewPanel(settings_service)
        try:
            tiny = Image.new("RGB", (1, 1), color="blue")
            panel.set_image(_make_image_info(tiny))
            assert panel._image_label.pixmap() is not None
            assert _meta_text(panel, "Dimensions") == "1 x 1"
        finally:
            panel.close()

    def test_set_image_very_wide_panorama(self, settings_service: SettingsService) -> None:
        """The set_image() method handles an extreme aspect ratio (wide panorama)."""
        panel = PreviewPanel(settings_service)
        try:
            wide = Image.new("RGB", (4000, 100), color="green")
            panel.set_image(_make_image_info(wide))
            assert panel._image_label.pixmap() is not None
            assert _meta_text(panel, "Dimensions") == "4000 x 100"
        finally:
            panel.close()

    def test_set_image_very_tall(self, settings_service: SettingsService) -> None:
        """The set_image() method handles an extreme aspect ratio (tall/narrow image)."""
        panel = PreviewPanel(settings_service)
        try:
            tall = Image.new("RGB", (50, 5000), color="yellow")
            panel.set_image(_make_image_info(tall))
            assert panel._image_label.pixmap() is not None
            assert _meta_text(panel, "Dimensions") == "50 x 5000"
        finally:
            panel.close()

    def test_set_image_large_25mp(self, settings_service: SettingsService) -> None:
        """The set_image() method handles a large 25-megapixel image (5000x5000)."""
        panel = PreviewPanel(settings_service)
        try:
            large = Image.new("RGB", (5000, 5000), color="white")
            panel.set_image(_make_image_info(large))
            assert panel._image_label.pixmap() is not None
            assert _meta_text(panel, "Dimensions") == "5000 x 5000"
        finally:
            panel.close()


class TestSetImageModeConversions:
    """The set_image() method converts unusual PIL modes for display."""

    def test_set_image_cmyk_mode(self, settings_service: SettingsService) -> None:
        """The set_image() method converts a CMYK image to RGB for display."""
        panel = PreviewPanel(settings_service)
        try:
            cmyk = Image.new("CMYK", (200, 200), color=(0, 0, 0, 0))
            panel.set_image(_make_image_info(cmyk))
            assert panel._image_label.pixmap() is not None
            assert _meta_text(panel, "Dimensions") == "200 x 200"
        finally:
            panel.close()

    def test_set_image_palette_mode(self, settings_service: SettingsService) -> None:
        """The set_image() method converts a palette (P) mode image for display."""
        panel = PreviewPanel(settings_service)
        try:
            palette_img = Image.new("P", (300, 200))
            palette_img.putpalette([i % 256 for i in range(768)])
            panel.set_image(_make_image_info(palette_img))
            assert panel._image_label.pixmap() is not None
        finally:
            panel.close()

    def test_set_image_1bit_mode(self, settings_service: SettingsService) -> None:
        """The set_image() method converts a 1-bit binary image for display."""
        panel = PreviewPanel(settings_service)
        try:
            binary = Image.new("1", (100, 100), color=1)
            panel.set_image(_make_image_info(binary))
            assert panel._image_label.pixmap() is not None
        finally:
            panel.close()

    def test_set_image_la_mode(self, settings_service: SettingsService) -> None:
        """The set_image() method converts an LA (grayscale + alpha) image for display."""
        panel = PreviewPanel(settings_service)
        try:
            la_img = Image.new("LA", (200, 200), color=(128, 255))
            panel.set_image(_make_image_info(la_img))
            assert panel._image_label.pixmap() is not None
        finally:
            panel.close()

    def test_set_image_i_mode_32bit(self, settings_service: SettingsService) -> None:
        """The set_image() method converts an I (32-bit integer) mode image for display."""
        panel = PreviewPanel(settings_service)
        try:
            i_img = Image.new("I", (200, 200), color=1000)
            panel.set_image(_make_image_info(i_img))
            assert panel._image_label.pixmap() is not None
        finally:
            panel.close()

    def test_set_image_f_mode_float(self, settings_service: SettingsService) -> None:
        """The set_image() method converts an F (float) mode image for display."""
        panel = PreviewPanel(settings_service)
        try:
            f_img = Image.new("F", (200, 200), color=1.5)
            panel.set_image(_make_image_info(f_img))
            assert panel._image_label.pixmap() is not None
        finally:
            panel.close()


class TestSetImageAnimatedGif:
    """The set_image() method displays the first frame of an animated GIF."""

    def test_set_image_animated_gif_shows_first_frame(self, tmp_path: Any, settings_service: SettingsService) -> None:
        """The set_image() method displays the first frame of an animated GIF."""
        # Create a multi-frame GIF
        frames = [
            Image.new("RGB", (100, 100), color="red"),
            Image.new("RGB", (100, 100), color="green"),
            Image.new("RGB", (100, 100), color="blue"),
        ]
        gif_path = tmp_path / "animated.gif"
        frames[0].save(
            gif_path,
            save_all=True,
            append_images=frames[1:],
            duration=100,
            loop=0,
        )

        panel = PreviewPanel(settings_service)
        try:
            gif_image = Image.open(gif_path)
            panel.set_image(_make_image_info(gif_image, path=gif_path))
            # Should show first frame dimensions
            assert _meta_text(panel, "Dimensions") == "100 x 100"
            assert panel._image_label.pixmap() is not None
            gif_image.close()
        finally:
            panel.close()


class TestResizeEvent:
    """The resizeEvent handler rescales the cached pixmap without reconversion."""

    def test_resize_event_with_no_image_does_not_crash(self, settings_service: SettingsService) -> None:
        """The resizeEvent handler does not crash when no image is set."""
        panel = PreviewPanel(settings_service)
        try:
            panel.show()
            event = QResizeEvent(QSize(400, 300), QSize(200, 200))
            panel.resizeEvent(event)
            # Should not crash, image label should still say "No preview"
        finally:
            panel.close()

    def test_resize_event_with_image_reapplies(self, sample_image: Any, settings_service: SettingsService) -> None:
        """The resizeEvent handler re-scales the image when the panel is resized."""
        panel = PreviewPanel(settings_service)
        try:
            panel.show()
            panel.set_image(_make_image_info(sample_image))

            # Simulate resize
            event = QResizeEvent(QSize(600, 400), QSize(200, 200))
            panel.resizeEvent(event)

            # Image should still be set after resize
            assert panel._image_label.pixmap() is not None
        finally:
            panel.close()

    def test_set_image_caches_pixmap(self, sample_image: Any, settings_service: SettingsService) -> None:
        """The set_image() method caches the full-resolution pixmap for fast resizing."""
        panel = PreviewPanel(settings_service)
        try:
            assert panel._cached_pixmap is None
            panel.set_image(_make_image_info(sample_image))
            assert panel._cached_pixmap is not None
            assert not panel._cached_pixmap.isNull()
        finally:
            panel.close()

    def test_resize_event_does_not_reconvert(self, sample_image: Any, settings_service: SettingsService) -> None:
        """The resizeEvent handler re-scales the cached pixmap without re-converting from PIL."""
        panel = PreviewPanel(settings_service)
        try:
            panel.show()
            panel.set_image(_make_image_info(sample_image))
            cached = panel._cached_pixmap
            assert cached is not None

            # Spy on _to_qimage - it should NOT be called during resize
            with patch.object(PreviewPanel, "_to_qimage", wraps=PreviewPanel._to_qimage) as spy:
                event = QResizeEvent(QSize(600, 400), QSize(200, 200))
                panel.resizeEvent(event)
                assert spy.call_count == 0

            # Cached pixmap should be the same object (not re-created)
            assert panel._cached_pixmap is cached
            assert panel._image_label.pixmap() is not None
        finally:
            panel.close()

    def test_clear_resets_cached_pixmap(self, sample_image: Any, settings_service: SettingsService) -> None:
        """The clear() method resets the cached pixmap to None."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_image(_make_image_info(sample_image))
            assert panel._cached_pixmap is not None
            panel.clear()
            assert panel._cached_pixmap is None
        finally:
            panel.close()


class TestSetImageRapidCalls:
    """Rapid successive set_image calls behave correctly."""

    def test_multiple_set_image_calls_last_one_wins(self, settings_service: SettingsService) -> None:
        """The last of multiple rapid set_image calls is displayed."""
        panel = PreviewPanel(settings_service)
        try:
            img1 = Image.new("RGB", (100, 100), color="red")
            img2 = Image.new("RGB", (200, 200), color="green")
            img3 = Image.new("RGB", (300, 300), color="blue")

            panel.set_image(_make_image_info(img1))
            panel.set_image(_make_image_info(img2))
            panel.set_image(_make_image_info(img3))

            assert _meta_text(panel, "Dimensions") == "300 x 300"
        finally:
            panel.close()

    def test_set_image_after_clear(self, sample_image: Any, settings_service: SettingsService) -> None:
        """The set_image() method works correctly after clear()."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_image(_make_image_info(sample_image))
            panel.clear()

            new_img = Image.new("RGB", (50, 50), color="cyan")
            panel.set_image(_make_image_info(new_img))
            assert _meta_text(panel, "Dimensions") == "50 x 50"
        finally:
            panel.close()


class TestSetImagePathErrors:
    """The set_image() method handles missing or unreadable paths gracefully."""

    def test_set_image_with_nonexistent_path_shows_unknown_size(
        self, sample_image: Any, settings_service: SettingsService
    ) -> None:
        """The set_image() method shows 'Unknown' for size with a nonexistent path."""
        panel = PreviewPanel(settings_service)
        try:
            fake_path = Path("/nonexistent/path/to/image.jpg")
            panel.set_image(_make_image_info(sample_image, path=fake_path))
            assert _meta_text(panel, "File") == "image.jpg"
            assert _meta_text(panel, "Size") == "Unknown"
        finally:
            panel.close()

    def test_set_image_with_path_stat_raises_oserror(
        self, sample_image: Any, settings_service: SettingsService
    ) -> None:
        """The set_image() method handles OSError from path.stat() gracefully."""
        panel = PreviewPanel(settings_service)
        try:
            real_path = Path("/tmp/some_image.png")
            with patch.object(Path, "stat", side_effect=OSError("Permission denied")):
                panel.set_image(_make_image_info(sample_image, path=real_path))
            assert _meta_text(panel, "Size") == "Unknown"
            assert _meta_text(panel, "File") == "some_image.png"
        finally:
            panel.close()


class TestSetImageFilenames:
    """The set_image() method handles Unicode and very long filenames."""

    def test_set_image_with_unicode_filename(
        self, sample_image: Any, tmp_path: Any, settings_service: SettingsService
    ) -> None:
        """The set_image() method handles Unicode characters in a filename."""
        panel = PreviewPanel(settings_service)
        try:
            unicode_file = tmp_path / "тест_画像_🖼️.png"
            unicode_file.write_bytes(b"fake data")
            panel.set_image(_make_image_info(sample_image, path=unicode_file))
            assert _meta_text(panel, "File") == "тест_画像_🖼️.png"
        finally:
            panel.close()

    def test_set_image_with_very_long_filename(
        self, sample_image: Any, tmp_path: Any, settings_service: SettingsService
    ) -> None:
        """The set_image() method handles a very long filename without crashing."""
        panel = PreviewPanel(settings_service)
        try:
            long_name = "a" * 200 + ".png"
            long_file = tmp_path / long_name
            long_file.write_bytes(b"fake data")
            panel.set_image(_make_image_info(sample_image, path=long_file))
            assert _meta_text(panel, "File") == long_name
        finally:
            panel.close()


class TestFormatFallback:
    """Format metadata falls back to the path extension."""

    def test_format_fallback_to_path_extension(self, tmp_path: Any, settings_service: SettingsService) -> None:
        """Format falls back to path extension when image.format is None."""
        panel = PreviewPanel(settings_service)
        try:
            test_file = tmp_path / "photo.psd"
            test_file.write_bytes(b"fake psd data")
            # Create image with no format set (as happens with Image.new)
            img = Image.new("RGB", (100, 100))
            assert img.format is None  # confirm precondition
            panel.set_image(_make_image_info(img, path=test_file))
            assert _meta_text(panel, "Format") == "PSD"
        finally:
            panel.close()

    def test_format_shows_unknown_when_no_format_and_no_path(self, settings_service: SettingsService) -> None:
        """Format shows 'Unknown' when image has no format and no path given."""
        panel = PreviewPanel(settings_service)
        try:
            img = Image.new("RGB", (100, 100))
            assert img.format is None
            panel.set_image(_make_image_info(img))
            assert _meta_text(panel, "Format") == "Unknown"
        finally:
            panel.close()

    def test_format_prefers_path_extension_over_pil_format(
        self, tmp_path: Any, settings_service: SettingsService
    ) -> None:
        """Format derives from path extension, not PIL format."""
        panel = PreviewPanel(settings_service)
        try:
            test_file = tmp_path / "photo.jpeg"
            test_file.write_bytes(b"fake data")
            img = Image.new("RGB", (100, 100))
            img.format = "PNG"  # simulate cached PNG thumbnail
            panel.set_image(_make_image_info(img, path=test_file))
            # Should use path extension (JPEG), not PIL format (PNG)
            assert _meta_text(panel, "Format") == "JPEG"
        finally:
            panel.close()


class TestStateConsistency:
    """The set_image() and clear() methods maintain consistent panel state."""

    def test_set_image_replaces_previous_image(self, settings_service: SettingsService) -> None:
        """The set_image() method fully replaces the previous image and metadata."""
        panel = PreviewPanel(settings_service)
        try:
            img1 = Image.new("RGB", (100, 100), color="red")
            img2 = Image.new("RGBA", (500, 400), color=(0, 0, 255, 128))

            panel.set_image(_make_image_info(img1))
            assert _meta_text(panel, "Dimensions") == "100 x 100"

            panel.set_image(_make_image_info(img2))
            assert _meta_text(panel, "Dimensions") == "500 x 400"
        finally:
            panel.close()

    def test_clear_when_already_clear_is_idempotent(self, settings_service: SettingsService) -> None:
        """The clear() method on an already-cleared panel does not crash."""
        panel = PreviewPanel(settings_service)
        try:
            panel.clear()
            panel.clear()

            assert panel._image_label.text() == "No preview"
        finally:
            panel.close()


class TestPilToQImageDeepCopy:
    """The _to_qimage() function returns a deep copy that survives garbage collection."""

    def test_to_qimage_returns_deep_copy_survives_gc(self, settings_service: SettingsService) -> None:
        """The _to_qimage() function returns a deep copy that survives garbage collection."""
        img = Image.new("RGB", (50, 50), color="red")
        qimage = PreviewPanel._to_qimage(img)

        # Delete source image and force GC
        del img
        gc.collect()

        # QImage should still be valid
        assert qimage.width() == 50
        assert qimage.height() == 50
        assert not qimage.isNull()


class TestPathDisplay:
    """Metadata shows only the filename, not the full path."""

    def test_set_image_shows_only_filename_not_full_path(
        self, sample_image: Any, tmp_path: Any, settings_service: SettingsService
    ) -> None:
        """Metadata shows only filename, not full path."""
        panel = PreviewPanel(settings_service)
        try:
            nested = tmp_path / "subdir" / "deep" / "image.png"
            nested.parent.mkdir(parents=True, exist_ok=True)
            nested.write_bytes(b"fake")
            panel.set_image(_make_image_info(sample_image, path=nested))
            assert _meta_text(panel, "File") == "image.png"
            assert "/" not in _meta_text(panel, "File")
        finally:
            panel.close()


class TestPilToQImageStride:
    """Stride mismatch regression: pixel values are preserved for non-aligned widths."""

    @pytest.mark.parametrize(
        "width",
        [1, 3, 5, 7, 9, 11, 13, 15, 17, 21, 63, 101],
    )
    def test_to_qimage_rgb_pixel_values_non_aligned_widths(self, width: int) -> None:
        """The _to_qimage() function preserves pixel values for RGB widths not divisible by 4."""
        height = 4
        # Create an image with unique pixel values per row so shearing is detectable
        pil_img = Image.new("RGB", (width, height))
        for y in range(height):
            for x in range(width):
                # Each pixel gets a unique color based on position
                pil_img.putpixel((x, y), ((x * 37 + y * 13) % 256, (x * 53 + y * 7) % 256, (x * 11 + y * 97) % 256))

        qimage = PreviewPanel._to_qimage(pil_img)

        assert qimage.width() == width
        assert qimage.height() == height

        # Verify EVERY pixel - if stride is wrong, lower rows will be sheared
        for y in range(height):
            for x in range(width):
                expected = pil_img.getpixel((x, y))
                qcolor = qimage.pixelColor(x, y)
                actual = (qcolor.red(), qcolor.green(), qcolor.blue())
                assert actual == expected, (
                    f"Pixel mismatch at ({x}, {y}) for width={width}: "
                    f"expected {expected}, got {actual} - stride shearing detected!"
                )

    def test_to_qimage_rgba_pixel_values(self, qapp: Any) -> None:
        """The _to_qimage() function preserves RGBA pixel values including the alpha channel."""
        width, height = 7, 3
        pil_img = Image.new("RGBA", (width, height))
        for y in range(height):
            for x in range(width):
                pil_img.putpixel((x, y), ((x * 41) % 256, (y * 67) % 256, ((x + y) * 23) % 256, (x * y * 17) % 256))

        qimage = PreviewPanel._to_qimage(pil_img)

        for y in range(height):
            for x in range(width):
                expected = pil_img.getpixel((x, y))
                qcolor = qimage.pixelColor(x, y)
                actual = (qcolor.red(), qcolor.green(), qcolor.blue(), qcolor.alpha())
                assert actual == expected, f"RGBA pixel mismatch at ({x}, {y}): expected {expected}, got {actual}"


class TestMultiPreviewEmptyList:
    """Empty list clears the preview."""

    def test_empty_list_clears_preview(self, preview_panel: PreviewPanel) -> None:
        """The set_multi_preview() method clears the panel when given an empty images list."""
        img = Image.new("RGB", (100, 100), color="red")
        preview_panel.set_image(_make_image_info(img))

        preview_panel.set_multi_preview([])

        assert preview_panel._image_label.text() == "No preview"


class TestMosaicMetadataLabels:
    """Metadata labels update correctly for multi-preview."""

    def test_filename_label_shows_count(self, preview_panel: PreviewPanel) -> None:
        """Filename label shows 'N files selected' for multi-preview."""
        infos = _solid_image_infos(5)

        preview_panel.set_multi_preview(infos)

        assert _meta_text(preview_panel, "File") == "5 files selected"

    def test_dimension_label_cleared(self, preview_panel: PreviewPanel) -> None:
        """Dimensions label shows 'Multiple file dimensions' for multi-preview."""
        # First set a single image
        img = Image.new("RGB", (100, 100), color="red")
        preview_panel.set_image(_make_image_info(img))
        assert _meta_text(preview_panel, "Dimensions") != ""

        # Switch to multi-preview
        infos = _solid_image_infos(3)
        preview_panel.set_multi_preview(infos)

        assert _meta_text(preview_panel, "Dimensions") == "Multiple file dimensions"

    def test_size_label_cleared(self, preview_panel: PreviewPanel) -> None:
        """Size label shows 'Unknown' for multi-preview with no paths."""
        infos = _solid_image_infos(3)

        preview_panel.set_multi_preview(infos)

        assert _meta_text(preview_panel, "Size") == "Unknown"


class TestMosaicClearsSingleState:
    """The set_multi_preview() method clears single-image state."""

    def test_clears_current_image(self, preview_panel: PreviewPanel) -> None:
        """The set_multi_preview() method clears _current_image."""
        img = Image.new("RGB", (100, 100), color="red")
        preview_panel.set_image(_make_image_info(img))

        preview_panel.set_multi_preview(_solid_image_infos(3))

    def test_clears_cached_pixmap(self, preview_panel: PreviewPanel) -> None:
        """The set_multi_preview() method clears _cached_pixmap."""
        img = Image.new("RGB", (100, 100), color="red")
        preview_panel.set_image(_make_image_info(img))
        assert preview_panel._cached_pixmap is not None

        preview_panel.set_multi_preview(_solid_image_infos(3))

        assert preview_panel._cached_pixmap is None


class TestMosaicModeConversion:
    """Non-RGB image modes are converted before pasting."""

    def test_grayscale_l_mode(self, preview_panel: PreviewPanel) -> None:
        """Mosaic handles L (grayscale) images without color corruption."""
        images = [Image.new("L", (200, 200), color=128)]
        preview_panel.set_multi_preview(_make_image_infos(images))
        assert preview_panel._preview_stack.currentWidget() is preview_panel._mosaic_container

    def test_palette_p_mode(self, preview_panel: PreviewPanel) -> None:
        """Mosaic handles P (palette) images without color corruption."""
        img = Image.new("P", (200, 200))
        img.putpalette([i % 256 for i in range(768)])
        preview_panel.set_multi_preview(_make_image_infos([img]))
        assert preview_panel._preview_stack.currentWidget() is preview_panel._mosaic_container

    def test_cmyk_mode(self, preview_panel: PreviewPanel) -> None:
        """Mosaic handles CMYK images by converting to RGB."""
        images = [Image.new("CMYK", (200, 200), color=(0, 0, 0, 0))]
        preview_panel.set_multi_preview(_make_image_infos(images))
        assert preview_panel._preview_stack.currentWidget() is preview_panel._mosaic_container

    def test_la_mode(self, preview_panel: PreviewPanel) -> None:
        """Mosaic handles LA (grayscale + alpha) images."""
        images = [Image.new("LA", (200, 200), color=(128, 255))]
        preview_panel.set_multi_preview(_make_image_infos(images))
        assert preview_panel._preview_stack.currentWidget() is preview_panel._mosaic_container

    def test_i_mode_32bit(self, preview_panel: PreviewPanel) -> None:
        """Mosaic handles I (32-bit integer) images."""
        images = [Image.new("I", (200, 200), color=1000)]
        preview_panel.set_multi_preview(_make_image_infos(images))
        assert preview_panel._preview_stack.currentWidget() is preview_panel._mosaic_container


class TestTagServiceConfiguration:
    """PreviewPanel accepts an optional tag_service."""

    def test_preview_panel_accepts_tag_service(self, settings_service: SettingsService) -> None:
        """PreviewPanel can be created with a tag_service parameter."""
        service = MagicMock()
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            assert panel._tag_service is service
        finally:
            panel.close()

    def test_preview_panel_works_without_tag_service(self, settings_service: SettingsService) -> None:
        """PreviewPanel works without tag_service (display-only mode)."""
        panel = PreviewPanel(settings_service)
        try:
            assert panel._tag_service is None
        finally:
            panel.close()


class TestSetTags:
    """The set_tags() method renders and stores the provided tags."""

    def test_set_tags_displays_pills(self, settings_service: SettingsService) -> None:
        """The set_tags() method creates tag pill widgets for user tags only."""
        panel = PreviewPanel(settings_service)
        try:
            tags = {
                Tag(id=1, name="landscape", source=TagSource.USER),
                Tag(id=2, name="color:red", source=TagSource.AUTO_COLOR),
            }
            panel.set_tags(tags)
            # Only user tags create pills; auto_color tags update color squares
            assert len(panel._tag_pills) == 1
            assert panel._tag_pills[0].text() == "landscape"
        finally:
            panel.close()

    def test_set_tags_clears_previous_pills(self, settings_service: SettingsService) -> None:
        """The set_tags() method replaces previous pills rather than appending to them."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags({Tag(id=1, name="old_tag", source=TagSource.USER)})
            assert len(panel._tag_pills) == 1

            panel.set_tags(
                {
                    Tag(id=2, name="new_tag1", source=TagSource.USER),
                    Tag(id=3, name="new_tag2", source=TagSource.USER),
                }
            )
            assert len(panel._tag_pills) == 2
            assert panel._tag_pills[0].text() == "new_tag1"
        finally:
            panel.close()

    def test_set_tags_empty_clears_pills(self, settings_service: SettingsService) -> None:
        """The set_tags() method removes all pills when given an empty list."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags({Tag(id=1, name="tag", source=TagSource.USER)})
            assert len(panel._tag_pills) == 1

            panel.set_tags([])
            assert len(panel._tag_pills) == 0
        finally:
            panel.close()

    def test_set_tags_stores_current_tags(self, settings_service: SettingsService) -> None:
        """The set_tags() method stores the tag list for later reference."""
        panel = PreviewPanel(settings_service)
        try:
            tags = {Tag(id=1, name="tag", source=TagSource.USER)}
            panel.set_tags(tags)
            assert panel._current_tags == tags
        finally:
            panel.close()

    def test_set_tags_stores_selected_paths(self, settings_service: SettingsService) -> None:
        """The set_tags() method stores selected_paths when they are provided."""
        panel = PreviewPanel(settings_service)
        try:
            paths = ["/path/to/img1.jpg", "/path/to/img2.jpg"]
            panel.set_tags([], selected_paths=paths)
            assert panel._selected_paths == paths
        finally:
            panel.close()

    def test_set_tags_normalizes_backslash_paths(self, settings_service: SettingsService) -> None:
        """Backslash paths (Windows) are normalized to forward slashes."""
        panel = PreviewPanel(settings_service)
        try:
            paths = ["D:\\Art\\img1.jpg", "D:\\Art\\img2.jpg"]
            panel.set_tags([], selected_paths=paths)
            assert panel._selected_paths == ["D:/Art/img1.jpg", "D:/Art/img2.jpg"]
        finally:
            panel.close()


class TestTagPillPresentation:
    """Tag pills are styled and tagged by role."""

    def test_tag_pill_has_pointing_hand_cursor(self, settings_service: SettingsService) -> None:
        """Tag pills have PointingHandCursor to indicate clickability."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags({Tag(id=1, name="tag", source=TagSource.USER)})
            assert panel._tag_pills[0].cursor().shape() == Qt.CursorShape.PointingHandCursor
        finally:
            panel.close()

    def test_tag_pill_role_primary_for_user_tags(self, settings_service: SettingsService) -> None:
        """User-created tags get 'primary' role."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags({Tag(id=1, name="tag", source=TagSource.USER)})
            assert panel._tag_pills[0].property("tagRole") == "primary"
        finally:
            panel.close()

    def test_tag_pill_role_secondary_for_auto_tags(self, settings_service: SettingsService) -> None:
        """Auto-color tags update color squares instead of creating pills."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags({Tag(id=1, name="red", source=TagSource.AUTO_COLOR)})
            # auto_color tags should NOT create pills
            assert len(panel._tag_pills) == 0
            # The red color square should be at full opacity (active)
            red_btn = panel._color_square_buttons["red"]
            effect = red_btn.graphicsEffect()
            assert isinstance(effect, QGraphicsOpacityEffect)
            assert abs(effect.opacity() - 1.0) < 0.01
        finally:
            panel.close()


class TestClearTagState:
    """The clear() method resets all tag-related state."""

    def test_clear_resets_tag_state(self, settings_service: SettingsService) -> None:
        """The clear() method resets all tag-related state."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags(
                {Tag(id=1, name="tag", source=TagSource.USER)},
                selected_paths=["/path/img.jpg"],
            )
            assert len(panel._current_tags) == 1
            assert len(panel._selected_paths) == 1

            panel.clear()
            assert panel._current_tags == set()
            assert panel._selected_paths == []
            assert panel._cached_file_tags == {}
            assert len(panel._tag_pills) == 0
        finally:
            panel.close()


class TestTagTriState:
    """Tag pills use tri-state opacity for multi-selection."""

    def test_tri_state_full_opacity_when_all_files_have_tag(self, settings_service: SettingsService) -> None:
        """Tag pill has full opacity (no effect) when all selected files have the tag."""
        panel = PreviewPanel(settings_service)
        try:
            paths = ["/img1.jpg", "/img2.jpg"]
            # Both files have tag 1
            panel._cached_file_tags = {
                "/img1.jpg": {Tag(id=1, name="tag", source=TagSource.USER)},
                "/img2.jpg": {Tag(id=1, name="tag", source=TagSource.USER)},
            }
            panel._selected_paths = paths

            pill = panel._create_tag_pill(Tag(id=1, name="tag", source=TagSource.USER))
            # No opacity effect = full opacity
            assert pill.graphicsEffect() is None
        finally:
            panel.close()

    def test_tri_state_half_opacity_when_some_files_have_tag(self, settings_service: SettingsService) -> None:
        """Tag pill has 0.5 opacity effect when only some selected files have the tag."""
        panel = PreviewPanel(settings_service)
        try:
            paths = ["/img1.jpg", "/img2.jpg"]
            # Only first file has tag 1
            panel._cached_file_tags = {
                "/img1.jpg": {Tag(id=1, name="tag", source=TagSource.USER)},
                "/img2.jpg": set(),
            }
            panel._selected_paths = paths

            pill = panel._create_tag_pill(Tag(id=1, name="tag", source=TagSource.USER))
            effect = pill.graphicsEffect()
            assert isinstance(effect, QGraphicsOpacityEffect)
            assert abs(effect.opacity() - 0.5) < 0.01
        finally:
            panel.close()

    def test_tri_state_low_opacity_when_no_files_have_tag(self, settings_service: SettingsService) -> None:
        """Tag pill has 0.3 opacity effect when no selected files have the tag."""
        panel = PreviewPanel(settings_service)
        try:
            paths = ["/img1.jpg", "/img2.jpg"]
            panel._cached_file_tags = {
                "/img1.jpg": set(),
                "/img2.jpg": set(),
            }
            panel._selected_paths = paths

            pill = panel._create_tag_pill(Tag(id=1, name="tag", source=TagSource.USER))
            effect = pill.graphicsEffect()
            assert isinstance(effect, QGraphicsOpacityEffect)
            assert abs(effect.opacity() - 0.3) < 0.01
        finally:
            panel.close()

    def test_tri_state_not_applied_for_single_selection(self, settings_service: SettingsService) -> None:
        """Tri-state opacity is not applied for single file selection."""
        panel = PreviewPanel(settings_service)
        try:
            paths = ["/img1.jpg"]
            panel._cached_file_tags = {"/img1.jpg": {Tag(id=1, name="tag", source=TagSource.USER)}}
            panel._selected_paths = paths

            pill = panel._create_tag_pill(Tag(id=1, name="tag", source=TagSource.USER))
            # No opacity effect for single selection
            assert pill.graphicsEffect() is None
        finally:
            panel.close()


class TestTagPillActions:
    """Clicking a tag pill adds or removes the tag from selected files."""

    def test_tag_pill_clicked_removes_tag_when_all_have_it(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """Clicking a tag pill removes the tag when all selected files have it."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            paths = ["/img1.jpg", "/img2.jpg"]
            panel._selected_paths = paths
            panel._cached_file_tags = {
                "/img1.jpg": {Tag(id=1, name="tag", source=TagSource.USER)},
                "/img2.jpg": {Tag(id=1, name="tag", source=TagSource.USER)},
            }

            panel._on_tag_pill_clicked(Tag(id=1, name="tag", source=TagSource.USER))
            service.remove_tags_from_files.assert_called_once_with(
                paths, {Tag(id=1, name="tag", source=TagSource.USER)}
            )
            service.add_tags_to_files.assert_not_called()
        finally:
            panel.close()

    def test_tag_pill_clicked_adds_tag_when_not_all_have_it(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """Clicking a tag pill adds the tag when not all selected files have it."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            paths = ["/img1.jpg", "/img2.jpg"]
            panel._selected_paths = paths
            panel._cached_file_tags = {
                "/img1.jpg": {Tag(id=1, name="tag", source=TagSource.USER)},
                "/img2.jpg": set(),
            }

            panel._on_tag_pill_clicked(Tag(id=1, name="tag", source=TagSource.USER))
            service.add_tags_to_files.assert_called_once_with(paths, {Tag(id=1, name="tag", source=TagSource.USER)})
            service.remove_tags_from_files.assert_not_called()
        finally:
            panel.close()

    def test_tag_pill_clicked_noop_without_selection(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """Clicking a tag pill does nothing when no files are selected."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            panel._selected_paths = []
            panel._on_tag_pill_clicked(Tag(id=1, name="tag", source=TagSource.USER))
            service.add_tags_to_files.assert_not_called()
            service.remove_tags_from_files.assert_not_called()
        finally:
            panel.close()

    def test_tag_pill_clicked_noop_without_tag_service(self, qapp: Any) -> None:
        """Clicking a tag pill does nothing when no tag_service is set."""
        panel = PreviewPanel(settings_service)  # No tag_service
        try:
            panel._selected_paths = ["/img1.jpg"]
            # Should not raise
            panel._on_tag_pill_clicked(Tag(id=1, name="tag", source=TagSource.USER))
        finally:
            panel.close()


class TestInlineTagInput:
    """Inline tag input creates tags and restores the add button."""

    def test_inline_tag_input_creation(self, mock_tag_service: Any, settings_service: SettingsService) -> None:
        """The _show_inline_tag_input() method creates a QLineEdit and hides the add button."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            panel._selected_paths = ["/img1.jpg"]
            assert not panel._add_tag_btn.isHidden()

            panel._show_inline_tag_input()
            assert panel._add_tag_btn.isHidden()
            assert panel._tag_input is not None
            assert isinstance(panel._tag_input, QLineEdit)
            assert panel._tag_input.placeholderText() == "Tag name..."
        finally:
            if panel._tag_input is not None:
                panel._tag_input.deleteLater()
            panel.close()

    def test_inline_tag_input_submitted_creates_tag(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """Submitting inline input creates the tag and adds it to files."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            panel._selected_paths = ["/img1.jpg"]
            panel._show_inline_tag_input()
            panel._tag_input.setText("new_tag")

            panel._on_tag_input_submitted()
            service.add_tags_to_files_by_name.assert_called_once_with(["/img1.jpg"], ["new_tag"])
        finally:
            if panel._tag_input is not None:
                panel._tag_input.deleteLater()
            panel.close()

    def test_inline_tag_input_finished_restores_button(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """Finishing inline input removes the input and shows the add button."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            panel._selected_paths = ["/img1.jpg"]
            panel._show_inline_tag_input()
            assert panel._add_tag_btn.isHidden()

            panel._on_tag_input_finished()
            assert not panel._add_tag_btn.isHidden()
            assert panel._tag_input is None
        finally:
            panel.close()

    def test_inline_tag_input_empty_does_not_create_tag(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """Submitting empty inline input does not create a tag."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            panel._selected_paths = ["/img1.jpg"]
            panel._show_inline_tag_input()
            panel._tag_input.setText("")

            panel._on_tag_input_submitted()
            service.add_tags_to_files.assert_not_called()
        finally:
            if panel._tag_input is not None:
                panel._tag_input.deleteLater()
            panel.close()


class TestGetUnionTags:
    """The get_union_tags() method combines tags across selected files."""

    def test_get_union_tags_combines_tags_from_multiple_files(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """The get_union_tags() method returns the union of tags from all paths."""
        service = mock_tag_service
        service.get_tags_for_files.side_effect = lambda paths: {
            ("/img1.jpg", "/img2.jpg"): {
                "/img1.jpg": {Tag(id=1, name="tag", source=TagSource.USER)},
                "/img2.jpg": {
                    Tag(id=1, name="tag", source=TagSource.USER),
                    Tag(id=2, name="tag2", source=TagSource.USER),
                },
            }
        }[tuple(paths)]

        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            union = panel.get_union_tags(["/img1.jpg", "/img2.jpg"])
            assert len(union) == 2
            tag_ids = {t.get_id() for t in union}
            assert tag_ids == {1, 2}
        finally:
            panel.close()

    def test_get_union_tags_without_tag_service_returns_empty(self, settings_service: SettingsService) -> None:
        """The get_union_tags() method returns an empty list when no tag_service is set."""
        panel = PreviewPanel(settings_service)
        try:
            union = panel.get_union_tags(["/img1.jpg"])
            assert union == set()
        finally:
            panel.close()


class TestTagSignals:
    """Tag changes emit signals when they occur externally."""

    def test_tags_changed_signal_emitted_on_external_change(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """The tags_changed signal is emitted when external tags change."""
        service = mock_tag_service
        service.get_tags_for_file.return_value = []

        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            signal_received = []
            panel.tags_changed.connect(lambda: signal_received.append(True))

            panel._selected_paths = ["/img1.jpg"]
            panel._on_external_tags_changed()
            assert len(signal_received) == 1
        finally:
            panel.close()


class TestTagsContainerLayout:
    """The tags container keeps visible height when empty and after updates."""

    def test_tags_container_has_minimum_height_when_empty(self, settings_service: SettingsService) -> None:
        """Tags container has a minimum height even when no tags are present."""
        panel = PreviewPanel(settings_service)
        try:
            assert panel._tags_container.minimumHeight() > 0, (
                "Tags container should have a minimum height to remain visible when empty"
            )
        finally:
            panel.close()

    def test_tags_container_nonzero_height_after_set_tags(self, settings_service: SettingsService) -> None:
        """After set_tags() with tags, the tags container has non-zero height."""
        panel = PreviewPanel(settings_service)
        try:
            panel.show()
            tags = {
                Tag(id=1, name="tag", source=TagSource.USER),
                Tag(id=2, name="tag", source=TagSource.USER),
            }
            panel.set_tags(tags)

            # The container must have non-zero height after tags are added
            assert panel._tags_container.height() > 0, "Tags container should have non-zero height after set_tags()"
        finally:
            panel.close()

    def test_tag_pills_have_nonzero_size_after_set_tags(self, settings_service: SettingsService) -> None:
        """Tag pills have non-zero size after set_tags()."""
        panel = PreviewPanel(settings_service)
        try:
            panel.show()
            tags = {
                Tag(id=1, name="tag", source=TagSource.USER),
                Tag(id=2, name="tag", source=TagSource.USER),
            }
            panel.set_tags(tags)

            for i, pill in enumerate(panel._tag_pills):
                assert pill.width() > 0, f"Pill {i} ('{pill.text()}') has zero width"
                assert pill.height() > 0, f"Pill {i} ('{pill.text()}') has zero height"
        finally:
            panel.close()

    def test_flow_layout_minimum_size_hint_with_items(self, settings_service: SettingsService) -> None:
        """FlowLayout is used for tag pills and items are added correctly."""
        panel = PreviewPanel(settings_service)
        try:
            panel.show()
            tags = {
                Tag(id=1, name="tag", source=TagSource.USER),
            }
            panel.set_tags(tags)

            assert isinstance(panel._tags_flow, FlowLayout)
            assert panel._tags_flow.count() > 0, "FlowLayout should contain items after set_tags()"
        finally:
            panel.close()

    def test_flow_layout_minimum_size_hint_empty_returns_nonzero_height(
        self, settings_service: SettingsService
    ) -> None:
        """Tags container has nonzero minimum height when FlowLayout is empty."""
        panel = PreviewPanel(settings_service)
        try:
            assert panel._tags_container.minimumHeight() > 0, (
                f"Tags container minimumHeight should be > 0 even when empty, "
                f"got {panel._tags_container.minimumHeight()}"
            )
        finally:
            panel.close()

    def test_tags_container_geometry_updates_on_clear(self, settings_service: SettingsService) -> None:
        """Tags container geometry updates properly after clearing tags."""
        panel = PreviewPanel(settings_service)
        try:
            panel.show()
            # Add tags
            panel.set_tags(
                {
                    Tag(id=1, name="tag", source=TagSource.USER),
                    Tag(id=2, name="tag", source=TagSource.USER),
                }
            )
            assert len(panel._tag_pills) == 2

            # Clear tags
            panel.set_tags([])
            assert len(panel._tag_pills) == 0

            # Container should still have its minimum height (not collapsed)
            assert panel._tags_container.minimumHeight() > 0
        finally:
            panel.close()


class TestAddTagButton:
    """The +Add tag button guards empty selection and does not stretch."""

    def test_add_button_disabled_without_selection(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """Add tag button does nothing when no files are selected."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            panel._selected_paths = []
            # Should not raise or show menu
            panel._on_add_tag_clicked()
            service.get_all_tags.assert_not_called()
        finally:
            panel.close()

    def test_add_tag_button_has_maximum_size_policy(self, settings_service: SettingsService) -> None:
        """The '+ add' tag button uses Maximum size policy so it doesn't stretch."""
        panel = PreviewPanel(settings_service)
        try:
            policy = panel._add_tag_btn.sizePolicy()
            assert policy.horizontalPolicy() == QSizePolicy.Policy.Maximum
            assert policy.verticalPolicy() == QSizePolicy.Policy.Fixed
        finally:
            panel.close()


class TestColorSquares:
    """Color squares reflect auto-color tag state with opacity."""

    def test_color_squares_container_exists(self, settings_service: SettingsService) -> None:
        """Preview panel has a color squares container with 10 buttons."""
        panel = PreviewPanel(settings_service)
        try:
            assert hasattr(panel, "_color_squares_container")
            assert len(panel._color_square_buttons) == len(BUCKET_COLORS)
            for name in BUCKET_COLORS:
                assert name in panel._color_square_buttons
        finally:
            panel.close()

    def test_color_squares_all_inactive_by_default(self, settings_service: SettingsService) -> None:
        """All color squares start at low opacity (0.3) when no tags are set."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags([])
            for name, btn in panel._color_square_buttons.items():
                effect = btn.graphicsEffect()
                assert isinstance(effect, QGraphicsOpacityEffect), f"Square '{name}' has no opacity effect"
                assert abs(effect.opacity() - 0.3) < 0.01, (
                    f"Square '{name}' opacity is {effect.opacity()}, expected 0.3"
                )
        finally:
            panel.close()

    def test_color_squares_active_when_tag_present(self, settings_service: SettingsService) -> None:
        """Color square is at full opacity when its tag is present on the file."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags({Tag(id=1, name="red", source=TagSource.AUTO_COLOR)})
            red_btn = panel._color_square_buttons["red"]
            effect = red_btn.graphicsEffect()
            assert isinstance(effect, QGraphicsOpacityEffect)
            assert abs(effect.opacity() - 1.0) < 0.01
            # Other squares should still be inactive
            blue_btn = panel._color_square_buttons["blue"]
            effect_blue = blue_btn.graphicsEffect()
            assert isinstance(effect_blue, QGraphicsOpacityEffect)
            assert abs(effect_blue.opacity() - 0.3) < 0.01
        finally:
            panel.close()

    def test_color_squares_tri_state_multi_selection(self, settings_service: SettingsService) -> None:
        """Color squares use tri-state opacity for multi-selection."""
        panel = PreviewPanel(settings_service)
        try:
            paths = ["/img1.jpg", "/img2.jpg"]
            panel._selected_paths = paths
            # Both files have color:red (tag id=1)
            panel._cached_file_tags = {
                "/img1.jpg": {Tag(id=1, name="red", source=TagSource.AUTO_COLOR)},
                "/img2.jpg": {Tag(id=1, name="red", source=TagSource.AUTO_COLOR)},
            }
            panel._current_tags = {Tag(id=1, name="red", source=TagSource.AUTO_COLOR)}

            # Call _update_color_squares directly (as set_tags would)
            panel._update_color_squares({"red"})
            red_btn = panel._color_square_buttons["red"]
            effect = red_btn.graphicsEffect()
            assert isinstance(effect, QGraphicsOpacityEffect)
            assert abs(effect.opacity() - 1.0) < 0.01  # all have it
        finally:
            panel.close()

    def test_color_squares_partial_opacity_multi_selection(self, settings_service: SettingsService) -> None:
        """Color square has 0.5 opacity when only some files have the color."""
        panel = PreviewPanel(settings_service)
        try:
            paths = ["/img1.jpg", "/img2.jpg"]
            panel._selected_paths = paths
            # Only first file has color:red (tag id=1)
            panel._cached_file_tags = {
                "/img1.jpg": {Tag(id=1, name="red", source=TagSource.AUTO_COLOR)},
                "/img2.jpg": set(),
            }
            panel._current_tags = {Tag(id=1, name="red", source=TagSource.AUTO_COLOR)}

            panel._update_color_squares({"red"})
            red_btn = panel._color_square_buttons["red"]
            effect = red_btn.graphicsEffect()
            assert isinstance(effect, QGraphicsOpacityEffect)
            assert abs(effect.opacity() - 0.5) < 0.01  # some have it
        finally:
            panel.close()

    def test_color_square_button_has_pointing_hand_cursor(self, settings_service: SettingsService) -> None:
        """Color square buttons have PointingHandCursor."""
        panel = PreviewPanel(settings_service)
        try:
            for name, btn in panel._color_square_buttons.items():
                assert btn.cursor().shape() == Qt.CursorShape.PointingHandCursor, (
                    f"Color square '{name}' should have PointingHandCursor"
                )
        finally:
            panel.close()

    def test_color_square_button_has_color_square_property(self, settings_service: SettingsService) -> None:
        """Color square buttons have the 'colorSquare' property set for QSS targeting."""
        panel = PreviewPanel(settings_service)
        try:
            for name, btn in panel._color_square_buttons.items():
                assert btn.property("colorSquare") is True, (
                    f"Color square '{name}' should have colorSquare=True property"
                )
        finally:
            panel.close()

    def test_color_square_clicked_adds_tag(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """Clicking a color square adds the color tag when not all files have it."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            paths = ["/img1.jpg"]
            panel._selected_paths = paths
            panel._cached_file_tags = {"/img1.jpg": set()}
            panel._current_tags = set()

            panel._on_color_square_clicked("red")
            service.add_tags_to_files_by_name.assert_called_once_with(paths, ["red"], source=TagSource.AUTO_COLOR)
            service.remove_tags_from_files.assert_not_called()
        finally:
            panel.close()

    def test_color_square_clicked_removes_tag(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """Clicking a color square removes the color tag when all files have it."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            paths = ["/img1.jpg"]
            panel._selected_paths = paths
            panel._cached_file_tags = {"/img1.jpg": {Tag(id=1, name="red", source=TagSource.AUTO_COLOR)}}
            panel._current_tags = {Tag(id=1, name="red", source=TagSource.AUTO_COLOR)}

            panel._on_color_square_clicked("red")
            service.remove_tags_from_files.assert_called_once_with(
                paths, {Tag(id=1, name="red", source=TagSource.AUTO_COLOR)}
            )
            service.add_tags_to_files.assert_not_called()
        finally:
            panel.close()

    def test_clear_resets_color_squares(self, settings_service: SettingsService) -> None:
        """The clear() method resets all color squares to inactive opacity."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags({Tag(id=1, name="red", source=TagSource.AUTO_COLOR)})
            red_btn = panel._color_square_buttons["red"]
            effect = red_btn.graphicsEffect()
            assert isinstance(effect, QGraphicsOpacityEffect)
            assert abs(effect.opacity() - 1.0) < 0.01

            panel.clear()
            effect = red_btn.graphicsEffect()
            assert isinstance(effect, QGraphicsOpacityEffect)
            assert abs(effect.opacity() - 0.3) < 0.01
        finally:
            panel.close()


class TestTagPillRemoveButton:
    """Tag pill remove buttons delete tags from selected files."""

    def test_tag_pill_has_remove_button(self, settings_service: SettingsService) -> None:
        """Tag pill widgets contain a remove (×) button."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags({Tag(id=1, name="tag", source=TagSource.USER)})
            pill = panel._tag_pills[0]
            assert hasattr(pill, "_remove_btn")
            assert pill._remove_btn.text() == "x"
        finally:
            panel.close()

    def test_tag_pill_remove_button_hidden_by_default(self, settings_service: SettingsService) -> None:
        """The remove (×) button is hidden by default."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags({Tag(id=1, name="tag", source=TagSource.USER)})
            pill = panel._tag_pills[0]
            assert pill._remove_btn.isHidden()
        finally:
            panel.close()

    def test_tag_pill_remove_button_has_object_name(self, settings_service: SettingsService) -> None:
        """The remove button has 'tagPillRemoveBtn' objectName for QSS targeting."""
        panel = PreviewPanel(settings_service)
        try:
            panel.set_tags({Tag(id=1, name="tag", source=TagSource.USER)})
            pill = panel._tag_pills[0]
            assert pill._remove_btn.objectName() == "tagPillRemoveBtn"
        finally:
            panel.close()

    def test_tag_pill_remove_clicked_calls_service(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """Clicking the × button removes the tag from files."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            paths = ["/img1.jpg"]
            panel._selected_paths = paths
            panel._cached_file_tags = {"/img1.jpg": {Tag(id=1, name="tag", source=TagSource.USER)}}

            panel._on_tag_remove_clicked(Tag(id=1, name="tag", source=TagSource.USER))
            service.remove_tags_from_files.assert_called_once_with(
                paths, {Tag(id=1, name="tag", source=TagSource.USER)}
            )
        finally:
            panel.close()

    def test_tag_pill_remove_noop_without_selection(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """Clicking the × button does nothing when no files are selected."""
        service = mock_tag_service
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            panel._selected_paths = []
            panel._on_tag_remove_clicked(Tag(id=1, name="tag", source=TagSource.USER))
            service.remove_tags_from_files.assert_not_called()
        finally:
            panel.close()


class TestAddTagDropdown:
    """The +Add dropdown excludes color tags from the list."""

    def test_add_tag_dropdown_filters_color_tags(
        self,
        mock_tag_service: Any,
        settings_service: SettingsService,
    ) -> None:
        """The +Add dropdown excludes color: tags from the list."""
        service = mock_tag_service
        service.get_all_tags.return_value = [
            Tag(id=1, name="landscape", source=TagSource.USER, usage_count=5),
            Tag(id=2, name="color:red", source=TagSource.AUTO_COLOR, usage_count=3),
            Tag(id=3, name="portrait", source=TagSource.USER, usage_count=2),
            Tag(id=4, name="color:blue", source=TagSource.AUTO_COLOR, usage_count=1),
        ]
        panel = PreviewPanel(settings_service=settings_service, tag_service=service)
        try:
            panel._selected_paths = ["/img1.jpg"]
            panel._current_tags = set()

            # Patch QMenu.exec to return a mock action with data=None (no selection)
            added_actions: list[str] = []

            with patch("tarragon.widgets.preview_panel.QMenu") as mock_qmenu:
                mock_menu = MagicMock()
                mock_qmenu.return_value = mock_menu

                def fake_add_action(name):
                    added_actions.append(name)
                    action = MagicMock()
                    return action

                mock_menu.addAction.side_effect = fake_add_action
                mock_action = MagicMock()
                mock_action.data.return_value = None  # simulates no selection
                mock_menu.exec.return_value = mock_action

                panel._on_add_tag_clicked()

            # Only custom (non-color) tags should be in the menu
            assert "landscape" in added_actions
            assert "portrait" in added_actions
            assert "color:red" not in added_actions
            assert "color:blue" not in added_actions
        finally:
            panel.close()
