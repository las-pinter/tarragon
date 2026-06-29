"""Tests for PreviewPanel widget."""

from __future__ import annotations

import gc
from pathlib import Path
from unittest.mock import patch

import pytest
from PIL import Image
from PySide6.QtCore import QSize
from PySide6.QtGui import QResizeEvent
from PySide6.QtWidgets import QLabel, QSizePolicy, QVBoxLayout, QWidget
from tarragon.widgets.preview_panel import PreviewPanel


@pytest.fixture(autouse=True)
def qapp():
    """Provide a shared QApplication instance for all Qt tests."""
    from PySide6.QtWidgets import QApplication

    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


@pytest.fixture
def sample_image():
    """Create a sample PIL Image for testing."""
    return Image.new("RGB", (800, 600), color="red")


@pytest.fixture
def sample_rgba_image():
    """Create a sample RGBA PIL Image for testing."""
    return Image.new("RGBA", (1024, 768), color=(0, 255, 0, 128))


# ── Instantiation Tests ──────────────────────────────────────────────


def test_preview_panel_is_qwidget():
    """PreviewPanel is a QWidget subclass."""
    assert issubclass(PreviewPanel, QWidget)


def test_preview_panel_instantiation(qapp):  # noqa: ARG001
    """PreviewPanel can be created without errors."""
    panel = PreviewPanel()
    try:
        assert panel is not None
        assert isinstance(panel, QWidget)
    finally:
        panel.close()


def test_preview_panel_has_layout(qapp):  # noqa: ARG001
    """PreviewPanel has a QVBoxLayout."""
    panel = PreviewPanel()
    try:
        layout = panel.layout()
        assert isinstance(layout, QVBoxLayout)
    finally:
        panel.close()


def test_preview_panel_has_image_label(qapp):  # noqa: ARG001
    """PreviewPanel has an image QLabel."""
    panel = PreviewPanel()
    try:
        assert hasattr(panel, "_image_label")
        assert isinstance(panel._image_label, QLabel)
    finally:
        panel.close()


def test_image_label_size_policy_is_ignored(qapp):  # noqa: ARG001
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


def test_preview_panel_initial_state(qapp):  # noqa: ARG001
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


def test_set_image_rgb(qapp, sample_image):  # noqa: ARG001
    """set_image displays an RGB image."""
    panel = PreviewPanel()
    try:
        panel.set_image(sample_image)
        assert panel._current_image is not None
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 800 × 600"
    finally:
        panel.close()


def test_set_image_rgba(qapp, sample_rgba_image):  # noqa: ARG001
    """set_image displays an RGBA image."""
    panel = PreviewPanel()
    try:
        panel.set_image(sample_rgba_image)
        assert panel._current_image is not None
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 1024 × 768"
    finally:
        panel.close()


def test_set_image_wiv_path(qapp, sample_image, tmp_path):  # noqa: ARG001
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


def test_set_image_wivout_path(qapp, sample_image):  # noqa: ARG001
    """set_image works without a path (shows 'Unknown file')."""
    panel = PreviewPanel()
    try:
        panel.set_image(sample_image)
        assert panel._filename_label.text() == "Unknown file"
        assert panel._size_label.text() == "Size: Unknown"
    finally:
        panel.close()


# ── clear Tests ──────────────────────────────────────────────────────


def test_clear_resets_state(qapp, sample_image):  # noqa: ARG001
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


def test_format_size_bytes(qapp):  # noqa: ARG001
    """_format_size formats bytes correctly."""
    assert PreviewPanel._format_size(500) == "500 B"


def test_format_size_kilobytes(qapp):  # noqa: ARG001
    """_format_size formats kilobytes correctly."""
    assert PreviewPanel._format_size(1536) == "1.5 KB"


def test_format_size_megabytes(qapp):  # noqa: ARG001
    """_format_size formats megabytes correctly."""
    assert PreviewPanel._format_size(2 * 1024 * 1024) == "2.0 MB"


def test_format_size_gigabytes(qapp):  # noqa: ARG001
    """_format_size formats gigabytes correctly."""
    assert PreviewPanel._format_size(3 * 1024 * 1024 * 1024) == "3.0 GB"


# ── PIL to QImage Conversion Tests ───────────────────────────────────


