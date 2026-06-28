"""Preview panel widget — displays single image preview with metadata and mosaic multi-preview."""

from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageOps
from PySide6.QtCore import Qt
from PySide6.QtGui import QImage, QPixmap, QResizeEvent
from PySide6.QtWidgets import QLabel, QSizePolicy, QVBoxLayout, QWidget

from tarragon.theme.tokens import get_token

# Theme tokens (coral-amber dark palette) — sourced from centralized theme system
BG_PRIMARY: str = get_token("colors", "bg_primary")
BG_SECONDARY: str = get_token("colors", "bg_secondary")
TEXT_PRIMARY: str = get_token("colors", "text_primary")
TEXT_SECONDARY: str = get_token("colors", "text_secondary")
CORAL_STRONG: str = get_token("colors", "coral_strong")


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

    def _setup_ui(self) -> None:
        """Build the UI layout."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(8)

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
            f"  font-size: 12px;"
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

    def set_image(self, image: Image.Image, path: Path | None = None) -> None:
        """Set the image to display.

        Args:
            image: PIL Image to display
            path: Optional file path for metadata display

        Raises:
            TypeError: If ``image`` is None.
        """
        if image is None:
            raise TypeError("image must be a PIL Image, not None")

        # Apply EXIF orientation so phone-camera images display upright
        original_format = image.format
        image = ImageOps.exif_transpose(image) or image
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

        # Canvas size — use a fixed reasonable size for the mosaic
        canvas_size = 800
        cell_w = canvas_size // cols
        cell_h = canvas_size // rows

        # Create the mosaic canvas (dark background)
        mosaic = Image.new("RGB", (cols * cell_w, rows * cell_h), color="#1c1b22")

        for idx, img in enumerate(display_images):
            row_i = idx // cols
            col_i = idx % cols

            # Resize image to fit cell while maintaining aspect ratio
            cell_img = img.copy()
            cell_img.thumbnail((cell_w, cell_h), Image.Resampling.LANCZOS)

            # Center in cell
            x_offset = col_i * cell_w + (cell_w - cell_img.width) // 2
            y_offset = row_i * cell_h + (cell_h - cell_img.height) // 2

            # Paste (handle RGBA images)
            if cell_img.mode == "RGBA":
                mosaic.paste(cell_img, (x_offset, y_offset), cell_img)
            else:
                mosaic.paste(cell_img, (x_offset, y_offset))

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
        if path:
            self._filename_label.setText(path.name)
            # File size
            try:
                size_bytes = path.stat().st_size
                self._size_label.setText(f"Size: {self._format_size(size_bytes)}")
            except OSError:
                self._size_label.setText("Size: Unknown")
        else:
            self._filename_label.setText("Unknown file")
            self._size_label.setText("Size: Unknown")

        # Dimensions
        width, height = image.size
        self._dimensions_label.setText(f"Dimensions: {width} × {height}")

        # Format — use PIL format if available, else derive from path extension
        if image.format:
            format_name = image.format
        elif path:
            format_name = path.suffix.lstrip(".").upper()
        else:
            format_name = "Unknown"
        self._format_label.setText(f"Format: {format_name}")

    @staticmethod
    def _format_size(size_bytes: int) -> str:
        """Format file size in human-readable form."""
        for unit in ("B", "KB", "MB", "GB"):
            if size_bytes < 1024:
                return f"{size_bytes:.1f} {unit}" if unit != "B" else f"{size_bytes} {unit}"
            size_bytes /= 1024
        return f"{size_bytes:.1f} TB"

    @staticmethod
    def _pil_to_qimage(pil_image: Image.Image) -> QImage:
        """Convert PIL Image to QImage for Qt display.

        Returns a self-owned copy so the pixel data survives garbage collection.
        """
        # Ensure image is in a compatible mode
        if pil_image.mode == "RGBA":
            mode = QImage.Format.Format_RGBA8888
        elif pil_image.mode == "RGB":
            mode = QImage.Format.Format_RGB888
        else:
            # Convert to RGB for compatibility
            pil_image = pil_image.convert("RGB")
            mode = QImage.Format.Format_RGB888

        data = pil_image.tobytes()
        qimage = QImage(data, pil_image.width, pil_image.height, mode)
        # .copy() creates a deep copy so pixel data outlives the local ``data`` bytes
        return qimage.copy()

    def resizeEvent(self, event: QResizeEvent) -> None:  # noqa: N802
        """Re-scale image when panel is resized — NO re-conversion, NO disk I/O."""
        super().resizeEvent(event)
        self._update_display()
