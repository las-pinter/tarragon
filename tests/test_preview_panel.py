"""Tests for PreviewPanel widget."""

from __future__ import annotations

import gc
from pathlib import Path
from typing import Any
from unittest.mock import patch

import pytest
from PIL import Image
from PySide6.QtCore import QSize
from PySide6.QtGui import QResizeEvent
from PySide6.QtWidgets import QLabel, QSizePolicy, QVBoxLayout, QWidget
from tarragon.widgets.preview_panel import PreviewPanel


@pytest.fixture
def sample_image() -> Image.Image:
    """Create a sample PIL Image for testing."""
    return Image.new("RGB", (800, 600), color="red")


@pytest.fixture
def sample_rgba_image() -> Image.Image:
    """Create a sample RGBA PIL Image for testing."""
    return Image.new("RGBA", (1024, 768), color=(0, 255, 0, 128))


# ── Instantiation Tests ──────────────────────────────────────────────


def test_preview_panel_is_qwidget() -> None:
    """PreviewPanel is a QWidget subclass."""
    assert issubclass(PreviewPanel, QWidget)


def test_preview_panel_instantiation(qapp: Any) -> None:  # noqa: ARG001
    """PreviewPanel can be created without errors."""
    panel = PreviewPanel()
    try:
        assert panel is not None
        assert isinstance(panel, QWidget)
    finally:
        panel.close()


def test_preview_panel_has_layout(qapp: Any) -> None:  # noqa: ARG001
    """PreviewPanel has a QVBoxLayout."""
    panel = PreviewPanel()
    try:
        layout = panel.layout()
        assert isinstance(layout, QVBoxLayout)
    finally:
        panel.close()


def test_preview_panel_has_image_label(qapp: Any) -> None:  # noqa: ARG001
    """PreviewPanel has an image QLabel."""
    panel = PreviewPanel()
    try:
        assert hasattr(panel, "_image_label")
        assert isinstance(panel._image_label, QLabel)
    finally:
        panel.close()


def test_image_label_size_policy_is_ignored(qapp: Any) -> None:  # noqa: ARG001
    """Image label size policy is Ignored/Ignored to prevent resize loop.

    When setPixmap() is called, QLabel's default sizeHint() returns the
    pixmap's size, causing the layout to expand the label. That triggers
    resizeEvent → _update_display → setPixmap(larger) → sizeHint grows →
    layout expands → infinite loop until native image size is reached.

    Setting Ignored/Ignored tells the layout to ignore sizeHint() and
    size the label based on available space instead.
    """
    panel = PreviewPanel()
    try:
        policy = panel._image_label.sizePolicy()
        assert policy.horizontalPolicy() == QSizePolicy.Policy.Ignored
        assert policy.verticalPolicy() == QSizePolicy.Policy.Ignored
    finally:
        panel.close()


def test_preview_panel_initial_state(qapp: Any) -> None:  # noqa: ARG001
    """PreviewPanel starts wiv no image an' empty metadata."""
    panel = PreviewPanel()
    try:
        assert panel._current_image is None
        assert panel._current_path is None
        assert panel._image_label.text() == "No preview"
        assert panel._filename_label.text() == ""
        assert panel._dimensions_label.text() == ""
    finally:
        panel.close()


# ── set_image Tests ──────────────────────────────────────────────────


def test_set_image_rgb(qapp: Any, sample_image: Any) -> None:  # noqa: ARG001
    """set_image displays an RGB image."""
    panel = PreviewPanel()
    try:
        panel.set_image(sample_image)
        assert panel._current_image is not None
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 800 × 600"
    finally:
        panel.close()


def test_set_image_rgba(qapp: Any, sample_rgba_image: Any) -> None:  # noqa: ARG001
    """set_image displays an RGBA image."""
    panel = PreviewPanel()
    try:
        panel.set_image(sample_rgba_image)
        assert panel._current_image is not None
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 1024 × 768"
    finally:
        panel.close()