def test_pil_to_qimage_rgb(qapp, sample_image):  # noqa: ARG001
    """_pil_to_qimage converts RGB image correctly."""
    qimage = PreviewPanel._pil_to_qimage(sample_image)
    assert qimage.width() == 800
    assert qimage.height() == 600


def test_pil_to_qimage_rgba(qapp, sample_rgba_image):  # noqa: ARG001
    """_pil_to_qimage converts RGBA image correctly."""
    qimage = PreviewPanel._pil_to_qimage(sample_rgba_image)
    assert qimage.width() == 1024
    assert qimage.height() == 768


def test_pil_to_qimage_grayscale(qapp):  # noqa: ARG001
    """_pil_to_qimage converts grayscale image to RGB."""
    gray_image = Image.new("L", (400, 300), color=128)
    qimage = PreviewPanel._pil_to_qimage(gray_image)
    assert qimage.width() == 400
    assert qimage.height() == 300


# ── Edge Case: Boundary Image Sizes ─────────────────────────────────


def test_set_image_1x1_pixel(qapp):  # noqa: ARG001
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


def test_set_image_very_wide_panorama(qapp):  # noqa: ARG001
    """set_image handles extreme aspect ratio (wide panorama)."""
    panel = PreviewPanel()
    try:
        wide = Image.new("RGB", (4000, 100), color="green")
        panel.set_image(wide)
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 4000 × 100"
    finally:
        panel.close()


def test_set_image_very_tall(qapp):  # noqa: ARG001
    """set_image handles extreme aspect ratio (tall/narrow image)."""
    panel = PreviewPanel()
    try:
        tall = Image.new("RGB", (50, 5000), color="yellow")
        panel.set_image(tall)
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 50 × 5000"
    finally:
        panel.close()


def test_set_image_large_25mp(qapp):  # noqa: ARG001
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


def test_set_image_cmyk_mode(qapp):  # noqa: ARG001
    """set_image converts CMYK image to RGB for display."""
    panel = PreviewPanel()
    try:
        cmyk = Image.new("CMYK", (200, 200), color=(0, 0, 0, 0))
        panel.set_image(cmyk)
        assert panel._image_label.pixmap() is not None
        assert panel._dimensions_label.text() == "Dimensions: 200 × 200"
    finally:
        panel.close()


def test_set_image_palette_mode(qapp):  # noqa: ARG001
    """set_image converts palette (P) mode image for display."""
    panel = PreviewPanel()
    try:
        palette_img = Image.new("P", (300, 200))
        palette_img.putpalette([i % 256 for i in range(768)])
        panel.set_image(palette_img)
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


def test_set_image_1bit_mode(qapp):  # noqa: ARG001
    """set_image converts 1-bit binary image for display."""
    panel = PreviewPanel()
    try:
        binary = Image.new("1", (100, 100), color=1)
        panel.set_image(binary)
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


def test_set_image_la_mode(qapp):  # noqa: ARG001
    """set_image converts LA (grayscale + alpha) image for display."""
    panel = PreviewPanel()
    try:
        la_img = Image.new("LA", (200, 200), color=(128, 255))
        panel.set_image(la_img)
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


def test_set_image_i_mode_32bit(qapp):  # noqa: ARG001
    """set_image converts I (32-bit integer) mode image for display."""
    panel = PreviewPanel()
    try:
        i_img = Image.new("I", (200, 200), color=1000)
        panel.set_image(i_img)
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


def test_set_image_f_mode_float(qapp):  # noqa: ARG001
    """set_image converts F (float) mode image for display."""
    panel = PreviewPanel()
    try:
        f_img = Image.new("F", (200, 200), color=1.5)
        panel.set_image(f_img)
        assert panel._image_label.pixmap() is not None
    finally:
        panel.close()


# ── Edge Case: Animated GIF ─────────────────────────────────────────


def test_set_image_animated_gif_shows_first_frame(qapp, tmp_path):  # noqa: ARG001
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


def test_set_image_wiv_none_raises_typeerror(qapp):  # noqa: ARG001
    """set_image wiv None raises TypeError (explicit guard against None)."""
    panel = PreviewPanel()
    try:
        with pytest.raises(TypeError, match="image must be a PIL Image, not None"):
            panel.set_image(None)  # type: ignore[arg-type]
    finally:
        panel.close()


