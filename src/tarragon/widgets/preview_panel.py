"""Preview panel widget — displays single image preview with metadata and mosaic multi-preview."""

from __future__ import annotations

import logging
import math
from pathlib import Path

from PIL import Image, ImageOps
from PySide6.QtCore import Qt
from PySide6.QtGui import QImage, QPixmap, QResizeEvent
from PySide6.QtWidgets import QLabel, QSizePolicy, QVBoxLayout, QWidget

from tarragon.theme.spacing import SM
from tarragon.theme.tokens import get_token
from tarragon.theme.typography import BODY_SIZE

logger = logging.getLogger(__name__)

# EXIF orientation tag ID
_EXIF_ORIENTATION_TAG = 0x0112

# Theme tokens (coral-amber dark palette) — sourced from centralized theme system
BG_PRIMARY: str = get_token("colors", "bg_primary")
BG_SECONDARY: str = get_token("colors", "bg_secondary")
TEXT_PRIMARY: str = get_token("colors", "text_primary")
TEXT_SECONDARY: str = get_token("colors", "text_secondary")
CORAL_STRONG: str = get_token("colors", "coral_strong")


def _apply_exif_from_original(image: Image.Image, original_path: Path) -> Image.Image:
    """Apply EXIF orientation from the original file to a cached image.

    Cached PNG thumbnails strip EXIF metadata, so ``ImageOps.exif_transpose()``
    is a no-op on them.  This helper reads the orientation tag from the
    *original* source file and applies the corresponding geometric
    transformation to *image* so that old caches still display correctly.

    Parameters
    ----------
    image:
        The (cached) PIL Image to transform in-place (a copy is returned).
    original_path:
        Path to the original source file whose EXIF orientation to read.

    Returns
    -------
    Image.Image
        The orientation-corrected image (may be the same object if no
        correction was needed).
    """
    try:
        with Image.open(original_path) as orig:
            exif = orig.getexif()
            orientation = exif.get(_EXIF_ORIENTATION_TAG)
            if orientation and orientation != 1:
                image = _transpose_for_orientation(image, orientation)
    except Exception:  # noqa: BLE001 — best-effort; never block preview
        logger.warning("Failed to read EXIF orientation from %s", original_path, exc_info=True)
    return image


def _transpose_for_orientation(image: Image.Image, orientation: int) -> Image.Image:
    """Apply the EXIF orientation transformation to *image*.

    Mirrors the logic of :func:`PIL.ImageOps.exif_transpose` for orientation
    values 2–8.  Orientation 1 (normal) is handled by the caller.
    """
    if orientation == 2:
        return image.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
    if orientation == 3:
        return image.transpose(Image.Transpose.ROTATE_180)
    if orientation == 4:
        return image.transpose(Image.Transpose.FLIP_TOP_BOTTOM)
    if orientation == 5:
        return image.transpose(Image.Transpose.TRANSPOSE)
    if orientation == 6:
        return image.transpose(Image.Transpose.ROTATE_270)
    if orientation == 7:
        return image.transpose(Image.Transpose.TRANSVERSE)
    if orientation == 8:
        return image.transpose(Image.Transpose.ROTATE_90)
    return image


