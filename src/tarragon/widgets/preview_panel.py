"""Preview panel widget — displays single image preview with metadata, mosaic multi-preview, and tag management."""

from __future__ import annotations

import logging
import math
from collections.abc import Callable
from pathlib import Path
from typing import Any

from PIL import Image, ImageOps
from PySide6.QtCore import QEvent, QSize, Qt, Signal
from PySide6.QtGui import QEnterEvent, QImage, QMouseEvent, QPixmap, QResizeEvent
from PySide6.QtWidgets import (
    QGraphicsOpacityEffect,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMenu,
    QPushButton,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from tarragon.db import normalize_path
from tarragon.services.tag_service import TagService
from tarragon.widgets.flow_layout import FlowLayout
from tarragon.theme.color_buckets import BUCKET_COLORS, BUCKET_HEX_COLORS
from tarragon.theme.spacing import SM, XS
from tarragon.theme.tokens import get_token

logger = logging.getLogger(__name__)

# EXIF orientation tag ID
_EXIF_ORIENTATION_TAG = 0x0112

# Theme tokens (coral-amber dark palette) — sourced from centralized theme system
BG_SECONDARY: str = get_token("colors", "bg_secondary")


def _apply_exif_from_original(image: Image.Image, original_path: Path) -> Image.Image:
    """Apply EXIF orientation from the original file to a cached image.

    Cached PNG thumbnails strip EXIF metadata, so ``ImageOps.exif_transpose()``
    is a no-op on them.  This helper reads the orientation tag from the
    *original* source file and applies the corresponding geometric
    transformation to *image* so that old caches still display correctly.

    WHY NOT ``ImageOps.exif_transpose()``?
    That function reads the orientation tag from the image object itself.
    Here we need to apply orientation from a *different* file (the original
    source) to a cached thumbnail dat has no EXIF data.  There is no way to
    tell ``exif_transpose()`` "use dis orientation value" — it only looks at
    the image's own EXIF.  Injecting EXIF into the cached image just to call
    it would be more complex and error-prone dan da manual mapping below.

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

    NOTE: This manual implementation exists because ``ImageOps.exif_transpose()``
    reads the orientation tag from the image *itself*, but we need to apply
    orientation read from a *different* file (the original source) to a cached
    thumbnail that has no EXIF data.  We cannot inject EXIF into the cached
    image and call ``exif_transpose()`` — that would be more complex and fragile
    than this straightforward mapping.  Do NOT replace dis wiv ``exif_transpose``.
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


class _ClickableLabel(QLabel):
    """A QLabel subclass dat emits a ``clicked`` signal on mouse press.

    Replaces da monkey-patched ``mousePressEvent`` approach wiv a proper
    Qt event-chain override, avoiding fragile instance-level patching.
    """

    clicked = Signal()

    def mousePressEvent(self, event: QMouseEvent) -> None:  # noqa: N802
        """Emit ``clicked`` an' propagate da event up da Qt chain."""
        self.clicked.emit()
        super().mousePressEvent(event)


class _TagPillWidget(QWidget):
    """A tag pill widget with a hover-revealed remove (×) button.

    Contains a QLabel for the tag name and a small QPushButton ("×") that
    is hidden by default and shown when the mouse enters the widget.
    Clicking the × button removes the tag; clicking the pill body toggles it.
    """

    def __init__(
        self,
        tag_name: str,
        on_remove: Callable[[], None],
        on_toggle: Callable[[], None],
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._tag_name = tag_name
        self._on_remove = on_remove
        self._on_toggle = on_toggle

        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)

        self._label = _ClickableLabel(tag_name)
        self._label.setCursor(Qt.CursorShape.PointingHandCursor)
        self._label.clicked.connect(self._on_toggle)
        layout.addWidget(self._label)

        self._remove_btn = QPushButton("×")
        self._remove_btn.setFixedSize(16, 16)
        self._remove_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._remove_btn.setObjectName("tagPillRemoveBtn")
        self._remove_btn.hide()
        self._remove_btn.clicked.connect(lambda _checked=False: self._on_remove())
        layout.addWidget(self._remove_btn)

        self.setMouseTracking(True)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self._label.setMouseTracking(True)
        self._remove_btn.setMouseTracking(True)

    def text(self) -> str:
        """Return the tag name displayed by this pill."""
        return self._tag_name

    def enterEvent(self, event: QEnterEvent) -> None:  # noqa: N802
        """Show the remove button on hover."""
        self._remove_btn.show()
        super().enterEvent(event)

    def leaveEvent(self, event: QEvent) -> None:  # noqa: N802
        """Hide the remove button when mouse leaves."""
        self._remove_btn.hide()
        super().leaveEvent(event)


class PreviewPanel(QWidget):
    """Widget that displays a single image preview with metadata and tag management.

    Shows:
    - Scaled image (maintains aspect ratio, fits panel)
    - Filename
    - Dimensions (width × height)
    - File size (formatted)
    - Format (JPEG, PNG, PSD, etc.)
    - Tag pills (clickable to toggle, with add/dropdown/create-new)
    """

    #: Emitted when tags change on the selected files (triggers gallery refresh).
    tags_changed = Signal()

    def __init__(
        self,
        tag_service: TagService | None = None,
        parent: QWidget | None = None,
    ) -> None:
        """Initialize the preview panel.

        Args:
            tag_service: TagService for tag CRUD operations. When None, tag
                management features are disabled (display-only mode).
            parent: Optional parent widget.
        """
        super().__init__(parent)
        self._tag_service = tag_service
        self._current_tags: list[dict[str, Any]] = []
        self._selected_paths: list[str] = []
        self._cached_file_tags: dict[str, set[int]] = {}
        self._tag_input: QLineEdit | None = None
        self._setup_ui()
        self._current_image: Image.Image | None = None
        self._current_path: Path | None = None
        self._cached_pixmap: QPixmap | None = None
        self._original_width: int | None = None
        self._original_height: int | None = None

        # React to external tag changes (e.g. from thumbnail auto-color)
        if self._tag_service is not None:
            self._tag_service.tagsChanged.connect(self._on_external_tags_changed)

    def _setup_ui(self) -> None:
        """Build the UI layout."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(SM, SM, SM, SM)
        layout.setSpacing(SM)

        # ── Image label (centered, scaled) ────────────────────────────
        self._image_label = QLabel()
        self._image_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._image_label.setMinimumSize(200, 200)
        self._image_label.setSizePolicy(
            QSizePolicy.Policy.Ignored,
            QSizePolicy.Policy.Ignored,
        )
        self._image_label.setText("No preview")
        self._image_label.setObjectName("previewImageLabel")
        layout.addWidget(self._image_label, stretch=1)

        # ── Metadata section ──────────────────────────────────────────
        self._metadata_header = QLabel("Metadata")
        self._metadata_header.setObjectName("previewSectionHeader")
        layout.addWidget(self._metadata_header)

        self._metadata_grid = QGridLayout()
        self._metadata_grid.setHorizontalSpacing(SM)
        self._metadata_grid.setVerticalSpacing(2)

        # Key labels (left column — muted)
        self._filename_key = QLabel("File")
        self._filename_key.setObjectName("previewMetaLabel")
        self._dimensions_key = QLabel("Dimensions")
        self._dimensions_key.setObjectName("previewMetaLabel")
        self._size_key = QLabel("Size")
        self._size_key.setObjectName("previewMetaLabel")
        self._format_key = QLabel("Format")
        self._format_key.setObjectName("previewMetaLabel")

        # Value labels (right column — tertiary)
        self._filename_label = QLabel()
        self._filename_label.setObjectName("previewMetaValue")
        self._filename_label.setWordWrap(True)
        self._dimensions_label = QLabel()
        self._dimensions_label.setObjectName("previewMetaValue")
        self._size_label = QLabel()
        self._size_label.setObjectName("previewMetaValue")
        self._format_label = QLabel()
        self._format_label.setObjectName("previewMetaValue")

        self._metadata_grid.addWidget(self._filename_key, 0, 0)
        self._metadata_grid.addWidget(self._filename_label, 0, 1)
        self._metadata_grid.addWidget(self._dimensions_key, 1, 0)
        self._metadata_grid.addWidget(self._dimensions_label, 1, 1)
        self._metadata_grid.addWidget(self._size_key, 2, 0)
        self._metadata_grid.addWidget(self._size_label, 2, 1)
        self._metadata_grid.addWidget(self._format_key, 3, 0)
        self._metadata_grid.addWidget(self._format_label, 3, 1)
        self._metadata_grid.setColumnStretch(1, 1)
        layout.addLayout(self._metadata_grid)

        # ── Tags section ──────────────────────────────────────────────
        self._tags_header = QLabel("Tags")
        self._tags_header.setObjectName("previewSectionHeader")
        layout.addWidget(self._tags_header)

        # Color squares row — 10 colored squares, always visible
        self._color_squares_container = QWidget()
        self._color_squares_layout = QHBoxLayout(self._color_squares_container)
        self._color_squares_layout.setContentsMargins(0, 0, 0, 0)
        self._color_squares_layout.setSpacing(XS)
        self._color_square_buttons: dict[str, QPushButton] = {}
        for bucket_name in BUCKET_COLORS:
            btn = self._create_color_square_button(bucket_name)
            self._color_square_buttons[bucket_name] = btn
            self._color_squares_layout.addWidget(btn)
        self._color_squares_layout.addStretch()
        self._color_squares_container.setMinimumHeight(self._COLOR_SQUARE_SIZE)
        layout.addWidget(self._color_squares_container)

        self._tags_container = QWidget()
        self._tags_container.setMinimumHeight(36)  # ensure visible even when empty (one row of pills)
        self._tags_flow = FlowLayout(self._tags_container, spacing=4)
        layout.addWidget(self._tags_container)

        self._add_tag_btn = QPushButton("+ add")
        self._add_tag_btn.setObjectName("previewAddTagBtn")
        self._add_tag_btn.setSizePolicy(QSizePolicy.Policy.Maximum, QSizePolicy.Policy.Fixed)
        self._add_tag_btn.clicked.connect(self._on_add_tag_clicked)
        layout.addWidget(self._add_tag_btn, alignment=Qt.AlignmentFlag.AlignLeft)

        # Internal tracking for tag pill widgets
        self._tag_pills: list[_TagPillWidget] = []

        layout.addStretch()

        self.setObjectName("previewPanel")

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
        self._current_tags = []
        self._selected_paths = []
        self._cached_file_tags = {}
        self._image_label.clear()
        self._image_label.setText("No preview")
        self._filename_label.clear()
        self._dimensions_label.clear()
        self._size_label.clear()
        self._format_label.clear()
        self._clear_tag_pills()
        # Reset all color squares to inactive
        self._update_color_squares(set())

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
        mosaic = Image.new("RGB", (canvas_size, canvas_size), color=BG_SECONDARY)

        for idx, img in enumerate(display_images):
            row_i = idx // cols
            col_i = idx % cols

            # Apply EXIF orientation so phone-camera images display upright
            cell_img = ImageOps.exif_transpose(img) or img.copy()

            # Preserve aspect ratio — contain fits within cell without cropping
            cell_img = ImageOps.contain(cell_img, (cell_w, cell_h), Image.Resampling.LANCZOS)

            # Center the contained image on a cell-sized background
            cell_bg = Image.new("RGB", (cell_w, cell_h), color=BG_SECONDARY)
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

        # Hide key labels for multi-select (keys are self-explanatory)
        for key_label in (
            self._filename_key,
            self._dimensions_key,
            self._size_key,
            self._format_key,
        ):
            key_label.hide()

        # Show caption if capped
        if total_selected > cap:
            self._format_label.setText(f"Showing {cap} of {total_selected} selected")

    def _update_metadata(self, image: Image.Image, path: Path | None) -> None:
        """Update metadata labels with image info."""
        logger.debug("_update_metadata: path=%s, dimensions=%s", path, image.size)

        # Show key labels for single-image view
        for key_label in (
            self._filename_key,
            self._dimensions_key,
            self._size_key,
            self._format_key,
        ):
            key_label.show()

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

    # ── Tag display & management ────────────────────────────────────────────

    def set_tags(
        self,
        tags: list[dict[str, Any]],
        selected_paths: list[str] | None = None,
    ) -> None:
        """Populate the tag pills area with interactive tag display.

        Args:
            tags: List of tag dicts, each with at least ``{"id": int, "name": str}``.
                Tags with ``source == "auto_color"`` update the color squares row.
                Tags with ``source == "user"`` create pills in the flow layout.
            selected_paths: Currently selected file paths. When provided with
                multiple paths, tri-state opacity is applied to pills and color
                squares based on how many selected files carry each tag.
        """
        self._current_tags = tags
        if selected_paths is not None:
            # Normalize path separators so lookups in _cached_file_tags
            # (keyed by forward-slash paths from the DB) match consistently.
            self._selected_paths = [normalize_path(p) for p in selected_paths]
            # Batch-fetch tag IDs for tri-state calculation
            if self._tag_service is not None:
                self._cached_file_tags = self._tag_service.get_file_tag_ids_batch(
                    self._selected_paths,
                )
            else:
                self._cached_file_tags = {}
        self._clear_tag_pills()

        # Split tags by source: color tags → squares, user tags → pills
        color_tag_names: set[str] = set()
        for tag in tags:
            source = tag.get("source", "user")
            if source == "auto_color":
                # Extract bare bucket name from "color:<name>"
                tag_name = tag.get("name", "")
                bare_name = tag_name.removeprefix("color:") if tag_name.startswith("color:") else tag_name
                color_tag_names.add(bare_name)
            else:
                pill = self._create_tag_pill(tag)
                self._tag_pills.append(pill)
                self._tags_flow.addWidget(pill)

        # Update color squares (always show all 10, with appropriate opacity)
        self._update_color_squares(color_tag_names)

        # Force layout recalculation so the tags container resizes to fit
        self._tags_container.updateGeometry()

    def _create_tag_pill(self, tag: dict[str, Any]) -> _TagPillWidget:
        """Create a clickable tag pill widget with tri-state opacity support.

        Args:
            tag: Tag dict with ``"id"``, ``"name"``, and optional ``"source"``.

        Returns:
            A _TagPillWidget styled as a tag pill with hover-X removal.
            For multi-selection, partial tags are shown at half opacity.
        """
        tag_name = str(tag.get("name", ""))

        pill = _TagPillWidget(
            tag_name=tag_name,
            on_remove=lambda t=tag: self._on_tag_remove_clicked(t),
            on_toggle=lambda t=tag: self._on_tag_pill_clicked(t),
        )
        # All pills in the flow layout are user-created (primary role)
        pill.setProperty("tagRole", "primary")
        pill._label.setProperty("tagRole", "primary")  # noqa: SLF001 — styling the inner label

        # Tri-state opacity for multi-selection
        if len(self._selected_paths) > 1 and self._cached_file_tags:
            tag_id = tag.get("id")
            if tag_id is not None:
                files_with_tag = sum(
                    1 for path in self._selected_paths if tag_id in self._cached_file_tags.get(path, set())
                )
                total = len(self._selected_paths)
                if files_with_tag == 0:
                    effect = QGraphicsOpacityEffect(pill)
                    effect.setOpacity(0.3)
                    pill.setGraphicsEffect(effect)
                elif files_with_tag < total:
                    effect = QGraphicsOpacityEffect(pill)
                    effect.setOpacity(0.5)
                    pill.setGraphicsEffect(effect)
                else:
                    pill.setGraphicsEffect(None)

        return pill

    def _on_tag_pill_clicked(self, tag: dict[str, Any]) -> None:
        """Handle click on a tag pill — toggle tag on/off for selected files.

        If all selected files have the tag, remove it from all.
        Otherwise, add it to all selected files.

        Args:
            tag: The tag dict that was clicked.
        """
        if not self._selected_paths or self._tag_service is None:
            return

        tag_id = tag.get("id")
        if tag_id is None:
            return

        # Check if all selected files have this tag
        all_have_tag = all(tag_id in self._cached_file_tags.get(path, set()) for path in self._selected_paths)

        if all_have_tag:
            # Remove tag from all selected files
            self._tag_service.remove_tags_from_files(
                self._selected_paths,
                {tag_id},
            )
        else:
            # Add tag to all selected files
            tag_name = tag.get("name", "")
            if tag_name:
                self._tag_service.add_tags_to_files(
                    self._selected_paths,
                    [tag_name],
                )
        # tagsChanged signal from service triggers _on_external_tags_changed

    def _on_tag_remove_clicked(self, tag: dict[str, Any]) -> None:
        """Handle click on the × button of a tag pill — remove tag from files.

        Args:
            tag: The tag dict to remove.
        """
        if not self._selected_paths or self._tag_service is None:
            return

        tag_id = tag.get("id")
        if tag_id is None:
            return

        self._tag_service.remove_tags_from_files(
            self._selected_paths,
            {tag_id},
        )
        # tagsChanged signal from service triggers _on_external_tags_changed

    # ── Color squares ────────────────────────────────────────────────────────

    _COLOR_SQUARE_SIZE = 24

    def _create_color_square_button(self, bucket_name: str) -> QPushButton:
        """Create a single color square button for a color bucket.

        Args:
            bucket_name: The color bucket name (e.g. "red", "blue").

        Returns:
            A QPushButton styled as a colored square.
        """
        hex_color = BUCKET_HEX_COLORS[bucket_name]
        btn = QPushButton()
        btn.setFixedSize(self._COLOR_SQUARE_SIZE, self._COLOR_SQUARE_SIZE)
        btn.setCursor(Qt.CursorShape.PointingHandCursor)
        btn.setProperty("colorSquare", True)
        btn.setToolTip(f"color:{bucket_name}")
        # background-color is dynamic (unique per color bucket) and cannot be
        # expressed as a QSS rule.  Border, border-radius, and hover styles are
        # handled by the QSS rule for QPushButton[colorSquare="true"].
        btn.setStyleSheet(
            f"QPushButton {{ background-color: {hex_color}; }}"
        )
        btn.clicked.connect(
            lambda _checked=False, name=bucket_name: self._on_color_square_clicked(name),
        )
        # Start inactive (low opacity)
        effect = QGraphicsOpacityEffect(btn)
        effect.setOpacity(0.3)
        btn.setGraphicsEffect(effect)
        return btn

    def _update_color_squares(self, active_color_names: set[str]) -> None:
        """Update the opacity of color squares based on which colors are active.

        Uses tri-state logic for multi-selection:
        - All files have the color → opacity 1.0
        - Some files have the color → opacity 0.5
        - No files have the color → opacity 0.3

        Args:
            active_color_names: Set of bare bucket names (e.g. {"red", "blue"})
                that are present on the current file(s).
        """
        for bucket_name, btn in self._color_square_buttons.items():
            if len(self._selected_paths) > 1 and self._cached_file_tags:
                # Tri-state for multi-selection
                color_tag_name = f"color:{bucket_name}"
                # Find the tag ID for this color tag
                color_tag_id: int | None = None
                for tag in self._current_tags:
                    if tag.get("name") == color_tag_name:
                        color_tag_id = tag.get("id")
                        break

                if color_tag_id is not None:
                    files_with_tag = sum(
                        1
                        for path in self._selected_paths
                        if color_tag_id in self._cached_file_tags.get(path, set())
                    )
                    total = len(self._selected_paths)
                    if files_with_tag == total:
                        opacity = 1.0
                    elif files_with_tag > 0:
                        opacity = 0.5
                    else:
                        opacity = 0.3
                else:
                    opacity = 0.3
            elif bucket_name in active_color_names:
                opacity = 1.0
            else:
                opacity = 0.3

            effect = btn.graphicsEffect()
            if not isinstance(effect, QGraphicsOpacityEffect):
                effect = QGraphicsOpacityEffect(btn)
                btn.setGraphicsEffect(effect)
            effect.setOpacity(opacity)

        # Force all buttons to repaint to invalidate QGraphicsOpacityEffect caches,
        # which otherwise cause buttons to render at stale positions after resize.
        self._color_squares_container.updateGeometry()
        for btn in self._color_square_buttons.values():
            btn.update()

    def _on_color_square_clicked(self, bucket_name: str) -> None:
        """Handle click on a color square — toggle the color tag on/off.

        If all selected files have the color tag, remove it from all.
        Otherwise, add it to all selected files.

        Args:
            bucket_name: The color bucket name (e.g. "red").
        """
        logger.debug("Color square clicked: bucket=%s, paths=%d", bucket_name, len(self._selected_paths))

        if not self._selected_paths or self._tag_service is None:
            return

        color_tag_name = f"color:{bucket_name}"

        # Find the tag ID for this color tag
        color_tag_id: int | None = None
        for tag in self._current_tags:
            if tag.get("name") == color_tag_name:
                color_tag_id = tag.get("id")
                break

        # Check if all selected files have this color tag
        all_have_tag = False
        if color_tag_id is not None:
            all_have_tag = all(
                color_tag_id in self._cached_file_tags.get(path, set())
                for path in self._selected_paths
            )

        if all_have_tag and color_tag_id is not None:
            self._tag_service.remove_tags_from_files(
                self._selected_paths,
                {color_tag_id},
            )
        else:
            self._tag_service.add_tags_to_files(
                self._selected_paths,
                [color_tag_name],
                source="auto_color",
            )
        # tagsChanged signal from service triggers _on_external_tags_changed

    def _on_add_tag_clicked(self) -> None:
        """Show dropdown menu of existing custom tags + 'Create new...' option.

        Color tags (name starting with 'color:') are excluded from the dropdown.
        """
        if self._tag_service is None or not self._selected_paths:
            return

        all_tags = self._tag_service.get_all_tags(folder_path=None)
        existing_tag_ids = {t["id"] for t in self._current_tags}

        menu = QMenu(self)

        # Add existing custom tags not already on the selected files
        # (filter out color tags — they're managed via the color squares)
        has_addable = False
        for tag in all_tags:
            if tag["id"] not in existing_tag_ids and not tag["name"].startswith("color:"):
                action = menu.addAction(tag["name"])
                action.setData(tag)
                has_addable = True

        if has_addable:
            menu.addSeparator()

        # "Create new..." option
        create_action = menu.addAction("Create new...")
        create_action.setData("create_new")

        # Show menu below the add button
        action = menu.exec(
            self._add_tag_btn.mapToGlobal(
                self._add_tag_btn.rect().bottomLeft(),
            ),
        )

        if action is None:
            return

        if action.data() == "create_new":
            self._show_inline_tag_input()
        elif action.data() is not None:
            # Add selected tag to files
            tag_data = action.data()
            tag_name = tag_data.get("name", "") if isinstance(tag_data, dict) else ""
            if tag_name:
                self._tag_service.add_tags_to_files(
                    self._selected_paths,
                    [tag_name],
                )

    def _show_inline_tag_input(self) -> None:
        """Replace the add button with an inline text input for new tag creation."""
        self._add_tag_btn.hide()

        self._tag_input = QLineEdit()
        self._tag_input.setObjectName("previewTagInput")
        self._tag_input.setPlaceholderText("Tag name...")
        self._tag_input.returnPressed.connect(self._on_tag_input_submitted)
        self._tag_input.editingFinished.connect(self._on_tag_input_finished)

        # Add input to layout (appears where add button was, before the stretch)
        main_layout = self.layout()
        if isinstance(main_layout, QVBoxLayout):
            # Insert before the stretch (last item)
            count = main_layout.count()
            main_layout.insertWidget(max(0, count - 1), self._tag_input)
        elif main_layout is not None:
            main_layout.addWidget(self._tag_input)

        self._tag_input.setFocus()

    def _on_tag_input_submitted(self) -> None:
        """Handle Enter in the inline tag input — create tag and add to files."""
        if self._tag_input is None:
            return
        tag_name = self._tag_input.text().strip()
        if tag_name and self._selected_paths and self._tag_service is not None:
            self._tag_service.add_tags_to_files(self._selected_paths, [tag_name])
        self._on_tag_input_finished()

    def _on_tag_input_finished(self) -> None:
        """Remove the inline input and restore the add button."""
        if self._tag_input is None:
            return
        main_layout = self.layout()
        if main_layout is not None:
            main_layout.removeWidget(self._tag_input)
        self._tag_input.deleteLater()
        self._tag_input = None
        self._add_tag_btn.show()

    def _on_external_tags_changed(self) -> None:
        """Refresh tag display when tags change externally (e.g. auto-color)."""
        if self._selected_paths and self._tag_service is not None:
            # Re-fetch tags for the current selection
            if len(self._selected_paths) == 1:
                tags = self._tag_service.get_tags_for_file(self._selected_paths[0])
            else:
                tags = self.get_union_tags(self._selected_paths)
            self.set_tags(tags, selected_paths=self._selected_paths)
        elif not self._selected_paths:
            self._clear_tag_pills()
            self._current_tags = []
        # Emit signal so main window can refresh gallery if needed
        self.tags_changed.emit()

    def get_union_tags(self, paths: list[str]) -> list[dict[str, Any]]:
        """Get the union of tags across multiple file paths.

        Args:
            paths: List of file paths to get tags for.

        Returns:
            List of unique tag dicts from all paths.
        """
        if self._tag_service is None:
            return []

        # Single pass: query each file once and cache tag info (id → name/source).
        # This avoids the previous O(n²) pattern where get_tags_for_file was
        # called again in the inner loop to find the source for each tag_id.
        tag_info: dict[int, dict[str, Any]] = {}
        for path in paths:
            tags = self._tag_service.get_tags_for_file(path)
            for t in tags:
                tid = t["id"]
                if tid not in tag_info:
                    tag_info[tid] = {
                        "name": t.get("name", ""),
                        "source": t.get("source", "user"),
                    }

        union_tags: list[dict[str, Any]] = []
        for tag_id in sorted(tag_info):
            name = tag_info[tag_id]["name"]
            if not name:
                # Fall back to service lookup when name wasn't in the tag dict
                name = self._tag_service.get_tag_name(tag_id)
            if name:
                union_tags.append({
                    "id": tag_id,
                    "name": name,
                    "source": tag_info[tag_id]["source"],
                })
        return union_tags

    def _clear_tag_pills(self) -> None:
        """Remove all tag pill widgets from the flow layout."""
        for pill in self._tag_pills:
            self._tags_flow.removeWidget(pill)
            pill.deleteLater()
        self._tag_pills.clear()
        # Force layout recalculation so the tags container shrinks back
        self._tags_container.updateGeometry()

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