# ── Edge Case: resizeEvent ──────────────────────────────────────────


def test_resize_event_wiv_no_image_does_not_crash(qapp):  # noqa: ARG001
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


def test_resize_event_wiv_image_reapplies(qapp, sample_image):  # noqa: ARG001
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


def test_set_image_caches_pixmap(qapp, sample_image):  # noqa: ARG001
    """set_image caches da full-resolution pixmap for fast resizing."""
    panel = PreviewPanel()
    try:
        assert panel._cached_pixmap is None
        panel.set_image(sample_image)
        assert panel._cached_pixmap is not None
        assert not panel._cached_pixmap.isNull()
    finally:
        panel.close()


def test_resize_event_does_not_reconvert(qapp, sample_image):  # noqa: ARG001
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


def test_clear_resets_cached_pixmap(qapp, sample_image):  # noqa: ARG001
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


def test_multiple_set_image_calls_last_one_wins(qapp):  # noqa: ARG001
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


def test_set_image_after_clear(qapp, sample_image):  # noqa: ARG001
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


def test_set_image_wiv_nonexistent_path_shows_unknown_size(qapp, sample_image):  # noqa: ARG001
    """set_image wiv nonexistent path shows 'Size: Unknown'."""
    panel = PreviewPanel()
    try:
        fake_path = Path("/nonexistent/path/to/image.jpg")
        panel.set_image(sample_image, path=fake_path)
        assert panel._filename_label.text() == "image.jpg"
        assert panel._size_label.text() == "Size: Unknown"
    finally:
        panel.close()


def test_set_image_wiv_path_stat_raises_oserror(qapp, sample_image):  # noqa: ARG001
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


def test_set_image_wiv_unicode_filename(qapp, sample_image, tmp_path):  # noqa: ARG001
    """set_image handles Unicode characters in filename."""
    panel = PreviewPanel()
    try:
        unicode_file = tmp_path / "тест_画像_🖼️.png"
        unicode_file.write_bytes(b"fake data")
        panel.set_image(sample_image, path=unicode_file)
        assert panel._filename_label.text() == "тест_画像_🖼️.png"
    finally:
        panel.close()


def test_set_image_wiv_very_long_filename(qapp, sample_image, tmp_path):  # noqa: ARG001
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


def test_format_size_zero_bytes(qapp):  # noqa: ARG001
    """_format_size handles 0 bytes."""
    assert PreviewPanel._format_size(0) == "0 B"


def test_format_size_exactly_1024_bytes(qapp):  # noqa: ARG001
    """_format_size handles exactly 1024 bytes (1.0 KB)."""
    assert PreviewPanel._format_size(1024) == "1.0 KB"


def test_format_size_terabytes(qapp):  # noqa: ARG001
    """_format_size handles terabyte range."""
    assert PreviewPanel._format_size(5 * 1024**4) == "5.0 TB"


def test_format_size_1_byte(qapp):  # noqa: ARG001
    """_format_size handles exactly 1 byte."""
    assert PreviewPanel._format_size(1) == "1 B"


# ── Edge Case: Format Fallback ──────────────────────────────────────


def test_format_fallback_to_path_extension(qapp, tmp_path):  # noqa: ARG001
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


def test_format_shows_unknown_when_no_format_and_no_path(qapp):  # noqa: ARG001
    """Format shows 'Unknown' when image has no format and no path given."""
    panel = PreviewPanel()
    try:
        img = Image.new("RGB", (100, 100))
        assert img.format is None
        panel.set_image(img)
        assert panel._format_label.text() == "Format: Unknown"
    finally:
        panel.close()


def test_format_prefers_path_extension_over_pil_format(qapp, tmp_path):  # noqa: ARG001
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


def test_set_image_replaces_previous_image(qapp):  # noqa: ARG001
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


def test_clear_when_already_clear_is_idempotent(qapp):  # noqa: ARG001
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


def test_pil_to_qimage_returns_deep_copy_survives_gc(qapp):  # noqa: ARG001
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


def test_set_image_applies_exif_orientation(qapp, tmp_path):  # noqa: ARG001
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


