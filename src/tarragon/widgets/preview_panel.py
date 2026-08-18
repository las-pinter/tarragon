"""Preview panel widget"""

from __future__ import annotations

import logging
import math
from functools import partial

from PIL import Image, ImageOps
from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QImage, QPixmap, QResizeEvent
from PySide6.QtWidgets import (
    QGraphicsOpacityEffect,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMenu,
    QPushButton,
    QSizePolicy,
    QStackedWidget,
    QVBoxLayout,
    QWidget,
)

from tarragon.common import ImageInfo
from tarragon.db._base import normalize_path
from tarragon.db.common.tag import Tag, TagSource
from tarragon.services.settings_service import SettingsService
from tarragon.services.tag_service import TagService
from tarragon.theme.color_buckets import BUCKET_COLORS, BUCKET_HEX_COLORS, ColorBucket
from tarragon.theme.constants import SPACING_S, SPACING_XS
from tarragon.widgets.flow_layout import FlowLayout
from tarragon.widgets.metadata import DimensionsMeta, FilenameMeta, FormatMeta, MetadataGrid, SizeMeta
from tarragon.widgets.tag_pill import TagPillWidget

logger = logging.getLogger(__name__)


class _MosaicCellLabel(QLabel):
    """A grid cell that keeps its own unscaled pixmap and rescales on resize."""

    def __init__(self, pixmap: QPixmap, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._source_pixmap = pixmap
        self.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.setMinimumSize(1, 1)
        self.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Ignored)
        self._rescale()

    def resizeEvent(self, event: QResizeEvent) -> None:  # noqa: N802
        super().resizeEvent(event)
        self._rescale()

    def _rescale(self) -> None:
        if self._source_pixmap.isNull():
            return
        scaled = self._source_pixmap.scaled(
            self.size(),
            Qt.AspectRatioMode.KeepAspectRatio,
            Qt.TransformationMode.SmoothTransformation,
        )
        self.setPixmap(scaled)