class PreviewPanel(QWidget):
    """Widget that displays a single image preview with metadata.

    Shows:
    - Scaled image (maintains aspect ratio, fits panel)
    - Filename
    - Dimensions (width × height)
    - File size (formatted)
    - Format (JPEG, PNG, PSD, etc.)
    """

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._setup_ui()
        self._current_image: Image.Image | None = None
        self._current_path: Path | None = None
        self._cached_pixmap: QPixmap | None = None
        self._original_width: int | None = None
        self._original_height: int | None = None

    def _setup_ui(self) -> None:
        """Build the UI layout."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(SM, SM, SM, SM)
        layout.setSpacing(SM)

        # Image label (centered, scaled)
        self._image_label = QLabel()
        self._image_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._image_label.setMinimumSize(200, 200)
        self._image_label.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Ignored)
        self._image_label.setText("No preview")
        self._image_label.setStyleSheet(
            f"QLabel {{"
            f"  background-color: {BG_SECONDARY};"
            f"  border: 1px solid {CORAL_STRONG};"
            f"  border-radius: 4px;"
            f"  color: {TEXT_SECONDARY};"
            f"  font-size: {BODY_SIZE}px;"
            f"}}"
        )
        layout.addWidget(self._image_label, stretch=1)

        # Metadata labels
        self._filename_label = QLabel()
        self._filename_label.setStyleSheet(f"color: {TEXT_PRIMARY}; font-weight: bold;")
        self._filename_label.setWordWrap(True)
        layout.addWidget(self._filename_label)

        self._dimensions_label = QLabel()
        self._dimensions_label.setStyleSheet(f"color: {TEXT_SECONDARY};")
        layout.addWidget(self._dimensions_label)

        self._size_label = QLabel()
        self._size_label.setStyleSheet(f"color: {TEXT_SECONDARY};")
        layout.addWidget(self._size_label)

        self._format_label = QLabel()
        self._format_label.setStyleSheet(f"color: {TEXT_SECONDARY};")
        layout.addWidget(self._format_label)

        layout.addStretch()

        self.setStyleSheet(f"background-color: {BG_PRIMARY};")

    def set_image(
        self,
        image: Image.Image,
        path: Path | None = None,
        original_width: int | None = None,
        original_height: int | None = None,
    ) -> None:
        """Set the image to display.

        Args:
            image: PIL Image to display
            path: Optional file path for metadata display and EXIF recovery
            original_width: Original image width in pixels (before caching/thumbnailing).
                When provided, displayed in metadata instead of the (possibly downscaled)
                image's actual pixel width.
            original_height: Original image height in pixels (before caching/thumbnailing).
                When provided, displayed in metadata instead of the (possibly downscaled)
                image's actual pixel height.

        Raises:
            TypeError: If ``image`` is None.
        """
        if image is None:
            raise TypeError("image must be a PIL Image, not None")

        self._original_width = original_width
        self._original_height = original_height

        logger.debug(
            "set_image: path=%s, size=%s, from_cache=%s",
            path,
            image.size,
            getattr(image, "_from_cache", False),
        )

        # Apply EXIF orientation so phone-camera images display upright.
        original_format = image.format

        # Check if image came from cache BEFORE exif_transpose (which may
        # return a new image object, losing custom attributes).
        from_cache = getattr(image, "_from_cache", False)

        # Detect whether the image carries its own EXIF orientation tag.
        # Cached PNG thumbnails strip EXIF, so we fall back to reading the
        # original file's orientation when the image itself has none.
        has_own_orientation = False
        try:
            if image.getexif().get(_EXIF_ORIENTATION_TAG):
                has_own_orientation = True
        except Exception:  # noqa: BLE001 — best-effort; never block preview
            logger.debug("Could not read EXIF orientation", exc_info=True)

        image = ImageOps.exif_transpose(image) or image

        # If the image had no EXIF of its own (likely loaded from cache),
        # recover orientation from the original source file — but ONLY if
        # the image was not loaded from cache.  Cached images already have
        # correct orientation (exif_transpose was applied during cache
        # generation), so applying it again would double-rotate.
        if not from_cache and not has_own_orientation and path is not None:
            image = _apply_exif_from_original(image, path)

        # Convert RGBA to RGB for display — alpha channel causes washed-out /
        # gray rendering in Qt's RGBA8888 format.  Composite onto the preview
        # background color so transparency is visually preserved.
        if image.mode == "RGBA":
            background = Image.new("RGB", image.size, BG_SECONDARY)
            background.paste(image, mask=image.split()[3])  # alpha as mask
            image = background
        elif image.mode != "RGB":
            image = image.convert("RGB")

        # exif_transpose returns a fresh copy that loses the .format attribute
        if image.format is None and original_format is not None:
            image.format = original_format
        self._current_image = image
        self._current_path = path

        # Convert PIL Image to QPixmap and CACHE it (avoids re-conversion on resize)
        qimage = self._pil_to_qimage(image)
        self._cached_pixmap = QPixmap.fromImage(qimage)

        # Scale to fit label
        self._update_display()

        # Update metadata (only once, not on resize)
        self._update_metadata(image, path)

    def _update_display(self) -> None:
        """Re-scale cached pixmap to fit current label size."""
        if self._cached_pixmap:
            label_size = self._image_label.size()
            scaled_pixmap = self._cached_pixmap.scaled(
                label_size,
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
            self._image_label.setPixmap(scaled_pixmap)

    def clear(self) -> None:
        """Clear the preview and metadata."""
        self._current_image = None
        self._current_path = None
        self._cached_pixmap = None
        self._original_width = None
        self._original_height = None
        self._image_label.clear()
        self._image_label.setText("No preview")
        self._filename_label.clear()
        self._dimensions_label.clear()
        self._size_label.clear()
        self._format_label.clear()

    def set_multi_preview(
        self,
        images: list[Image.Image],
        total_selected: int,
        cap: int = 9,
    ) -> None:
        """Render N-up mosaic when multiple files are selected.

        Args:
            images: List of PIL Images to display (capped at ``cap``).
            total_selected: Total number of selected files (may exceed len(images)).
            cap: Maximum number of images to show in mosaic (default 9).

        Creates a grid layout:
            - cols = ceil(sqrt(N)) where N = min(len(images), cap)
            - rows = ceil(N / cols)
            - Each cell gets the image scaled to fit
            - If total_selected > cap, show caption: "Showing {cap} of {total_selected} selected"

        Clears single-image preview state.
        """
        # Clear single-image state
        self._current_image = None
        self._current_path = None
        self._cached_pixmap = None

        if not images:
            self.clear()
            return

        # Cap the number of images to display
        display_images = images[:cap]
        n = len(display_images)

        # Calculate grid dimensions
        cols = math.ceil(math.sqrt(n))
        rows = math.ceil(n / cols)

        # Layout constants — padding around canvas edge, gap between cells
        canvas_size = 800
        canvas_padding = 8
        cell_gap = 6

        # Cell size accounts for padding on both sides and gaps between cells
        total_gap_w = cell_gap * (cols - 1)
        total_gap_h = cell_gap * (rows - 1)
        available_w = canvas_size - 2 * canvas_padding - total_gap_w
        available_h = canvas_size - 2 * canvas_padding - total_gap_h
        cell_w = available_w // cols
        cell_h = available_h // rows

        # Create the mosaic canvas (dark background)
        mosaic = Image.new("RGB", (canvas_size, canvas_size), color="#1c1b22")

        for idx, img in enumerate(display_images):
            row_i = idx // cols
            col_i = idx % cols

            # Apply EXIF orientation so phone-camera images display upright
            cell_img = ImageOps.exif_transpose(img) or img.copy()

            # Preserve aspect ratio — contain fits within cell without cropping
            cell_img = ImageOps.contain(cell_img, (cell_w, cell_h), Image.Resampling.LANCZOS)

            # Center the contained image on a cell-sized background
            cell_bg = Image.new("RGB", (cell_w, cell_h), color="#1c1b22")
            if cell_img.mode == "RGBA":
                cell_bg.paste(
                    cell_img,
                    ((cell_w - cell_img.width) // 2, (cell_h - cell_img.height) // 2),
                    cell_img,
                )
            else:
                if cell_img.mode != "RGB":
                    cell_img = cell_img.convert("RGB")
                cell_bg.paste(
                    cell_img,
                    ((cell_w - cell_img.width) // 2, (cell_h - cell_img.height) // 2),
                )
            cell_img = cell_bg

            # Convert non-RGB/RGBA modes to RGB to avoid color corruption
            if cell_img.mode == "RGBA":
                paste_mask = cell_img
            else:
                if cell_img.mode != "RGB":
                    cell_img = cell_img.convert("RGB")
                paste_mask = None

            # Position cell with padding and gap offsets
            x_offset = canvas_padding + col_i * (cell_w + cell_gap)
            y_offset = canvas_padding + row_i * (cell_h + cell_gap)

            mosaic.paste(cell_img, (x_offset, y_offset), paste_mask)

        # Convert mosaic PIL Image to QPixmap and display
        qimage = self._pil_to_qimage(mosaic)
        pixmap = QPixmap.fromImage(qimage)

        # Scale to fit label
        label_size = self._image_label.size()
        scaled_pixmap = pixmap.scaled(
            label_size,
            Qt.AspectRatioMode.KeepAspectRatio,
            Qt.TransformationMode.SmoothTransformation,
        )
        self._image_label.setPixmap(scaled_pixmap)

        # Update metadata labels for multi-select
        self._filename_label.setText(f"{total_selected} files selected")
        self._dimensions_label.clear()
        self._size_label.clear()
        self._format_label.clear()

        # Show caption if capped
        if total_selected > cap:
            self._format_label.setText(f"Showing {cap} of {total_selected} selected")

    def _update_metadata(self, image: Image.Image, path: Path | None) -> None:
        """Update metadata labels with image info."""
        logger.debug("_update_metadata: path=%s, dimensions=%s", path, image.size)
        if path:
            self._filename_label.setText(path.name)
            # File size
            try:
                size_bytes = path.stat().st_size
                self._size_label.setText(f"Size: {self._format_size(size_bytes)}")
            except OSError:
                logger.warning("Could not read file size for %s", path, exc_info=True)
                self._size_label.setText("Size: Unknown")
        else:
            self._filename_label.setText("Unknown file")
            self._size_label.setText("Size: Unknown")

        # Dimensions — use original dimensions if provided (cached thumbnails
        # are downscaled, so image.size would show the thumbnail size, not the
        # original image dimensions).
        if self._original_width is not None and self._original_height is not None:
            width = self._original_width
            height = self._original_height
        else:
            width, height = image.size
        self._dimensions_label.setText(f"Dimensions: {width} × {height}")

        # Format — always derive from original file path, not cached image.
        # Cached PNG thumbnails report format="PNG" regardless of the original
        # file type, so we prefer the path extension when available.
        if path:
            format_name = path.suffix.lstrip(".").upper()
        elif image.format:
            format_name = image.format
        else:
            format_name = "Unknown"
        self._format_label.setText(f"Format: {format_name}")

    @staticmethod
    def _format_size(size_bytes: int) -> str:
        """Format file size in human-readable form."""
        value: float = size_bytes
        for unit in ("B", "KB", "MB", "GB"):
            if value < 1024:
                return f"{value:.1f} {unit}" if unit != "B" else f"{size_bytes} {unit}"
            value /= 1024
        return f"{value:.1f} TB"

    @staticmethod
    def _pil_to_qimage(pil_image: Image.Image) -> QImage:
        """Convert PIL Image to QImage for Qt display.

        Returns a self-owned copy so the pixel data survives garbage collection.
        """
        # Ensure image is in a compatible mode and compute explicit bytesPerLine.
        # PIL's tobytes() produces tightly packed rows; Qt's 4-arg constructor
        # assumes 4-byte aligned scanlines, causing progressive shearing when
        # width × bpp is not divisible by 4.
        if pil_image.mode == "RGBA":
            mode = QImage.Format.Format_RGBA8888
            bytes_per_line = pil_image.width * 4
        elif pil_image.mode == "RGB":
            mode = QImage.Format.Format_RGB888
            bytes_per_line = pil_image.width * 3
        else:
            # Convert to RGB for compatibility
            pil_image = pil_image.convert("RGB")
            mode = QImage.Format.Format_RGB888
            bytes_per_line = pil_image.width * 3

        data = pil_image.tobytes()
        qimage = QImage(data, pil_image.width, pil_image.height, bytes_per_line, mode)
        # .copy() creates a deep copy so pixel data outlives the local ``data`` bytes
        return qimage.copy()

    def resizeEvent(self, event: QResizeEvent) -> None:  # noqa: N802
        """Re-scale image when panel is resized — NO re-conversion, NO disk I/O."""
        super().resizeEvent(event)
        self._update_display()