def test_set_image_wiv_no_exif_keeps_dimensions(qapp, tmp_path):  # noqa: ARG001
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


def test_set_image_shows_only_filename_not_full_path(qapp, sample_image, tmp_path):  # noqa: ARG001
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


def test_set_image_rgba_converted_to_rgb(qapp):  # noqa: ARG001
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


def test_set_image_rgba_fully_transparent_becomes_dark_bg(qapp):  # noqa: ARG001
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


def test_set_image_rgba_semi_transparent_blends(qapp):  # noqa: ARG001
    """RGBA semi-transparent pixels blend wiv da dark background."""
    panel = PreviewPanel()
    try:
        # 50% transparent white on dark bg
        rgba = Image.new("RGBA", (10, 10), color=(255, 255, 255, 128))
        panel.set_image(rgba)
        assert panel._current_image.mode == "RGB"
        pixel = panel._current_image.getpixel((0, 0))
        # Blended value should be between bg (28,27,34) and white (255,255,255)
        for ch in pixel:
            assert 28 <= ch <= 255, f"Channel value {ch} outside expected blend range"
    finally:
        panel.close()


def test_set_image_rgb_stays_rgb(qapp):  # noqa: ARG001
    """RGB images are not modified by da RGBA→RGB conversion."""
    panel = PreviewPanel()
    try:
        rgb = Image.new("RGB", (100, 100), color=(50, 100, 150))
        panel.set_image(rgb)
        assert panel._current_image.mode == "RGB"
        # Pixel values should be unchanged
        pixel = panel._current_image.getpixel((0, 0))
        assert pixel == (50, 100, 150)
    finally:
        panel.close()


# ── Regression: EXIF from original file (Bug B — slanted preview) ─────


def test_set_image_applies_exif_from_original_for_cached_image(qapp, tmp_path):  # noqa: ARG001
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


def test_set_image_no_double_rotation_when_image_has_exif(qapp, tmp_path):  # noqa: ARG001
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


def test_set_image_exif_recovery_noop_when_original_has_no_exif(qapp, tmp_path):  # noqa: ARG001
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


def test_set_image_exif_recovery_handles_missing_original(qapp, tmp_path):  # noqa: ARG001
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


def test_apply_exif_from_original_orientation_6(tmp_path):
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


def test_apply_exif_from_original_orientation_3(tmp_path):
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


def test_apply_exif_from_original_orientation_8(tmp_path):
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


def test_apply_exif_from_original_no_orientation_tag(tmp_path):
    """_apply_exif_from_original is a no-op when no orientation tag exists."""
    from tarragon.widgets.preview_panel import _apply_exif_from_original

    orig = Image.new("RGB", (200, 100), color="white")
    orig_path = tmp_path / "test.jpg"
    orig.save(orig_path, format="JPEG")
    orig.close()

    cached = Image.new("RGB", (200, 100), color="white")
    result = _apply_exif_from_original(cached, orig_path)
    assert result.size == (200, 100)


def test_apply_exif_from_original_missing_file(tmp_path):
    """_apply_exif_from_original returns image unchanged for missing file."""
    from tarragon.widgets.preview_panel import _apply_exif_from_original

    cached = Image.new("RGB", (200, 100), color="red")
    fake_path = tmp_path / "nonexistent.jpg"
    result = _apply_exif_from_original(cached, fake_path)
    assert result.size == (200, 100)
    assert result is cached  # same object returned


def test_transpose_for_orientation_all_values():
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


def test_transpose_for_orientation_pixel_content_5_and_7():
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


def test_set_image_skips_exif_recovery_when_from_cache(qapp, tmp_path):  # noqa: ARG001
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
        cached._from_cache = True  # type: ignore[attr-defined]
        assert not cached.getexif().get(0x0112), "Cached image should have no EXIF orientation"

        # Display da cached image wiv path pointing to da original
        panel.set_image(cached, path=orig_path)

        # Should NOT be rotated — cache already has correct orientation
        assert panel._dimensions_label.text() == "Dimensions: 200 × 100", (
            f"Expected 200 × 100 (no double rotation), got: {panel._dimensions_label.text()}"
        )
    finally:
        panel.close()


def test_set_image_applies_exif_from_original_when_not_from_cache(qapp, tmp_path):  # noqa: ARG001
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
