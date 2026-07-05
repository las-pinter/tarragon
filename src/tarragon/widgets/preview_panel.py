"""Preview panel widget — displays single image preview with metadata, mosaic multi-preview, and tag management."""

from __future__ import annotations

import logging
import math
from pathlib import Path
from typing import Any

from PIL import Image, ImageOps
from PySide6.QtCore import QRect, QSize, Qt, Signal
from PySide6.QtGui import QImage, QPixmap, QResizeEvent
from PySide6.QtWidgets import (
    QGridLayout,
    QLabel,
    QLayout,
    QLayoutItem,
    QLineEdit,
    QMenu,
    QPushButton,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from tarragon.services.tag_service import TagService
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


class _FlowLayout(QLayout):
    """Simple left-to-right flow layout that wraps widgets to the next row.

    Used for tag pills in the preview panel metadata area.
    """

    def __init__(self, parent: QWidget | None = None, spacing: int = 4) -> None:
        super().__init__(parent)
        self._items: list[QLayoutItem] = []
        self._spacing = spacing

    def addItem(self, item: QLayoutItem) -> None:  # noqa: N802
        self._items.append(item)

    def count(self) -> int:
        return len(self._items)

    def itemAt(self, index: int) -> QLayoutItem | None:  # noqa: N802
        if 0 <= index < len(self._items):
            return self._items[index]
        return None

    def takeAt(self, index: int) -> QLayoutItem | None:  # noqa: N802
        if 0 <= index < len(self._items):
            return self._items.pop(index)
        return None

    def expandingDirections(self) -> Qt.Orientation:  # noqa: N802
        return Qt.Orientation(0)

    def hasHeightForWidth(self) -> bool:  # noqa: N802
        return True

    def heightForWidth(self, width: int) -> int:  # noqa: N802
        return self._do_layout(width, dry_run=True)

    def sizeHint(self) -> QSize:  # noqa: N802
        return self.minimumSizeHint()

    def minimumSizeHint(self) -> QSize:  # noqa: N802
        if not self._items:
            # Return a size for one row so the container doesn't collapse to zero
            return QSize(0, 36)
        # At minimum, fit the widest single item
        max_w = 0
        for item in self._items:
            wid = item.widget()
            if wid is not None:
                max_w = max(max_w, wid.sizeHint().width())
        margins = self.contentsMargins()
        total_h = self._do_layout(max_w + margins.left() + margins.right(), dry_run=True)
        return QSize(max_w, total_h)

    def setGeometry(self, rect: QRect) -> None:  # noqa: N802
        super().setGeometry(rect)
        self._do_layout(rect.width(), dry_run=False)

    def _do_layout(self, width: int, dry_run: bool) -> int:
        """Position items in rows, wrapping when *width* is exceeded.

        Returns the total height needed for all rows.
        """
        margins = self.contentsMargins()
        effective_width = width - margins.left() - margins.right()
        x = 0
        y = 0
        row_height = 0

        for item in self._items:
            wid = item.widget()
            if wid is None:
                continue
            hint = wid.sizeHint()
            next_x = x + hint.width() + self._spacing

            if next_x - self._spacing > effective_width and x > 0:
                x = 0
                y += row_height + self._spacing
                row_height = 0
                next_x = hint.width() + self._spacing

            if not dry_run:
                wid.move(margins.left() + x, margins.top() + y)

            x = next_x
            row_height = max(row_height, hint.height())

        return y + row_height + margins.top() + margins.bottom()


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
        self._image_label.setStyleSheet(
            f"QLabel {{"
            f"  background-color: {BG_SECONDARY};"
            f"  border: none;"
            f"  border-radius: 8px;"
            f"  color: {TEXT_SECONDARY};"
            f"  font-size: {BODY_SIZE}px;"
            f"}}"
        )
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

        self._tags_container = QWidget()
        self._tags_container.setMinimumHeight(36)  # ensure visible even when empty (one row of pills)
        self._tags_flow = _FlowLayout(self._tags_container, spacing=4)
        layout.addWidget(self._tags_container)

        self._add_tag_btn = QPushButton("+ add")
        self._add_tag_btn.setObjectName("previewAddTagBtn")
        self._add_tag_btn.clicked.connect(self._on_add_tag_clicked)
        layout.addWidget(self._add_tag_btn)

        # Internal tracking for tag pill widgets
        self._tag_pills: list[QLabel] = []

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
                An optional ``"source"`` key (``"user"`` → primary pill,
                anything else → secondary pill) controls the visual role.
            selected_paths: Currently selected file paths. When provided with
                multiple paths, tri-state opacity is applied to pills based on
                how many selected files carry each tag.
        """
        self._current_tags = tags
        if selected_paths is not None:
            self._selected_paths = selected_paths
            # Batch-fetch tag IDs for tri-state calculation
            if self._tag_service is not None and len(selected_paths) > 1:
                self._cached_file_tags = self._tag_service.get_file_tag_ids_batch(
                    selected_paths,
                )
            else:
                self._cached_file_tags = {}
        self._clear_tag_pills()

        for tag in tags:
            pill = self._create_tag_pill(tag)
            self._tag_pills.append(pill)
            self._tags_flow.addWidget(pill)

        # Force layout recalculation so the tags container resizes to fit
        self._tags_container.updateGeometry()

    def _create_tag_pill(self, tag: dict[str, Any]) -> QLabel:
        """Create a clickable tag pill label with tri-state opacity support.

        Args:
            tag: Tag dict with ``"id"``, ``"name"``, and optional ``"source"``.

        Returns:
            A QLabel styled as a tag pill. For multi-selection, partial tags
            are shown at half opacity.
        """
        pill = QLabel(str(tag.get("name", "")))
        role = self._tag_role(tag)
        pill.setProperty("tagRole", role)

        # Tri-state opacity for multi-selection
        if len(self._selected_paths) > 1 and self._cached_file_tags:
            tag_id = tag.get("id")
            if tag_id is not None:
                files_with_tag = sum(
                    1 for path in self._selected_paths if tag_id in self._cached_file_tags.get(path, set())
                )
                total = len(self._selected_paths)
                if files_with_tag == 0:
                    pill.setStyleSheet("opacity: 0.3;")
                elif files_with_tag < total:
                    pill.setStyleSheet("opacity: 0.5;")
                # else: full opacity (all files have tag) — default QSS

        pill.setCursor(Qt.CursorShape.PointingHandCursor)
        pill.mousePressEvent = lambda _e, t=tag: self._on_tag_pill_clicked(t)  # type: ignore[assignment]
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

    def _on_add_tag_clicked(self) -> None:
        """Show dropdown menu of existing tags + 'Create new...' option."""
        if self._tag_service is None or not self._selected_paths:
            return

        all_tags = self._tag_service.get_all_tags(folder_path=None)
        existing_tag_ids = {t["id"] for t in self._current_tags}

        menu = QMenu(self)

        # Add existing tags not already on the selected files
        has_addable = False
        for tag in all_tags:
            if tag["id"] not in existing_tag_ids:
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
                tags = self._get_union_tags(self._selected_paths)
            self.set_tags(tags, selected_paths=self._selected_paths)
        elif not self._selected_paths:
            self._clear_tag_pills()
            self._current_tags = []
        # Emit signal so main window can refresh gallery if needed
        self.tags_changed.emit()

    def _get_union_tags(self, paths: list[str]) -> list[dict[str, Any]]:
        """Get the union of tags across multiple file paths.

        Args:
            paths: List of file paths to get tags for.

        Returns:
            List of unique tag dicts from all paths.
        """
        if self._tag_service is None:
            return []
        all_tag_ids: set[int] = set()
        for path in paths:
            tags = self._tag_service.get_tags_for_file(path)
            all_tag_ids.update(t["id"] for t in tags)

        union_tags: list[dict[str, Any]] = []
        for tag_id in sorted(all_tag_ids):
            tag_name = self._tag_service.get_tag_name(tag_id)
            if tag_name:
                # Determine source from any file that has this tag
                source = "user"
                for path in paths:
                    file_tags = self._tag_service.get_tags_for_file(path)
                    for ft in file_tags:
                        if ft["id"] == tag_id:
                            source = ft.get("source", "user")
                            break
                    else:
                        continue
                    break
                union_tags.append({"id": tag_id, "name": tag_name, "source": source})
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
    def _tag_role(tag: dict[str, Any]) -> str:
        """Determine the tag pill role from a tag dict.

        Returns ``"primary"`` for user-created tags, ``"secondary"`` for
        auto-generated tags (e.g. auto-color detection).
        """
        source = tag.get("source", "user")
        return "primary" if source == "user" else "secondary"

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