class PreviewPanel(QWidget):
    """Widget that displays a single image preview with metadata and tag management.

    Shows:
    - Scaled image (maintains aspect ratio, fits panel)
    - Filename
    - Dimensions
    - File size
    - Format (JPEG, PNG, PSD, etc.)
    - Tag pills
    """

    # Emitted when tags change on the selected files (triggers gallery refresh).
    tags_changed = Signal()

    _metadata_grid: MetadataGrid
    _tag_input: QLineEdit | None = None
    _cached_pixmap: QPixmap | None = None

    _COLOR_SQUARE_SIZE = 24

    def __init__(
        self,
        settings_service: SettingsService,
        tag_service: TagService | None = None,
        parent: QWidget | None = None,
    ) -> None:
        """Initialize the preview panel.

        Args:
            settings_service: SettingsService for requesting stored
                settings.
            tag_service: TagService for tag CRUD operations. When None, tag
                management features are disabled.
            parent: Optional parent widget.
        """
        super().__init__(parent)
        self._tag_service = tag_service
        self._settings_service = settings_service

        self._current_tags: set[Tag] = set()
        self._selected_paths: list[str] = []
        self._cached_file_tags: dict[str, set[Tag]] = {}

        # Build the UI layout
        layout = QVBoxLayout(self)
        layout.setContentsMargins(SPACING_S, SPACING_S, SPACING_S, SPACING_S)
        layout.setSpacing(SPACING_S)

        # ---------------------------------------------------------------------
        # Image
        # ---------------------------------------------------------------------
        self._image_label = QLabel()
        self._image_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._image_label.setMinimumSize(200, 200)
        self._image_label.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Ignored)
        self._image_label.setText("No preview")
        self._image_label.setObjectName("previewImageLabel")

        self._mosaic_container = QWidget()
        self._mosaic_row_widgets: list[QWidget] = []
        self._mosaic_rows_layout = QVBoxLayout(self._mosaic_container)
        self._mosaic_rows_layout.setSpacing(SPACING_S)
        self._mosaic_rows_layout.setContentsMargins(SPACING_S, SPACING_S, SPACING_S, SPACING_S)

        self._preview_stack = QStackedWidget()
        self._preview_stack.addWidget(self._image_label)
        self._preview_stack.addWidget(self._mosaic_container)
        layout.addWidget(self._preview_stack, stretch=1)

        # ---------------------------------------------------------------------
        # Metadata
        # ---------------------------------------------------------------------
        self._metadata_header = QLabel("Metadata")
        self._metadata_header.setObjectName("previewSectionHeader")
        layout.addWidget(self._metadata_header)

        self._metadata_grid = self._build_metadata_grid()
        layout.addLayout(self._metadata_grid.get_widget())

        # ---------------------------------------------------------------------
        # Tags
        # ---------------------------------------------------------------------
        self._tags_header = QLabel("Tags")
        self._tags_header.setObjectName("previewSectionHeader")
        layout.addWidget(self._tags_header)

        # ---------------------------------------------------------------------
        # Color squares
        # ---------------------------------------------------------------------
        self._color_squares_container = QWidget()
        self._color_squares_layout = QHBoxLayout(self._color_squares_container)
        self._color_squares_layout.setContentsMargins(0, 0, 0, 0)
        self._color_squares_layout.setSpacing(SPACING_XS)
        self._color_square_buttons: dict[str, QPushButton] = {}
        for bucket_name in BUCKET_COLORS:
            btn = self._create_color_square_button(bucket_name)
            self._color_square_buttons[bucket_name] = btn
            self._color_squares_layout.addWidget(btn)
        self._color_squares_layout.addStretch()
        self._color_squares_container.setMinimumHeight(self._COLOR_SQUARE_SIZE)
        layout.addWidget(self._color_squares_container)

        # ---------------------------------------------------------------------
        # Tag Containers
        # ---------------------------------------------------------------------
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
        self._tag_pills: list[TagPillWidget] = []

        layout.addStretch()

        self.setObjectName("previewPanel")

        # React to external tag changes (e.g. from thumbnail auto-color)
        if self._tag_service is not None:
            self._tag_service.tags_changed.connect(self._on_external_tags_changed)

    def _build_metadata_grid(self) -> MetadataGrid:
        metadata_grid = MetadataGrid()
        metadata_grid.add_metadata(FilenameMeta())
        metadata_grid.add_metadata(DimensionsMeta())
        metadata_grid.add_metadata(SizeMeta())
        metadata_grid.add_metadata(FormatMeta())
        return metadata_grid

    def _build_mosaic_row(self, row_pixmaps: list[QPixmap], cols: int) -> QWidget:
        """Build one horizontal row of image cells, centered if not full width."""
        row_widget = QWidget()
        row_layout = QHBoxLayout(row_widget)
        row_layout.setContentsMargins(0, 0, 0, 0)
        row_layout.setSpacing(SPACING_S)

        side_stretch = cols - len(row_pixmaps)
        if side_stretch > 0:
            row_layout.addStretch(side_stretch)

        for pixmap in row_pixmaps:
            cell = _MosaicCellLabel(pixmap)
            row_layout.addWidget(cell, stretch=2)

        if side_stretch > 0:
            row_layout.addStretch(side_stretch)

        return row_widget

    def set_image(
        self,
        image_info: ImageInfo,
    ) -> None:
        """Set the image to display.

        Args:
            image_info: Info object of the image to be diisplayed
        """

        logger.debug(
            "Called - image_info: path: %s, size: %s, from_cache: %s",
            image_info.path,
            image_info.image.size,
            getattr(image_info.image, "_from_cache", False),
        )

        # Convert PIL Image to QPixmap and CACHE it (avoids re-conversion on resize)
        # qimage = self._pil_to_qimage(image_info.image)
        qimage = self._to_qimage(image_info.image)
        self._cached_pixmap = QPixmap.fromImage(qimage)
        self._preview_stack.setCurrentWidget(self._image_label)

        # Scale to fit label
        self._update_display()

        # Update metadata
        self._metadata_grid.update([image_info])

    def set_multi_preview(self, image_infos: list[ImageInfo]) -> None:
        """Render N-up mosaic when multiple files are selected.

        Args:
            images: List of images to display

        Clears single-image preview state.
        """

        logger.debug("Called")

        for image_info in image_infos:
            logger.debug(
                "image_info: path: %s, size: %s, from_cache: %s",
                image_info.path,
                image_info.image.size,
                getattr(image_info.image, "_from_cache", False),
            )

        if not image_infos:
            self.clear()
            return

        # Cap the number of images to display
        cap = self._settings_service.max_multi_preview.get()
        image_infos_capped = image_infos[:cap]
        n = len(image_infos_capped)
        cols = math.ceil(math.sqrt(n))
        self._clear_mosaic_cells()

        pixmaps: list[QPixmap] = []
        for image_info in image_infos_capped:
            img = ImageOps.exif_transpose(image_info.image) or image_info.image.copy()
            if img.mode not in ("RGB", "RGBA"):
                img = img.convert("RGB")
            pixmaps.append(QPixmap.fromImage(self._to_qimage(img)))

        for i in range(0, n, cols):
            row_widget = self._build_mosaic_row(pixmaps[i : i + cols], cols)
            self._mosaic_rows_layout.addWidget(row_widget, stretch=1)
            self._mosaic_row_widgets.append(row_widget)

        self._preview_stack.setCurrentWidget(self._mosaic_container)
        self._cached_pixmap = None

        # Update metadata
        self._metadata_grid.update(image_infos)

    def _clear_mosaic_cells(self) -> None:
        """Remove all mosaic cell widgets from the grid."""
        for row_widget in self._mosaic_row_widgets:
            self._mosaic_rows_layout.removeWidget(row_widget)
            row_widget.deleteLater()
        self._mosaic_row_widgets.clear()

    def resizeEvent(self, event: QResizeEvent) -> None:  # noqa: N802
        """Re-scale image when panel is resized"""
        super().resizeEvent(event)
        self._update_display()

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

    # -------------------------------------------------------------------------
    # Color squares
    # -------------------------------------------------------------------------

    def _create_color_square_button(self, color_bucket: ColorBucket) -> QPushButton:
        """Create a single color square button for a color bucket.

        Args:
            color_bucket: The color bucket name

        Returns:
            A QPushButton styled as a colored square.
        """
        hex_color = BUCKET_HEX_COLORS[color_bucket]
        btn = QPushButton()
        btn.setFixedSize(self._COLOR_SQUARE_SIZE, self._COLOR_SQUARE_SIZE)
        btn.setCursor(Qt.CursorShape.PointingHandCursor)
        btn.setProperty("colorSquare", True)
        btn.setToolTip(f"{color_bucket}")
        # background-color is dynamic (unique per color bucket) and cannot be
        # expressed as a QSS rule.  Border, border-radius, and hover styles are
        # handled by the QSS rule for QPushButton[colorSquare="true"].
        btn.setStyleSheet(f"QPushButton {{ background-color: {hex_color}; }}")
        btn.clicked.connect(
            lambda _checked=False, name=color_bucket: self._on_color_square_clicked(name),
        )
        # Start inactive (low opacity)
        effect = QGraphicsOpacityEffect(btn)
        effect.setOpacity(0.3)
        btn.setGraphicsEffect(effect)
        return btn

    def _on_color_square_clicked(self, color_bucket: ColorBucket) -> None:
        """Handle click on a color square

        Toggle the color tag on/off.

        If all selected files have the color tag, remove it from all.
        Otherwise, add it to all selected files.

        Args:
            color_bucket: The color bucket name
        """
        logger.debug("Called - bucket: %s, paths: %s", color_bucket, self._selected_paths)

        if not self._selected_paths or self._tag_service is None:
            return

        color_tag_name = f"{color_bucket}"

        # Find the tag ID for this color tag
        color_tag: Tag | None = None
        for tag in self._current_tags:
            if tag == color_tag_name:
                color_tag = tag
                break

        # Check if all selected files have this color tag
        all_have_tag = False
        if color_tag is not None:
            all_have_tag = all(color_tag in self._cached_file_tags.get(path, set()) for path in self._selected_paths)

        if all_have_tag and color_tag is not None:
            self._tag_service.remove_tags_from_files(
                self._selected_paths,
                {color_tag},
            )
        else:
            self._tag_service.add_tags_to_files_by_name(
                self._selected_paths,
                [color_tag_name],
                source=TagSource.AUTO_COLOR,
            )

    def _update_color_squares(self, active_color_tags: set[Tag]) -> None:
        """Update the opacity of color squares based on which colors are active.

        Uses tri-state logic for multi-selection:
        - All files have the color: opacity 1.0
        - Some files have the color: opacity 0.5
        - No files have the color: opacity 0.3

        Args:
            active_color_tags: Set of bare bucket tags which are present on
            a current file(s).
        """
        logger.debug("Called - active_color_tags: %s", active_color_tags)
        for bucket_name, btn in self._color_square_buttons.items():
            if len(self._selected_paths) > 1 and self._cached_file_tags:
                # Tri-state for multi-selection
                color_tag_name = f"{bucket_name}"
                color_tag: Tag | None = None
                for tag in self._current_tags:
                    if tag == color_tag_name:
                        color_tag = tag
                        break

                if color_tag is not None:
                    files_with_tag = sum(
                        1 for path in self._selected_paths if color_tag in self._cached_file_tags.get(path, {})
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
            elif bucket_name in [t.get_name() for t in active_color_tags]:
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

    # -------------------------------------------------------------------------
    # Tag display & management
    # -------------------------------------------------------------------------

    def set_tags(
        self,
        tags: set[Tag],
        selected_paths: list[str] | None = None,
    ) -> None:
        """Populate the tag pills area with interactive tag display.

        Args:
            tags: List of Tags
            selected_paths: Currently selected file paths.
        """
        logger.debug("Called - tags: %s, selected_paths: %s", tags, selected_paths)
        self._current_tags = tags
        if selected_paths is not None:
            self._selected_paths = [normalize_path(p) for p in selected_paths]
            if self._tag_service is not None:
                self._cached_file_tags = self._tag_service.get_tags_for_files(
                    self._selected_paths,
                )
            else:
                self._cached_file_tags = {}
        self._clear_tag_pills()

        # Split tags by source
        color_tags: set[Tag] = set()
        for tag in sorted(tags):
            source = tag.get_source()
            if source == TagSource.AUTO_COLOR:
                color_tags.add(tag)
            else:
                pill = self._create_tag_pill(tag)
                self._tag_pills.append(pill)
                self._tags_flow.addWidget(pill)

        # Update color squares (always show all 10, with appropriate opacity)
        self._update_color_squares(color_tags)

        # Force layout recalculation so the tags container resizes to fit
        self._tags_container.updateGeometry()

    def _create_tag_pill(self, tag: Tag) -> TagPillWidget:
        """Create a clickable tag pill widget with tri-state opacity support.

        Args:
            tag: Tag object

        Returns:
            A _TagPillWidget styled as a tag pill with hover-X removal.
            For multi-selection, partial tags are shown at half opacity.
        """
        pill = TagPillWidget(
            tag_name=tag.get_name(),
            on_remove=partial(self._on_tag_remove_clicked, tag),
            on_toggle=partial(self._on_tag_pill_clicked, tag),
        )

        # Tri-state opacity for multi-selection
        if len(self._selected_paths) > 1 and self._cached_file_tags:
            files_with_tag = sum(1 for path in self._selected_paths if tag in self._cached_file_tags.get(path, set()))
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
                pill.setGraphicsEffect(None)  # type: ignore[arg-type]  # Qt accepts None to clear effects at runtime

        return pill

    def _on_tag_remove_clicked(self, tag: Tag) -> None:
        """Handle click on the "x" button of a tag pill: remove tag from files.

        Args:
            tag: The tag to remove.
        """
        logger.debug("Called")
        if not self._selected_paths or self._tag_service is None:
            return

        self._tag_service.remove_tags_from_files(
            self._selected_paths,
            {tag},
        )

    def _on_tag_pill_clicked(self, tag: Tag) -> None:
        """Handle click on a tag pill: toggle tag on/off for selected files.

        If all selected files have the tag, remove it from all.
        Otherwise, add it to all selected files.

        Args:
            tag: The tag that was clicked.
        """
        logger.debug("Called")
        if not self._selected_paths or self._tag_service is None:
            return

        # Check if all selected files have this tag
        all_have_tag = all(tag in self._cached_file_tags.get(path, set()) for path in self._selected_paths)

        if all_have_tag:
            # Remove tag from all selected files
            self._tag_service.remove_tags_from_files(
                self._selected_paths,
                {tag},
            )
        else:
            # Add tag to all selected files
            self._tag_service.add_tags_to_files(
                self._selected_paths,
                {tag},
            )

    def _on_add_tag_clicked(self) -> None:
        """Show dropdown menu of existing custom tags + 'Create new...' option."""
        logger.debug("Called")

        if self._tag_service is None or not self._selected_paths:
            return

        tags = self._tag_service.get_all_tags()

        menu = QMenu(self)

        has_addable = False
        for tag in [t for t in tags if t.get_source() != TagSource.AUTO_COLOR]:
            if tag not in self._current_tags:
                action = menu.addAction(tag.get_name())
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

        if action.data() == "create_new":
            self._show_inline_tag_input()
        elif action.data() is not None:
            # Add selected tag to files
            tag = action.data()
            self._tag_service.add_tags_to_files(
                self._selected_paths,
                {tag},
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
        """Handle Enter in the inline tag input, create tag and add to files."""
        logger.debug("Called")
        if self._tag_input is None:
            return
        tag_name = self._tag_input.text().strip()
        if tag_name and self._selected_paths and self._tag_service is not None:
            self._tag_service.add_tags_to_files_by_name(self._selected_paths, [tag_name])
        self._on_tag_input_finished()

    def _on_tag_input_finished(self) -> None:
        """Remove the inline input and restore the add button."""
        logger.debug("Called")
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
        logger.debug("Called")
        if self._selected_paths and self._tag_service is not None:
            # Re-fetch tags for the current selection
            if len(self._selected_paths) == 1:
                tags = self._tag_service.get_tags_for_file(self._selected_paths[0])
            else:
                tags = self.get_union_tags(self._selected_paths)
            self.set_tags(tags, selected_paths=self._selected_paths)
        elif not self._selected_paths:
            self._clear_tag_pills()
            self._current_tags = set()
        # Emit signal so main window can refresh gallery if needed
        self.tags_changed.emit()

    def get_union_tags(self, paths: list[str]) -> set[Tag]:
        """Get the union of tags across multiple file paths.

        Args:
            paths: List of file paths to get tags for.

        Returns:
            Set of unique tags dicts from all paths.
        """
        logger.debug("Called - paths: %s", paths)
        if self._tag_service is None:
            logger.debug("No tag service, returning empty")
            return set()

        union_tags: set[Tag] = set()
        tags_dict = self._tag_service.get_tags_for_files(paths)
        for tags in tags_dict.values():
            for tag in tags:
                union_tags.add(tag)

        logger.debug("union_tags: %s", union_tags)
        return union_tags

    def _clear_tag_pills(self) -> None:
        """Remove all tag pill widgets from the flow layout."""
        for pill in self._tag_pills:
            self._tags_flow.removeWidget(pill)
            pill.deleteLater()
        self._tag_pills.clear()
        # Force layout recalculation so the tags container shrinks back
        self._tags_container.updateGeometry()

    def clear(self) -> None:
        """Clear the preview and metadata."""
        self._cached_pixmap = None
        self._current_tags = set()
        self._selected_paths = []
        self._cached_file_tags = {}
        self._image_label.clear()
        self._image_label.setText("No preview")
        self._metadata_grid.clear()
        self._clear_tag_pills()
        self._update_color_squares(set())
        self._preview_stack.setCurrentWidget(self._image_label)
        self._clear_mosaic_cells()

    @staticmethod
    def _to_qimage(pil_image: Image.Image) -> QImage:
        """Convert PIL Image to RGB QImage to ensure compatibility."""
        if pil_image.mode in ("RGBA", "LA", "PA"):
            img = pil_image.convert("RGBA")
            fmt, bpp = QImage.Format.Format_RGBA8888, 4
        else:
            img = pil_image.convert("RGB")
            fmt, bpp = QImage.Format.Format_RGB888, 3

        data = img.tobytes()
        qimage = QImage(data, pil_image.width, pil_image.height, img.width * bpp, fmt)
        return qimage.copy()