def test_set_image_wiv_path(qapp: Any, sample_image: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """set_image displays metadata when path is provided."""
    # Create a dummy file
    test_file = tmp_path / "test_image.jpg"
    test_file.write_bytes(b"fake image data")

    panel = PreviewPanel()
    try:
        panel.set_image(sample_image, path=test_file)
        assert panel._current_path == test_file
        assert panel._filename_label.text() == "test_image.jpg"
        assert "Size:" in panel._size_label.text()
        assert "Format:" in panel._format_label.text()
    finally:
        panel.close()


def test_set_image_wivout_path(qapp: Any, sample_image: Any) -> None:  # noqa: ARG001
    """set_image works without a path (shows 'Unknown file')."""
    panel = PreviewPanel()
    try:
        panel.set_image(sample_image)
        assert panel._filename_label.text() == "Unknown file"
        assert panel._size_label.text() == "Size: Unknown"
    finally:
        panel.close()


# ── clear Tests ──────────────────────────────────────────────────────


def test_clear_resets_state(qapp: Any, sample_image: Any) -> None:  # noqa: ARG001
    """clear resets da panel to initial state."""
    panel = PreviewPanel()
    try:
        panel.set_image(sample_image)
        assert panel._current_image is not None

        panel.clear()
        assert panel._current_image is None
        assert panel._current_path is None
        assert panel._image_label.text() == "No preview"
        assert panel._filename_label.text() == ""
    finally:
        panel.close()


# ── Metadata Tests ───────────────────────────────────────────────────


def test_format_size_bytes(qapp: Any) -> None:  # noqa: ARG001
    """_format_size formats bytes correctly."""
    assert PreviewPanel._format_size(500) == "500 B"


def test_format_size_kilobytes(qapp: Any) -> None:  # noqa: ARG001
    """_format_size formats kilobytes correctly."""
    assert PreviewPanel._format_size(1536) == "1.5 KB"


def test_format_size_megabytes(qapp: Any) -> None:  # noqa: ARG001
    """_format_size formats megabytes correctly."""
    assert PreviewPanel._format_size(2 * 1024 * 1024) == "2.0 MB"


def test_format_size_gigabytes(qapp: Any) -> None:  # noqa: ARG001
    """_format_size formats gigabytes correctly."""
    assert PreviewPanel._format_size(3 * 1024 * 1024 * 1024) == "3.0 GB"


# ── PIL to QImage Conversion Tests ───────────────────────────────────


def test_pil_to_qimage_rgb(qapp: Any, sample_image: Any) -> None:  # noqa: ARG001
    """_pil_to_qimage converts RGB image correctly."""
    qimage = PreviewPanel._pil_to_qimage(sample_image)
    assert qimage.width() == 800
    assert qimage.height() == 600


def test_pil_to_qimage_rgba(qapp: Any, sample_rgba_image: Any) -> None:  # noqa: ARG001
    """_pil_to_qimage converts RGBA image correctly."""
    qimage = PreviewPanel._pil_to_qimage(sample_rgba_image)
    assert qimage.width() == 1024
    assert qimage.height() == 768


def test_pil_to_qimage_grayscale(qapp: Any) -> None:  # noqa: ARG001
    """_pil_to_qimage converts grayscale image to RGB."""
    gray_image = Image.new("L", (400, 300), color=128)
    qimage = PreviewPanel._pil_to_qimage(gray_image)
    assert qimage.width() == 400
    assert qimage.height() == 300


# ── Edge Case: Boundary Image Sizes ─────────────────────────────────


def test_set_image_1x1_pixel(qapp: Any) -> None:  # noqa: ARG001
    """set_image handles a 1x1 pixel image without crashing."""
    panel = PreviewPanel()
    try:
        tiny = Image.new("RGB", (1, 1), color="blue")
        panel.set_image(tiny)
        assert panel._current_image is not None
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 1 × 1"
    finally:
        panel.close()


def test_set_image_very_wide_panorama(qapp: Any) -> None:  # noqa: ARG001
    """set_image handles extreme aspect ratio (wide panorama)."""
    panel = PreviewPanel()
    try:
        wide = Image.new("RGB", (4000, 100), color="green")
        panel.set_image(wide)
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 4000 × 100"
    finally:
        panel.close()


def test_set_image_very_tall(qapp: Any) -> None:  # noqa: ARG001
    """set_image handles extreme aspect ratio (tall/narrow image)."""
    panel = PreviewPanel()
    try:
        tall = Image.new("RGB", (50, 5000), color="yellow")
        panel.set_image(tall)
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 50 × 5000"
    finally:
        panel.close()


def test_set_image_large_25mp(qapp: Any) -> None:  # noqa: ARG001
    """set_image handles a large 25-megapixel image (5000x5000)."""
    panel = PreviewPanel()
    try:
        large = Image.new("RGB", (5000, 5000), color="white")
        panel.set_image(large)
        assert panel._current_image is not None
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 5000 × 5000"
    finally:
        panel.close()


# ── Edge Case: PIL Mode Conversions ─────────────────────────────────


def test_set_image_cmyk_mode(qapp: Any) -> None:  # noqa: ARG001
    """set_image converts CMYK image to RGB for display."""
    panel = PreviewPanel()
    try:
        cmyk = Image.new("CMYK", (200, 200), color=(0, 0, 0, 0))
        panel.set_image(cmyk)
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 200 × 200"
    finally:
        panel.close()


def test_set_image_palette_mode(qapp: Any) -> None:  # noqa: ARG001
    """set_image converts palette (P) mode image for display."""
    panel = PreviewPanel()
    try:
        palette_img = Image.new("P", (300, 200))
        palette_img.putpalette([i % 256 for i in range(768)])
        panel.set_image(palette_img)
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


def test_set_image_1bit_mode(qapp: Any) -> None:  # noqa: ARG001
    """set_image converts 1-bit binary image for display."""
    panel = PreviewPanel()
    try:
        binary = Image.new("1", (100, 100), color=1)
        panel.set_image(binary)
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


def test_set_image_la_mode(qapp: Any) -> None:  # noqa: ARG001
    """set_image converts LA (grayscale + alpha) image for display."""
    panel = PreviewPanel()
    try:
        la_img = Image.new("LA", (200, 200), color=(128, 255))
        panel.set_image(la_img)
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


def test_set_image_i_mode_32bit(qapp: Any) -> None:  # noqa: ARG001
    """set_image converts I (32-bit integer) mode image for display."""
    panel = PreviewPanel()
    try:
        i_img = Image.new("I", (200, 200), color=1000)
        panel.set_image(i_img)
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


def test_set_image_f_mode_float(qapp: Any) -> None:  # noqa: ARG001
    """set_image converts F (float) mode image for display."""
    panel = PreviewPanel()
    try:
        f_img = Image.new("F", (200, 200), color=1.5)
        panel.set_image(f_img)
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


# ── Edge Case: Animated GIF ─────────────────────────────────────────


def test_set_image_animated_gif_shows_first_frame(qapp: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """set_image displays first frame of an animated GIF."""
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

    panel = PreviewPanel()
    try:
        gif_image = Image.open(gif_path)
        panel.set_image(gif_image, path=gif_path)
        # Should show first frame dimensions
        assert panel._dimensions_label.text() == "Dimensions: 100 × 100"
        assert panel._image_label.pixmap() is not None
        gif_image.close()
    finally:
        panel.close()


# ── Edge Case: None / Invalid Inputs ────────────────────────────────


def test_set_image_wiv_none_raises_typeerror(qapp: Any) -> None:  # noqa: ARG001
    """set_image wiv None raises TypeError (explicit guard against None)."""
    panel = PreviewPanel()
    try:
        with pytest.raises(TypeError, match="image must be a PIL Image, not None"):
            panel.set_image(None)
    finally:
        panel.close()


# ── Edge Case: resizeEvent ──────────────────────────────────────────


def test_resize_event_wiv_no_image_does_not_crash(qapp: Any) -> None:  # noqa: ARG001
    """resizeEvent when no image is set does not crash."""
    panel = PreviewPanel()
    try:
        panel.show()
        event = QResizeEvent(QSize(400, 300), QSize(200, 200))
        panel.resizeEvent(event)
        # Should not crash, image label should still say "No preview"
        assert panel._current_image is None
    finally:
        panel.close()


def test_resize_event_wiv_image_reapplies(qapp: Any, sample_image: Any) -> None:  # noqa: ARG001
    """resizeEvent re-scales da image when panel is resized."""
    panel = PreviewPanel()
    try:
        panel.show()
        panel.set_image(sample_image)
        assert panel._current_image is not None

        # Simulate resize
        event = QResizeEvent(QSize(600, 400), QSize(200, 200))
        panel.resizeEvent(event)

        # Image should still be set after resize
        assert panel._current_image is not None
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


def test_set_image_caches_pixmap(qapp: Any, sample_image: Any) -> None:  # noqa: ARG001
    """set_image caches da full-resolution pixmap for fast resizing."""
    panel = PreviewPanel()
    try:
        assert panel._cached_pixmap is None
        panel.set_image(sample_image)
        assert panel._cached_pixmap is not None
        assert not panel._cached_pixmap.isNull()
    finally:
        panel.close()


def test_resize_event_does_not_reconvert(qapp: Any, sample_image: Any) -> None:  # noqa: ARG001
    """resizeEvent re-scales da cached pixmap wivout re-converting from PIL."""
    panel = PreviewPanel()
    try:
        panel.show()
        panel.set_image(sample_image)
        cached = panel._cached_pixmap
        assert cached is not None

        # Spy on _pil_to_qimage — it should NOT be called during resize
        with patch.object(PreviewPanel, "_pil_to_qimage", wraps=PreviewPanel._pil_to_qimage) as spy:
            event = QResizeEvent(QSize(600, 400), QSize(200, 200))
            panel.resizeEvent(event)
            assert spy.call_count == 0

        # Cached pixmap should be da same object (not re-created)
        assert panel._cached_pixmap is cached
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


def test_clear_resets_cached_pixmap(qapp: Any, sample_image: Any) -> None:  # noqa: ARG001
    """clear resets da cached pixmap to None."""
    panel = PreviewPanel()
    try:
        panel.set_image(sample_image)
        assert panel._cached_pixmap is not None
        panel.clear()
        assert panel._cached_pixmap is None
    finally:
        panel.close()


# ── Edge Case: Multiple Rapid set_image Calls ───────────────────────


def test_multiple_set_image_calls_last_one_wins(qapp: Any) -> None:  # noqa: ARG001
    """Multiple rapid set_image calls — last image is displayed."""
    panel = PreviewPanel()
    try:
        img1 = Image.new("RGB", (100, 100), color="red")
        img2 = Image.new("RGB", (200, 200), color="green")
        img3 = Image.new("RGB", (300, 300), color="blue")

        panel.set_image(img1)
        panel.set_image(img2)
        panel.set_image(img3)

        assert panel._current_image is not None
        assert panel._dimensions_label.text() == "Dimensions: 300 × 300"
    finally:
        panel.close()


def test_set_image_after_clear(qapp: Any, sample_image: Any) -> None:  # noqa: ARG001
    """set_image works correctly after clear."""
    panel = PreviewPanel()
    try:
        panel.set_image(sample_image)
        panel.clear()
        assert panel._current_image is None

        new_img = Image.new("RGB", (50, 50), color="cyan")
        panel.set_image(new_img)
        assert panel._current_image is not None
        assert panel._dimensions_label.text() == "Dimensions: 50 × 50"
    finally:
        panel.close()


# ── Edge Case: Path Errors ──────────────────────────────────────────


def test_set_image_wiv_nonexistent_path_shows_unknown_size(qapp: Any, sample_image: Any) -> None:  # noqa: ARG001
    """set_image wiv nonexistent path shows 'Size: Unknown'."""
    panel = PreviewPanel()
    try:
        fake_path = Path("/nonexistent/path/to/image.jpg")
        panel.set_image(sample_image, path=fake_path)
        assert panel._filename_label.text() == "image.jpg"
        assert panel._size_label.text() == "Size: Unknown"
    finally:
        panel.close()


def test_set_image_wiv_path_stat_raises_oserror(qapp: Any, sample_image: Any) -> None:  # noqa: ARG001
    """set_image handles OSError from path.stat() gracefully."""
    panel = PreviewPanel()
    try:
        real_path = Path("/tmp/some_image.png")
        with patch.object(Path, "stat", side_effect=OSError("Permission denied")):
            panel.set_image(sample_image, path=real_path)
        assert panel._size_label.text() == "Size: Unknown"
        assert panel._filename_label.text() == "some_image.png"
    finally:
        panel.close()


# ── Edge Case: Unicode and Long Filenames ───────────────────────────


def test_set_image_wiv_unicode_filename(qapp: Any, sample_image: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """set_image handles Unicode characters in filename."""
    panel = PreviewPanel()
    try:
        unicode_file = tmp_path / "тест_画像_🖼️.png"
        unicode_file.write_bytes(b"fake data")
        panel.set_image(sample_image, path=unicode_file)
        assert panel._filename_label.text() == "тест_画像_🖼️.png"
    finally:
        panel.close()


def test_set_image_wiv_very_long_filename(qapp: Any, sample_image: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """set_image handles very long filename without crashing."""
    panel = PreviewPanel()
    try:
        long_name = "a" * 200 + ".png"
        long_file = tmp_path / long_name
        long_file.write_bytes(b"fake data")
        panel.set_image(sample_image, path=long_file)
        assert panel._filename_label.text() == long_name
    finally:
        panel.close()


# ── Edge Case: _format_size Boundaries ──────────────────────────────


def test_format_size_zero_bytes(qapp: Any) -> None:  # noqa: ARG001
    """_format_size handles 0 bytes."""
    assert PreviewPanel._format_size(0) == "0 B"


def test_format_size_exactly_1024_bytes(qapp: Any) -> None:  # noqa: ARG001
    """_format_size handles exactly 1024 bytes (1.0 KB)."""
    assert PreviewPanel._format_size(1024) == "1.0 KB"


def test_format_size_terabytes(qapp: Any) -> None:  # noqa: ARG001
    """_format_size handles terabyte range."""
    assert PreviewPanel._format_size(5 * 1024**4) == "5.0 TB"


def test_format_size_1_byte(qapp: Any) -> None:  # noqa: ARG001
    """_format_size handles exactly 1 byte."""
    assert PreviewPanel._format_size(1) == "1 B"


# ── Edge Case: Format Fallback ──────────────────────────────────────


def test_format_fallback_to_path_extension(qapp: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """Format falls back to path extension when image.format is None."""
    panel = PreviewPanel()
    try:
        test_file = tmp_path / "photo.psd"
        test_file.write_bytes(b"fake psd data")
        # Create image wiv no format set (as happens wiv Image.new)
        img = Image.new("RGB", (100, 100))
        assert img.format is None  # confirm precondition
        panel.set_image(img, path=test_file)
        assert panel._format_label.text() == "Format: PSD"
    finally:
        panel.close()


def test_format_shows_unknown_when_no_format_and_no_path(qapp: Any) -> None:  # noqa: ARG001
    """Format shows 'Unknown' when image has no format and no path given."""
    panel = PreviewPanel()
    try:
        img = Image.new("RGB", (100, 100))
        assert img.format is None
        panel.set_image(img)
        assert panel._format_label.text() == "Format: Unknown"
    finally:
        panel.close()


def test_format_prefers_path_extension_over_pil_format(qapp: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """Format derives from path extension, not PIL format (which may be PNG from cache).

    Regression test for Bug 3: cached PNG thumbnails report image.format="PNG"
    regardless of the original file type.  The format label should show the
    original file's extension, not the cache format.
    """
    panel = PreviewPanel()
    try:
        test_file = tmp_path / "photo.jpeg"
        test_file.write_bytes(b"fake data")
        img = Image.new("RGB", (100, 100))
        img.format = "PNG"  # simulate cached PNG thumbnail
        panel.set_image(img, path=test_file)
        # Should use path extension (JPEG), not PIL format (PNG)
        assert panel._format_label.text() == "Format: JPEG"
    finally:
        panel.close()


# ── Edge Case: State Consistency ────────────────────────────────────


def test_set_image_replaces_previous_image(qapp: Any) -> None:  # noqa: ARG001
    """set_image fully replaces previous image and metadata."""
    panel = PreviewPanel()
    try:
        img1 = Image.new("RGB", (100, 100), color="red")
        img2 = Image.new("RGBA", (500, 400), color=(0, 0, 255, 128))

        panel.set_image(img1)
        assert panel._dimensions_label.text() == "Dimensions: 100 × 100"

        panel.set_image(img2)
        assert panel._current_image is not None
        assert panel._dimensions_label.text() == "Dimensions: 500 × 400"
    finally:
        panel.close()


def test_clear_when_already_clear_is_idempotent(qapp: Any) -> None:  # noqa: ARG001
    """clear on an already-cleared panel does not crash."""
    panel = PreviewPanel()
    try:
        panel.clear()
        panel.clear()
        assert panel._current_image is None
        assert panel._image_label.text() == "No preview"
    finally:
        panel.close()


# ── Edge Case: _pil_to_qimage Deep Copy ─────────────────────────────


def test_pil_to_qimage_returns_deep_copy_survives_gc(qapp: Any) -> None:  # noqa: ARG001
    """_pil_to_qimage returns a deep copy dat survives garbage collection."""
    img = Image.new("RGB", (50, 50), color="red")
    qimage = PreviewPanel._pil_to_qimage(img)

    # Delete source image and force GC
    del img
    gc.collect()

    # QImage should still be valid
    assert qimage.width() == 50
    assert qimage.height() == 50
    assert not qimage.isNull()


# ── Edge Case: EXIF Orientation ─────────────────────────────────────


def test_set_image_applies_exif_orientation(qapp: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """set_image applies EXIF orientation so dimensions swap accordingly.

    A 200×100 image wiv orientation tag 6 (rotate 90° CW) should display
    as 100×200 after EXIF transposition is applied.
    """
    panel = PreviewPanel()
    try:
        # Create a 200×100 image an' embed EXIF orientation tag 6
        img = Image.new("RGB", (200, 100), color="orange")
        exif = img.getexif()
        exif[0x0112] = 6  # Orientation: rotate 90° CW
        jpg_path = tmp_path / "exif_orientation.jpg"
        img.save(jpg_path, format="JPEG", exif=exif)
        img.close()

        # Re-open to get a loaded image wiv EXIF metadata
        loaded = Image.open(jpg_path)
        panel.set_image(loaded, path=jpg_path)

        # After exif_transpose, 200×100 becomes 100×200
        assert panel._dimensions_label.text() == "Dimensions: 100 × 200"
        assert panel._image_label.pixmap() is not None
        loaded.close()
    finally:
        panel.close()


def test_set_image_wiv_no_exif_keeps_dimensions(qapp: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """set_image wiv no EXIF orientation keeps original dimensions."""
    panel = PreviewPanel()
    try:
        img = Image.new("RGB", (200, 100), color="orange")
        jpg_path = tmp_path / "no_exif.jpg"
        img.save(jpg_path, format="JPEG")
        img.close()

        loaded = Image.open(jpg_path)
        panel.set_image(loaded, path=jpg_path)
        assert panel._dimensions_label.text() == "Dimensions: 200 × 100"
        loaded.close()
    finally:
        panel.close()


# ── Edge Case: Path wiv Directory Components ────────────────────────


def test_set_image_shows_only_filename_not_full_path(qapp: Any, sample_image: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """Metadata shows only filename, not full path."""
    panel = PreviewPanel()
    try:
        nested = tmp_path / "subdir" / "deep" / "image.png"
        nested.parent.mkdir(parents=True, exist_ok=True)
        nested.write_bytes(b"fake")
        panel.set_image(sample_image, path=nested)
        assert panel._filename_label.text() == "image.png"
        assert "/" not in panel._filename_label.text()
    finally:
        panel.close()


# ── Regression: RGBA → RGB conversion (Bug A — gray preview) ─────────


def test_set_image_rgba_converted_to_rgb(qapp: Any) -> None:  # noqa: ARG001
    """set_image converts RGBA images to RGB so they don't appear gray.

    Cached thumbnails are saved as RGBA.  When displayed via Qt's
    Format_RGBA8888 the alpha channel causes a washed-out / gray look.
    After set_image(), the internal image must be RGB.
    """
    panel = PreviewPanel()
    try:
        rgba = Image.new("RGBA", (200, 100), color=(255, 0, 0, 128))
        panel.set_image(rgba)
        assert panel._current_image is not None
        assert panel._current_image.mode == "RGB", (
            f"Expected RGB after set_image, got {panel._current_image.mode}"
        )
    finally:
        panel.close()


def test_set_image_rgba_fully_transparent_becomes_dark_bg(qapp: Any) -> None:  # noqa: ARG001
    """RGBA image wiv full transparency composites onto da dark preview bg."""
    from tarragon.widgets.preview_panel import BG_SECONDARY

    panel = PreviewPanel()
    try:
        # Fully transparent RGBA image
        rgba = Image.new("RGBA", (10, 10), color=(255, 0, 0, 0))
        panel.set_image(rgba)
        assert panel._current_image is not None
        assert panel._current_image.mode == "RGB"
        # All pixels should be the background colour (alpha=0 → only bg shows)
        pixel = panel._current_image.getpixel((0, 0))
        # BG_SECONDARY is "#1c1b22" → (28, 27, 34)
        assert pixel == (28, 27, 34), f"Expected bg colour (28, 27, 34), got {pixel}"
    finally:
        panel.close()


def test_set_image_rgba_semi_transparent_blends(qapp: Any) -> None:  # noqa: ARG001
    """RGBA semi-transparent pixels blend wiv da dark background."""
    panel = PreviewPanel()
    try:
        # 50% transparent white on dark bg
        rgba = Image.new("RGBA", (10, 10), color=(255, 255, 255, 128))
        panel.set_image(rgba)
        assert panel._current_image is not None
        assert panel._current_image.mode == "RGB"
        pixel = panel._current_image.getpixel((0, 0))
        # Blended value should be between bg (28,27,34) and white (255,255,255)
        for ch in pixel:
            assert 28 <= ch <= 255, f"Channel value {ch} outside expected blend range"
    finally:
        panel.close()


def test_set_image_rgb_stays_rgb(qapp: Any) -> None:  # noqa: ARG001
    """RGB images are not modified by da RGBA→RGB conversion."""
    panel = PreviewPanel()
    try:
        rgb = Image.new("RGB", (100, 100), color=(50, 100, 150))
        panel.set_image(rgb)
        assert panel._current_image is not None
        assert panel._current_image.mode == "RGB"
        # Pixel values should be unchanged
        pixel = panel._current_image.getpixel((0, 0))
        assert pixel == (50, 100, 150)
    finally:
        panel.close()


# ── Regression: EXIF from original file (Bug B — slanted preview) ─────


def test_set_image_applies_exif_from_original_for_cached_image(qapp: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """Cached image (no EXIF) gets orientation from da original file.

    Simulates da bug: a cached PNG has no EXIF data, but da original JPEG
    has orientation=6 (rotate 90° CW).  The preview should display da
    image rotated.
    """
    panel = PreviewPanel()
    try:
        # Create an original JPEG wiv EXIF orientation 6 (rotate 90° CW)
        orig = Image.new("RGB", (200, 100), color="orange")
        exif = orig.getexif()
        exif[0x0112] = 6  # Orientation: rotate 90° CW
        orig_path = tmp_path / "original.jpg"
        orig.save(orig_path, format="JPEG", exif=exif)
        orig.close()

        # Simulate a cached image: 200×100 PNG wiv NO EXIF
        # (as if it was saved before EXIF correction was added)
        cached = Image.new("RGB", (200, 100), color="orange")
        assert not cached.getexif().get(0x0112), "Cached image should have no EXIF orientation"

        # Display da cached image wiv path pointing to da original
        panel.set_image(cached, path=orig_path)

        # After EXIF recovery, 200×100 should become 100×200
        assert panel._dimensions_label.text() == "Dimensions: 100 × 200", (
            f"Expected 100 × 200 after EXIF recovery, got: {panel._dimensions_label.text()}"
        )
    finally:
        panel.close()


def test_set_image_no_double_rotation_when_image_has_exif(qapp: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """Image wiv its own EXIF is not double-rotated via da original file."""
    panel = PreviewPanel()
    try:
        # Create a JPEG wiv EXIF orientation 6
        img = Image.new("RGB", (200, 100), color="blue")
        exif = img.getexif()
        exif[0x0112] = 6
        jpg_path = tmp_path / "oriented.jpg"
        img.save(jpg_path, format="JPEG", exif=exif)
        img.close()

        # Re-open — dis image HAS its own EXIF
        loaded = Image.open(jpg_path)
        assert loaded.getexif().get(0x0112) == 6

        panel.set_image(loaded, path=jpg_path)

        # Should be rotated exactly once: 200×100 → 100×200
        assert panel._dimensions_label.text() == "Dimensions: 100 × 200"
        loaded.close()
    finally:
        panel.close()


def test_set_image_exif_recovery_noop_when_original_has_no_exif(qapp: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """EXIF recovery is a no-op when da original file has no orientation tag."""
    panel = PreviewPanel()
    try:
        # Original JPEG wiv no EXIF
        orig = Image.new("RGB", (200, 100), color="green")
        orig_path = tmp_path / "plain.jpg"
        orig.save(orig_path, format="JPEG")
        orig.close()

        # Cached image (no EXIF)
        cached = Image.new("RGB", (200, 100), color="green")
        panel.set_image(cached, path=orig_path)

        # Dimensions should be unchanged
        assert panel._dimensions_label.text() == "Dimensions: 200 × 100"
    finally:
        panel.close()


def test_set_image_exif_recovery_handles_missing_original(qapp: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """EXIF recovery gracefully handles a nonexistent original file."""
    panel = PreviewPanel()
    try:
        cached = Image.new("RGB", (200, 100), color="red")
        fake_path = tmp_path / "does_not_exist.jpg"
        # Should not raise
        panel.set_image(cached, path=fake_path)
        assert panel._current_image is not None
        assert panel._dimensions_label.text() == "Dimensions: 200 × 100"
    finally:
        panel.close()


# ── Unit tests for helper functions ────────────────────────────────────


def test_apply_exif_from_original_orientation_6(tmp_path: Any) -> None:
    """_apply_exif_from_original rotates 90° CW for orientation 6."""
    from tarragon.widgets.preview_panel import _apply_exif_from_original

    # Create original wiv orientation 6
    orig = Image.new("RGB", (200, 100), color="red")
    exif = orig.getexif()
    exif[0x0112] = 6
    orig_path = tmp_path / "test.jpg"
    orig.save(orig_path, format="JPEG", exif=exif)
    orig.close()

    # Apply to a different image (simulating cached image)
    cached = Image.new("RGB", (200, 100), color="red")
    result = _apply_exif_from_original(cached, orig_path)
    assert result.size == (100, 200), f"Expected (100, 200), got {result.size}"


def test_apply_exif_from_original_orientation_3(tmp_path: Any) -> None:
    """_apply_exif_from_original rotates 180° for orientation 3."""
    from tarragon.widgets.preview_panel import _apply_exif_from_original

    orig = Image.new("RGB", (200, 100), color="blue")
    exif = orig.getexif()
    exif[0x0112] = 3
    orig_path = tmp_path / "test.jpg"
    orig.save(orig_path, format="JPEG", exif=exif)
    orig.close()

    cached = Image.new("RGB", (200, 100), color="blue")
    result = _apply_exif_from_original(cached, orig_path)
    # 180° rotation preserves dimensions
    assert result.size == (200, 100)


def test_apply_exif_from_original_orientation_8(tmp_path: Any) -> None:
    """_apply_exif_from_original rotates 90° CCW for orientation 8."""
    from tarragon.widgets.preview_panel import _apply_exif_from_original

    orig = Image.new("RGB", (200, 100), color="green")
    exif = orig.getexif()
    exif[0x0112] = 8
    orig_path = tmp_path / "test.jpg"
    orig.save(orig_path, format="JPEG", exif=exif)
    orig.close()

    cached = Image.new("RGB", (200, 100), color="green")
    result = _apply_exif_from_original(cached, orig_path)
    assert result.size == (100, 200)


def test_apply_exif_from_original_no_orientation_tag(tmp_path: Any) -> None:
    """_apply_exif_from_original is a no-op when no orientation tag exists."""
    from tarragon.widgets.preview_panel import _apply_exif_from_original

    orig = Image.new("RGB", (200, 100), color="white")
    orig_path = tmp_path / "test.jpg"
    orig.save(orig_path, format="JPEG")
    orig.close()

    cached = Image.new("RGB", (200, 100), color="white")
    result = _apply_exif_from_original(cached, orig_path)
    assert result.size == (200, 100)


def test_apply_exif_from_original_missing_file(tmp_path: Any) -> None:
    """_apply_exif_from_original returns image unchanged for missing file."""
    from tarragon.widgets.preview_panel import _apply_exif_from_original

    cached = Image.new("RGB", (200, 100), color="red")
    fake_path = tmp_path / "nonexistent.jpg"
    result = _apply_exif_from_original(cached, fake_path)
    assert result.size == (200, 100)
    assert result is cached  # same object returned


def test_transpose_for_orientation_all_values() -> None:
    """_transpose_for_orientation handles all EXIF orientation values 2-8."""
    from tarragon.widgets.preview_panel import _transpose_for_orientation

    # Use an asymmetric image so rotations are detectable
    base = Image.new("RGB", (200, 100), color="red")

    # Orientation 2: flip horizontal
    result = _transpose_for_orientation(base.copy(), 2)
    assert result.size == (200, 100)

    # Orientation 3: rotate 180
    result = _transpose_for_orientation(base.copy(), 3)
    assert result.size == (200, 100)

    # Orientation 4: flip vertical
    result = _transpose_for_orientation(base.copy(), 4)
    assert result.size == (200, 100)

    # Orientation 5: transpose (rotate 90 + flip horizontal)
    result = _transpose_for_orientation(base.copy(), 5)
    assert result.size == (100, 200)

    # Orientation 6: rotate 270 (= 90 CW)
    result = _transpose_for_orientation(base.copy(), 6)
    assert result.size == (100, 200)

    # Orientation 7: transverse (rotate 90 + flip vertical)
    result = _transpose_for_orientation(base.copy(), 7)
    assert result.size == (100, 200)

    # Orientation 8: rotate 90 CCW
    result = _transpose_for_orientation(base.copy(), 8)
    assert result.size == (100, 200)

    # Orientation 1 (or unknown): no-op
    result = _transpose_for_orientation(base.copy(), 1)
    assert result.size == (200, 100)
    result = _transpose_for_orientation(base.copy(), 99)
    assert result.size == (200, 100)


def test_transpose_for_orientation_pixel_content_5_and_7() -> None:
    """_transpose_for_orientation produces correct pixels for orientations 5 and 7.

    Dimensions alone are insufficient — orientations 5 and 7 both swap
    dimensions the same way, but produce mirror-image pixel layouts.
    Uses an asymmetric pixel pattern so any mismatch is detectable.
    """
    from tarragon.widgets.preview_panel import _transpose_for_orientation

    # Create asymmetric test image — each pixel has a unique-ish value
    base = Image.new("RGB", (6, 4))
    for x in range(6):
        for y in range(4):
            base.putpixel((x, y), (x * 40, y * 60, 0))

    # Orientation 5: must match PIL's TRANSPOSE exactly
    expected_5 = base.transpose(Image.Transpose.TRANSPOSE)
    result_5 = _transpose_for_orientation(base.copy(), 5)
    assert result_5.size == expected_5.size, (
        f"Orientation 5 size mismatch: {result_5.size} != {expected_5.size}"
    )
    assert list(result_5.getdata()) == list(expected_5.getdata()), (
        "Orientation 5 pixels do not match PIL TRANSPOSE"
    )

    # Orientation 7: must match PIL's TRANSVERSE exactly
    expected_7 = base.transpose(Image.Transpose.TRANSVERSE)
    result_7 = _transpose_for_orientation(base.copy(), 7)
    assert result_7.size == expected_7.size, (
        f"Orientation 7 size mismatch: {result_7.size} != {expected_7.size}"
    )
    assert list(result_7.getdata()) == list(expected_7.getdata()), (
        "Orientation 7 pixels do not match PIL TRANSVERSE"
    )


# ── Regression: Double EXIF from cache (Bug 2) ────────────────────────


def test_set_image_skips_exif_recovery_when_from_cache(qapp: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """Cached image (_from_cache=True) does NOT get EXIF applied from original.

    Regression test for Bug 2: when a cached PNG (already correctly oriented)
    was loaded, set_image() would read EXIF from the original JPEG and apply
    rotation AGAIN, double-rotating the image.
    """
    panel = PreviewPanel()
    try:
        # Create an original JPEG wiv EXIF orientation 6 (rotate 90° CW)
        orig = Image.new("RGB", (200, 100), color="orange")
        exif = orig.getexif()
        exif[0x0112] = 6  # Orientation: rotate 90° CW
        orig_path = tmp_path / "original.jpg"
        orig.save(orig_path, format="JPEG", exif=exif)
        orig.close()

        # Simulate a cached image: 200×100 PNG wiv NO EXIF, marked as from cache
        cached = Image.new("RGB", (200, 100), color="orange")
        cached._from_cache = True
        assert not cached.getexif().get(0x0112), "Cached image should have no EXIF orientation"

        # Display da cached image wiv path pointing to da original
        panel.set_image(cached, path=orig_path)

        # Should NOT be rotated — cache already has correct orientation
        assert panel._dimensions_label.text() == "Dimensions: 200 × 100", (
            f"Expected 200 × 100 (no double rotation), got: {panel._dimensions_label.text()}"
        )
    finally:
        panel.close()


def test_set_image_applies_exif_from_original_when_not_from_cache(qapp: Any, tmp_path: Any) -> None:  # noqa: ARG001
    """Non-cached image wiv no EXIF still gets orientation from da original file.

    Ensures da _from_cache flag doesn't break da legitimate EXIF recovery path
    for images loaded directly from disk (not from cache).
    """
    panel = PreviewPanel()
    try:
        # Create an original JPEG wiv EXIF orientation 6
        orig = Image.new("RGB", (200, 100), color="orange")
        exif = orig.getexif()
        exif[0x0112] = 6
        orig_path = tmp_path / "original.jpg"
        orig.save(orig_path, format="JPEG", exif=exif)
        orig.close()

        # Image wiv NO EXIF and NO _from_cache flag (loaded from original)
        fresh = Image.new("RGB", (200, 100), color="orange")
        assert not getattr(fresh, "_from_cache", False)

        panel.set_image(fresh, path=orig_path)

        # Should be rotated once: 200×100 → 100×200
        assert panel._dimensions_label.text() == "Dimensions: 100 × 200"
    finally:
        panel.close()


# ── Regression: QImage stride mismatch (Bug — sheared preview) ────────


@pytest.mark.parametrize(
    "width",
    [1, 3, 5, 7, 9, 11, 13, 15, 17, 21, 63, 101],
)
def test_pil_to_qimage_rgb_pixel_values_non_aligned_widths(qapp: Any, width: int) -> None:  # noqa: ARG001
    """_pil_to_qimage preserves pixel values for RGB widths not divisible by 4.

    Regression test for stride mismatch: the 4-arg QImage constructor assumes
    4-byte aligned scanlines, but PIL's tobytes() produces tightly packed rows.
    When width × 3 is not divisible by 4, Qt reads past each row boundary,
    causing progressive shearing (the '/_/' glitch pattern).

    Uses widths where (width * 3) % 4 != 0 to trigger the bug.
    """
    height = 4
    # Create an image wiv unique pixel values per row so shearing is detectable
    pil_img = Image.new("RGB", (width, height))
    for y in range(height):
        for x in range(width):
            # Each pixel gets a unique color based on position
            pil_img.putpixel((x, y), ((x * 37 + y * 13) % 256, (x * 53 + y * 7) % 256, (x * 11 + y * 97) % 256))

    qimage = PreviewPanel._pil_to_qimage(pil_img)

    assert qimage.width() == width
    assert qimage.height() == height

    # Verify EVERY pixel — if stride is wrong, lower rows will be sheared
    for y in range(height):
        for x in range(width):
            expected = pil_img.getpixel((x, y))
            qcolor = qimage.pixelColor(x, y)
            actual = (qcolor.red(), qcolor.green(), qcolor.blue())
            assert actual == expected, (
                f"Pixel mismatch at ({x}, {y}) for width={width}: "
                f"expected {expected}, got {actual} — stride shearing detected!"
            )


def test_pil_to_qimage_rgba_pixel_values(qapp: Any) -> None:  # noqa: ARG001
    """_pil_to_qimage preserves RGBA pixel values including alpha channel."""
    width, height = 7, 3
    pil_img = Image.new("RGBA", (width, height))
    for y in range(height):
        for x in range(width):
            pil_img.putpixel((x, y), ((x * 41) % 256, (y * 67) % 256, ((x + y) * 23) % 256, (x * y * 17) % 256))

    qimage = PreviewPanel._pil_to_qimage(pil_img)

    for y in range(height):
        for x in range(width):
            expected = pil_img.getpixel((x, y))
            qcolor = qimage.pixelColor(x, y)
            actual = (qcolor.red(), qcolor.green(), qcolor.blue(), qcolor.alpha())
            assert actual == expected, (
                f"RGBA pixel mismatch at ({x}, {y}): expected {expected}, got {actual}"
            )


# ── Multi-preview: aspect ratio preservation ──────────────────────────


def test_set_multi_preview_preserves_wide_aspect_ratio(qapp: Any) -> None:  # noqa: ARG001
    """set_multi_preview does NOT crop wide images — aspect ratio is preserved.

    A wide panorama image (800×200, 4:1 ratio) placed in a single-cell mosaic
    should appear letterboxed (with background-colored bars top and bottom),
    not cropped to fill the square cell.
    """
    panel = PreviewPanel()
    try:
        wide = Image.new("RGB", (800, 200), color="red")
        captured: list[Image.Image] = []

        _real_pil_to_qimage = PreviewPanel._pil_to_qimage

        def _capture_qimage(pil_image: Image.Image) -> Any:
            captured.append(pil_image.copy())
            return _real_pil_to_qimage(pil_image)

        with patch.object(PreviewPanel, "_pil_to_qimage", side_effect=_capture_qimage):
            panel.set_multi_preview([wide], total_selected=1)

        assert len(captured) == 1
        mosaic = captured[0]

        # For 1 image: cols=1, rows=1, cell_w=cell_h=784 (800 - 2*8 padding)
        # The wide image (4:1) should be contained, not cropped.
        # Check that the top-center pixel is background color (letterbox bar),
        # not red (which would mean the image was cropped/stretched to fill).
        bg_color = (28, 27, 34)  # #1c1b22
        top_center = mosaic.getpixel((mosaic.width // 2, 10))
        assert top_center == bg_color, (
            f"Expected background color {bg_color} at top center (letterbox bar), "
            f"got {top_center} — image may have been cropped to fill cell"
        )

        # Center pixel should be red (the actual image content)
        center = mosaic.getpixel((mosaic.width // 2, mosaic.height // 2))
        assert center == (255, 0, 0), f"Expected red at center, got {center}"
    finally:
        panel.close()


def test_set_multi_preview_preserves_tall_aspect_ratio(qapp: Any) -> None:  # noqa: ARG001
    """set_multi_preview does NOT crop tall images — aspect ratio is preserved.

    A tall narrow image (200×800, 1:4 ratio) should appear pillarboxed
    (with background-colored bars on left and right).
    """
    panel = PreviewPanel()
    try:
        tall = Image.new("RGB", (200, 800), color="blue")
        captured: list[Image.Image] = []

        _real_pil_to_qimage = PreviewPanel._pil_to_qimage

        def _capture_qimage(pil_image: Image.Image) -> Any:
            captured.append(pil_image.copy())
            return _real_pil_to_qimage(pil_image)

        with patch.object(PreviewPanel, "_pil_to_qimage", side_effect=_capture_qimage):
            panel.set_multi_preview([tall], total_selected=1)

        assert len(captured) == 1
        mosaic = captured[0]

        bg_color = (28, 27, 34)  # #1c1b22
        # Left-center pixel should be background (pillarbox bar)
        left_center = mosaic.getpixel((10, mosaic.height // 2))
        assert left_center == bg_color, (
            f"Expected background color {bg_color} at left center (pillarbox bar), "
            f"got {left_center} — image may have been cropped to fill cell"
        )

        # Center pixel should be blue (the actual image content)
        center = mosaic.getpixel((mosaic.width // 2, mosaic.height // 2))
        assert center == (0, 0, 255), f"Expected blue at center, got {center}"
    finally:
        panel.close()


def test_set_multi_preview_square_image_fills_cell(qapp: Any) -> None:  # noqa: ARG001
    """set_multi_preview with a square image fills the cell completely."""
    panel = PreviewPanel()
    try:
        square = Image.new("RGB", (500, 500), color="green")
        captured: list[Image.Image] = []

        _real_pil_to_qimage = PreviewPanel._pil_to_qimage

        def _capture_qimage(pil_image: Image.Image) -> Any:
            captured.append(pil_image.copy())
            return _real_pil_to_qimage(pil_image)

        with patch.object(PreviewPanel, "_pil_to_qimage", side_effect=_capture_qimage):
            panel.set_multi_preview([square], total_selected=1)

        assert len(captured) == 1
        mosaic = captured[0]

        # Square image in square cell — should fill entirely, no letterboxing
        center = mosaic.getpixel((mosaic.width // 2, mosaic.height // 2))
        assert center == (0, 128, 0), f"Expected green at center, got {center}"

        # Corner of the cell area should also be green (no background bars)
        # Cell starts at (8, 8) — check just inside
        corner = mosaic.getpixel((10, 10))
        assert corner == (0, 128, 0), f"Expected green at cell corner, got {corner}"
    finally:
        panel.close()


def test_set_multi_preview_rgba_preserves_aspect_ratio(qapp: Any) -> None:  # noqa: ARG001
    """set_multi_preview handles RGBA images wiv aspect ratio preserved."""
    panel = PreviewPanel()
    try:
        rgba_wide = Image.new("RGBA", (600, 150), color=(255, 0, 0, 128))
        captured: list[Image.Image] = []

        _real_pil_to_qimage = PreviewPanel._pil_to_qimage

        def _capture_qimage(pil_image: Image.Image) -> Any:
            captured.append(pil_image.copy())
            return _real_pil_to_qimage(pil_image)

        with patch.object(PreviewPanel, "_pil_to_qimage", side_effect=_capture_qimage):
            panel.set_multi_preview([rgba_wide], total_selected=1)

        assert len(captured) == 1
        mosaic = captured[0]

        bg_color = (28, 27, 34)
        # Top area should be background (letterbox) since image is 4:1
        top_center = mosaic.getpixel((mosaic.width // 2, 10))
        assert top_center == bg_color, (
            f"Expected background at top, got {top_center}"
        )
    finally:
        panel.close()


def test_set_multi_preview_multiple_images_all_preserve_ratio(qapp: Any) -> None:  # noqa: ARG001
    """set_multi_preview wiv 4 images preserves each image's aspect ratio."""
    panel = PreviewPanel()
    try:
        images = [
            Image.new("RGB", (800, 200), color="red"),    # wide 4:1
            Image.new("RGB", (200, 800), color="blue"),   # tall 1:4
            Image.new("RGB", (400, 400), color="green"),  # square 1:1
            Image.new("RGB", (600, 300), color="yellow"), # wide 2:1
        ]
        captured: list[Image.Image] = []

        _real_pil_to_qimage = PreviewPanel._pil_to_qimage

        def _capture_qimage(pil_image: Image.Image) -> Any:
            captured.append(pil_image.copy())
            return _real_pil_to_qimage(pil_image)

        with patch.object(PreviewPanel, "_pil_to_qimage", side_effect=_capture_qimage):
            panel.set_multi_preview(images, total_selected=4)

        assert len(captured) == 1
        mosaic = captured[0]
        # Mosaic should be 800x800
        assert mosaic.size == (800, 800)

        # Verify the mosaic has non-uniform content (not all one color),
        # meaning images were placed with their aspect ratios preserved
        # and background fills the gaps
        bg_color = (28, 27, 34)
        # There should be some background pixels visible (from letterboxing/pillarboxing)
        bg_pixel_count = 0
        for y in range(0, 800, 20):
            for x in range(0, 800, 20):
                if mosaic.getpixel((x, y)) == bg_color:
                    bg_pixel_count += 1
        # With mixed aspect ratios, we expect significant background visibility
        assert bg_pixel_count > 0, "Expected some background pixels from aspect-ratio preservation"
    finally:
        panel.close()


# ── Tag Management Tests ──────────────────────────────────────────────


@pytest.fixture
def mock_tag_service() -> Any:
    """Create a mock TagService for testing tag management."""
    from unittest.mock import MagicMock

    # Create a mock that has the tagsChanged signal
    service = MagicMock()
    service.tagsChanged = MagicMock()
    service.get_tags_for_file.return_value = []
    service.get_all_tags.return_value = []
    service.get_file_tag_ids_batch.return_value = {}
    service.get_tag_name.return_value = None
    return service


def test_preview_panel_accepts_tag_service(qapp: Any) -> None:  # noqa: ARG001
    """PreviewPanel can be created with a tag_service parameter."""
    from unittest.mock import MagicMock

    service = MagicMock()
    panel = PreviewPanel(tag_service=service)
    try:
        assert panel._tag_service is service
    finally:
        panel.close()


def test_preview_panel_works_without_tag_service(qapp: Any) -> None:  # noqa: ARG001
    """PreviewPanel works without tag_service (display-only mode)."""
    panel = PreviewPanel()
    try:
        assert panel._tag_service is None
    finally:
        panel.close()


def test_set_tags_displays_pills(qapp: Any) -> None:  # noqa: ARG001
    """set_tags creates tag pill labels for each tag."""
    panel = PreviewPanel()
    try:
        tags = [
            {"id": 1, "name": "landscape", "source": "user"},
            {"id": 2, "name": "color:red", "source": "auto"},
        ]
        panel.set_tags(tags)
        assert len(panel._tag_pills) == 2
        assert panel._tag_pills[0].text() == "landscape"
        assert panel._tag_pills[1].text() == "color:red"
    finally:
        panel.close()


def test_set_tags_clears_previous_pills(qapp: Any) -> None:  # noqa: ARG001
    """set_tags replaces previous pills, not appending."""
    panel = PreviewPanel()
    try:
        panel.set_tags([{"id": 1, "name": "old_tag", "source": "user"}])
        assert len(panel._tag_pills) == 1

        panel.set_tags([
            {"id": 2, "name": "new_tag1", "source": "user"},
            {"id": 3, "name": "new_tag2", "source": "user"},
        ])
        assert len(panel._tag_pills) == 2
        assert panel._tag_pills[0].text() == "new_tag1"
    finally:
        panel.close()


def test_set_tags_empty_clears_pills(qapp: Any) -> None:  # noqa: ARG001
    """set_tags with empty list removes all pills."""
    panel = PreviewPanel()
    try:
        panel.set_tags([{"id": 1, "name": "tag", "source": "user"}])
        assert len(panel._tag_pills) == 1

        panel.set_tags([])
        assert len(panel._tag_pills) == 0
    finally:
        panel.close()


def test_set_tags_stores_current_tags(qapp: Any) -> None:  # noqa: ARG001
    """set_tags stores the tag list for later reference."""
    panel = PreviewPanel()
    try:
        tags = [{"id": 1, "name": "test", "source": "user"}]
        panel.set_tags(tags)
        assert panel._current_tags == tags
    finally:
        panel.close()


def test_set_tags_stores_selected_paths(qapp: Any) -> None:  # noqa: ARG001
    """set_tags stores selected_paths when provided."""
    panel = PreviewPanel()
    try:
        paths = ["/path/to/img1.jpg", "/path/to/img2.jpg"]
        panel.set_tags([], selected_paths=paths)
        assert panel._selected_paths == paths
    finally:
        panel.close()


def test_tag_pill_has_pointing_hand_cursor(qapp: Any) -> None:  # noqa: ARG001
    """Tag pills have PointingHandCursor to indicate clickability."""
    from PySide6.QtCore import Qt

    panel = PreviewPanel()
    try:
        panel.set_tags([{"id": 1, "name": "clickable", "source": "user"}])
        assert panel._tag_pills[0].cursor().shape() == Qt.CursorShape.PointingHandCursor
    finally:
        panel.close()


def test_tag_pill_role_primary_for_user_tags(qapp: Any) -> None:  # noqa: ARG001
    """User-created tags get 'primary' role."""
    panel = PreviewPanel()
    try:
        panel.set_tags([{"id": 1, "name": "manual", "source": "user"}])
        assert panel._tag_pills[0].property("tagRole") == "primary"
    finally:
        panel.close()


def test_tag_pill_role_secondary_for_auto_tags(qapp: Any) -> None:  # noqa: ARG001
    """Auto-generated tags get 'secondary' role."""
    panel = PreviewPanel()
    try:
        panel.set_tags([{"id": 1, "name": "color:red", "source": "auto"}])
        assert panel._tag_pills[0].property("tagRole") == "secondary"
    finally:
        panel.close()


def test_clear_resets_tag_state(qapp: Any) -> None:  # noqa: ARG001
    """clear() resets all tag-related state."""
    panel = PreviewPanel()
    try:
        panel.set_tags(
            [{"id": 1, "name": "tag", "source": "user"}],
            selected_paths=["/path/img.jpg"],
        )
        assert len(panel._current_tags) == 1
        assert len(panel._selected_paths) == 1

        panel.clear()
        assert panel._current_tags == []
        assert panel._selected_paths == []
        assert panel._cached_file_tags == {}
        assert len(panel._tag_pills) == 0
    finally:
        panel.close()


def test_tri_state_full_opacity_when_all_files_have_tag(qapp: Any) -> None:  # noqa: ARG001
    """Tag pill has full opacity (no effect) when all selected files have the tag."""
    panel = PreviewPanel()
    try:
        paths = ["/img1.jpg", "/img2.jpg"]
        # Both files have tag 1
        panel._cached_file_tags = {"/img1.jpg": {1}, "/img2.jpg": {1}}
        panel._selected_paths = paths

        pill = panel._create_tag_pill({"id": 1, "name": "shared", "source": "user"})
        # No opacity effect = full opacity
        assert pill.graphicsEffect() is None
    finally:
        panel.close()


def test_tri_state_half_opacity_when_some_files_have_tag(qapp: Any) -> None:  # noqa: ARG001
    """Tag pill has 0.5 opacity effect when only some selected files have the tag."""
    from PySide6.QtWidgets import QGraphicsOpacityEffect

    panel = PreviewPanel()
    try:
        paths = ["/img1.jpg", "/img2.jpg"]
        # Only first file has tag 1
        panel._cached_file_tags = {"/img1.jpg": {1}, "/img2.jpg": set()}
        panel._selected_paths = paths

        pill = panel._create_tag_pill({"id": 1, "name": "partial", "source": "user"})
        effect = pill.graphicsEffect()
        assert isinstance(effect, QGraphicsOpacityEffect)
        assert abs(effect.opacity() - 0.5) < 0.01
    finally:
        panel.close()


def test_tri_state_low_opacity_when_no_files_have_tag(qapp: Any) -> None:  # noqa: ARG001
    """Tag pill has 0.3 opacity effect when no selected files have the tag."""
    from PySide6.QtWidgets import QGraphicsOpacityEffect

    panel = PreviewPanel()
    try:
        paths = ["/img1.jpg", "/img2.jpg"]
        panel._cached_file_tags = {"/img1.jpg": set(), "/img2.jpg": set()}
        panel._selected_paths = paths

        pill = panel._create_tag_pill({"id": 1, "name": "absent", "source": "user"})
        effect = pill.graphicsEffect()
        assert isinstance(effect, QGraphicsOpacityEffect)
        assert abs(effect.opacity() - 0.3) < 0.01
    finally:
        panel.close()


def test_tri_state_not_applied_for_single_selection(qapp: Any) -> None:  # noqa: ARG001
    """Tri-state opacity is not applied for single file selection."""
    panel = PreviewPanel()
    try:
        paths = ["/img1.jpg"]
        panel._cached_file_tags = {"/img1.jpg": {1}}
        panel._selected_paths = paths

        pill = panel._create_tag_pill({"id": 1, "name": "tag", "source": "user"})
        # No opacity effect for single selection
        assert pill.graphicsEffect() is None
    finally:
        panel.close()


def test_tag_pill_clicked_removes_tag_when_all_have_it(
    qapp: Any, mock_tag_service: Any,  # noqa: ARG001
) -> None:
    """Clicking a tag pill removes the tag when all selected files have it."""
    service = mock_tag_service
    panel = PreviewPanel(tag_service=service)
    try:
        paths = ["/img1.jpg", "/img2.jpg"]
        panel._selected_paths = paths
        panel._cached_file_tags = {"/img1.jpg": {1}, "/img2.jpg": {1}}

        panel._on_tag_pill_clicked({"id": 1, "name": "tag", "source": "user"})
        service.remove_tags_from_files.assert_called_once_with(paths, {1})
        service.add_tags_to_files.assert_not_called()
    finally:
        panel.close()


def test_tag_pill_clicked_adds_tag_when_not_all_have_it(
    qapp: Any, mock_tag_service: Any,  # noqa: ARG001
) -> None:
    """Clicking a tag pill adds the tag when not all selected files have it."""
    service = mock_tag_service
    panel = PreviewPanel(tag_service=service)
    try:
        paths = ["/img1.jpg", "/img2.jpg"]
        panel._selected_paths = paths
        panel._cached_file_tags = {"/img1.jpg": {1}, "/img2.jpg": set()}

        panel._on_tag_pill_clicked({"id": 1, "name": "tag", "source": "user"})
        service.add_tags_to_files.assert_called_once_with(paths, ["tag"])
        service.remove_tags_from_files.assert_not_called()
    finally:
        panel.close()


def test_tag_pill_clicked_noop_without_selection(
    qapp: Any, mock_tag_service: Any,  # noqa: ARG001
) -> None:
    """Clicking a tag pill does nothing when no files are selected."""
    service = mock_tag_service
    panel = PreviewPanel(tag_service=service)
    try:
        panel._selected_paths = []
        panel._on_tag_pill_clicked({"id": 1, "name": "tag", "source": "user"})
        service.add_tags_to_files.assert_not_called()
        service.remove_tags_from_files.assert_not_called()
    finally:
        panel.close()


def test_tag_pill_clicked_noop_without_tag_service(qapp: Any) -> None:  # noqa: ARG001
    """Clicking a tag pill does nothing when no tag_service is set."""
    panel = PreviewPanel()  # No tag_service
    try:
        panel._selected_paths = ["/img1.jpg"]
        # Should not raise
        panel._on_tag_pill_clicked({"id": 1, "name": "tag", "source": "user"})
    finally:
        panel.close()


def test_inline_tag_input_creation(qapp: Any, mock_tag_service: Any) -> None:  # noqa: ARG001
    """_show_inline_tag_input creates a QLineEdit and hides the add button."""
    from PySide6.QtWidgets import QLineEdit

    service = mock_tag_service
    panel = PreviewPanel(tag_service=service)
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
    qapp: Any, mock_tag_service: Any,  # noqa: ARG001
) -> None:
    """Submitting inline input creates the tag and adds it to files."""
    service = mock_tag_service
    panel = PreviewPanel(tag_service=service)
    try:
        panel._selected_paths = ["/img1.jpg"]
        panel._show_inline_tag_input()
        panel._tag_input.setText("new_tag")

        panel._on_tag_input_submitted()
        service.add_tags_to_files.assert_called_once_with(["/img1.jpg"], ["new_tag"])
    finally:
        if panel._tag_input is not None:
            panel._tag_input.deleteLater()
        panel.close()


def test_inline_tag_input_finished_restores_button(
    qapp: Any, mock_tag_service: Any,  # noqa: ARG001
) -> None:
    """Finishing inline input removes the input and shows the add button."""
    service = mock_tag_service
    panel = PreviewPanel(tag_service=service)
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
    qapp: Any, mock_tag_service: Any,  # noqa: ARG001
) -> None:
    """Submitting empty inline input does not create a tag."""
    service = mock_tag_service
    panel = PreviewPanel(tag_service=service)
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


def test_get_union_tags_combines_tags_from_multiple_files(
    qapp: Any, mock_tag_service: Any,  # noqa: ARG001
) -> None:
    """_get_union_tags returns the union of tags from all paths."""
    service = mock_tag_service
    service.get_tags_for_file.side_effect = lambda path: {
        "/img1.jpg": [{"id": 1, "name": "shared", "source": "user"}],
        "/img2.jpg": [
            {"id": 1, "name": "shared", "source": "user"},
            {"id": 2, "name": "unique", "source": "user"},
        ],
    }[path]
    service.get_tag_name.side_effect = lambda tid: {1: "shared", 2: "unique"}.get(tid)

    panel = PreviewPanel(tag_service=service)
    try:
        union = panel._get_union_tags(["/img1.jpg", "/img2.jpg"])
        assert len(union) == 2
        tag_ids = {t["id"] for t in union}
        assert tag_ids == {1, 2}
    finally:
        panel.close()


def test_get_union_tags_without_tag_service_returns_empty(qapp: Any) -> None:  # noqa: ARG001
    """_get_union_tags returns empty list when no tag_service."""
    panel = PreviewPanel()
    try:
        union = panel._get_union_tags(["/img1.jpg"])
        assert union == []
    finally:
        panel.close()


def test_tags_changed_signal_emitted_on_external_change(
    qapp: Any, mock_tag_service: Any,  # noqa: ARG001
) -> None:
    """tags_changed signal is emitted when external tags change."""
    service = mock_tag_service
    service.get_tags_for_file.return_value = []

    panel = PreviewPanel(tag_service=service)
    try:
        signal_received = []
        panel.tags_changed.connect(lambda: signal_received.append(True))

        panel._selected_paths = ["/img1.jpg"]
        panel._on_external_tags_changed()
        assert len(signal_received) == 1
    finally:
        panel.close()


def test_add_button_disabled_without_selection(
    qapp: Any, mock_tag_service: Any,  # noqa: ARG001
) -> None:
    """Add tag button does nothing when no files are selected."""
    service = mock_tag_service
    panel = PreviewPanel(tag_service=service)
    try:
        panel._selected_paths = []
        # Should not raise or show menu
        panel._on_add_tag_clicked()
        service.get_all_tags.assert_not_called()
    finally:
        panel.close()


# ── Regression: Tags not visible in preview panel ─────────────────────


def test_tags_container_has_minimum_height_when_empty(qapp: Any) -> None:  # noqa: ARG001
    """Tags container has a minimum height even when no tags are present.

    Regression test: the _tags_container collapsed to zero height when empty
    because _FlowLayout.minimumSizeHint() returned QSize(0, 0), making tags
    invisible even after they were added.
    """
    panel = PreviewPanel()
    try:
        assert panel._tags_container.minimumHeight() > 0, (
            "Tags container should have a minimum height to remain visible when empty"
        )
    finally:
        panel.close()


def test_tags_container_nonzero_height_after_set_tags(qapp: Any) -> None:  # noqa: ARG001
    """After set_tags() with tags, the tags container has non-zero height.

    Regression test: set_tags() added pills but never called updateGeometry(),
    so the layout never recalculated and the container stayed at zero height.
    """
    panel = PreviewPanel()
    try:
        panel.show()
        tags = [
            {"id": 1, "name": "landscape", "source": "user"},
            {"id": 2, "name": "portrait", "source": "user"},
        ]
        panel.set_tags(tags)

        # The container must have non-zero height after tags are added
        assert panel._tags_container.height() > 0, (
            "Tags container should have non-zero height after set_tags()"
        )
    finally:
        panel.close()


def test_tag_pills_have_nonzero_size_after_set_tags(qapp: Any) -> None:  # noqa: ARG001
    """Tag pills have non-zero size after set_tags().

    Regression test: pills were created but the flow layout didn't position
    them because minimumSizeHint() returned (0, 0) and no geometry update
    was triggered.
    """
    panel = PreviewPanel()
    try:
        panel.show()
        tags = [
            {"id": 1, "name": "landscape", "source": "user"},
            {"id": 2, "name": "color:red", "source": "auto"},
        ]
        panel.set_tags(tags)

        for i, pill in enumerate(panel._tag_pills):
            assert pill.width() > 0, f"Pill {i} ('{pill.text()}') has zero width"
            assert pill.height() > 0, f"Pill {i} ('{pill.text()}') has zero height"
    finally:
        panel.close()


def test_flow_layout_minimum_size_hint_with_items(qapp: Any) -> None:  # noqa: ARG001
    """_FlowLayout.minimumSizeHint() returns proper size when items exist."""
    from tarragon.widgets.preview_panel import _FlowLayout

    panel = PreviewPanel()
    try:
        panel.show()
        tags = [{"id": 1, "name": "test_tag", "source": "user"}]
        panel.set_tags(tags)

        hint = panel._tags_flow.minimumSizeHint()
        assert hint.height() > 0, (
            f"FlowLayout minimumSizeHint height should be > 0 with items, got {hint.height()}"
        )
    finally:
        panel.close()


def test_flow_layout_minimum_size_hint_empty_returns_nonzero_height(qapp: Any) -> None:  # noqa: ARG001
    """_FlowLayout.minimumSizeHint() returns nonzero height when empty.

    This prevents the tags container from collapsing to zero height.
    """
    from tarragon.widgets.preview_panel import _FlowLayout

    panel = PreviewPanel()
    try:
        hint = panel._tags_flow.minimumSizeHint()
        assert hint.height() > 0, (
            f"FlowLayout minimumSizeHint height should be > 0 even when empty, got {hint.height()}"
        )
    finally:
        panel.close()


def test_tags_container_geometry_updates_on_clear(qapp: Any) -> None:  # noqa: ARG001
    """Tags container geometry updates properly after clearing tags.

    Regression test: _clear_tag_pills() didn't call updateGeometry(),
    so the container could remain at its expanded size after clearing.
    """
    panel = PreviewPanel()
    try:
        panel.show()
        # Add tags
        panel.set_tags([
            {"id": 1, "name": "tag1", "source": "user"},
            {"id": 2, "name": "tag2", "source": "user"},
        ])
        assert len(panel._tag_pills) == 2

        # Clear tags
        panel.set_tags([])
        assert len(panel._tag_pills) == 0

        # Container should still have its minimum height (not collapsed)
        assert panel._tags_container.minimumHeight() > 0
    finally:
        panel.close()


# ── Regression: Add Tag button full width ─────────────────────────────


def test_add_tag_button_has_maximum_size_policy(qapp: Any) -> None:  # noqa: ARG001
    """The '+ add' tag button uses Maximum size policy so it doesn't stretch.

    Regression test: the button had no size constraint, causing it to expand
    to fill the full width of the preview panel layout.
    """
    panel = PreviewPanel()
    try:
        policy = panel._add_tag_btn.sizePolicy()
        assert policy.horizontalPolicy() == QSizePolicy.Policy.Maximum
        assert policy.verticalPolicy() == QSizePolicy.Policy.Fixed
    finally:
        panel.close()
