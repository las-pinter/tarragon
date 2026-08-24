"""Metadata"""

import logging
from abc import abstractmethod

from PySide6.QtWidgets import (
    QGridLayout,
    QLabel,
)

from tarragon.common import ImageInfo
from tarragon.theme.constants import SPACING_S

logger = logging.getLogger(__name__)


class Metadata:
    def __init__(self, name: str) -> None:
        self._name = name
        self._name_widget = QLabel()
        self._name_widget.setObjectName("previewMetaLabel")
        self._name_widget.setText(name)
        self._label_widget = QLabel()
        self._label_widget.setObjectName("previewMetaValue")

    def get_name(self) -> str:
        return self._name

    def get_name_widget(self) -> QLabel:
        return self._name_widget

    def get_label_widget(self) -> QLabel:
        return self._label_widget

    def show_label(self, show: bool) -> None:
        if show:
            self._label_widget.show()
        else:
            self._label_widget.hide()

    def clear(self) -> None:
        self._label_widget.clear()

    @abstractmethod
    def update(self, image_infos: list[ImageInfo]) -> None:
        pass


class FilenameMeta(Metadata):
    def __init__(self) -> None:
        super().__init__("File")
        self._label_widget.setWordWrap(True)

    def update(self, image_infos: list[ImageInfo]) -> None:
        if len(image_infos) == 0:
            self._label_widget.setText("Unknown")
            return

        if len(image_infos) > 1:
            self._label_widget.setText(f"{len(image_infos)} files selected")
            return

        info = image_infos[0]
        if not info.path:
            self._label_widget.setText("Unknown")
            return

        self._label_widget.setText(info.path.name)
        return


class DimensionsMeta(Metadata):
    def __init__(self) -> None:
        super().__init__("Dimensions")

    def update(self, image_infos: list[ImageInfo]) -> None:
        if len(image_infos) == 0:
            self._label_widget.setText("Unknown")
            return

        if len(image_infos) > 1:
            self._label_widget.setText("Multiple file dimensions")
            return

        info = image_infos[0]

        if info.width is not None and info.height is not None:
            width = info.width
            height = info.height
        else:
            width, height = info.image.size
        self._label_widget.setText(f"{width} x {height}")


class SizeMeta(Metadata):
    def __init__(self) -> None:
        super().__init__("Size")

    def update(self, image_infos: list[ImageInfo]) -> None:
        if len(image_infos) == 0:
            self._label_widget.setText("Unknown")
            return

        total_size_bytes = 0

        for info in image_infos:
            try:
                if not info.path:
                    self._label_widget.setText("Unknown")
                    return
                total_size_bytes = total_size_bytes + info.path.stat().st_size
            except OSError:
                logger.warning("Could not read file size for %s", info.path, exc_info=True)
                self._label_widget.setText("Unknown")
                return

        if len(image_infos) > 1:
            self._label_widget.setText(f"Multiple selected images total size: {self.format_size(total_size_bytes)}")
            return
        self._label_widget.setText(f"Size: {self.format_size(total_size_bytes)}")

    @staticmethod
    def format_size(size_bytes: int) -> str:
        """Format file size in human-readable form."""
        value: float = size_bytes
        for unit in ("B", "KB", "MB", "GB"):
            if value < 1024:
                return f"{value:.1f} {unit}" if unit != "B" else f"{size_bytes} {unit}"
            value /= 1024
        return f"{value:.1f} TB"


class FormatMeta(Metadata):
    def __init__(self) -> None:
        super().__init__("Format")

    def update(self, image_infos: list[ImageInfo]) -> None:
        if len(image_infos) == 0:
            self._label_widget.setText("Unknown")
            return

        if len(image_infos) > 1:
            self._label_widget.setText("Multiple files selected")
            return

        info = image_infos[0]

        if info.path:
            format_name = info.path.suffix.lstrip(".").upper()
        elif info.image.format:
            format_name = info.image.format
        else:
            format_name = "Unknown"
        self._label_widget.setText(f"{format_name}")


class MetadataGrid:
    def __init__(self) -> None:
        self._metadata_dict: dict[str, Metadata] = {}
        self._rows = 0
        self._widget = QGridLayout()
        self._widget.setHorizontalSpacing(SPACING_S)
        self._widget.setVerticalSpacing(2)
        self._widget.setColumnStretch(1, 1)

    def add_metadata(self, metadata: Metadata) -> None:
        self._metadata_dict[metadata.get_name()] = metadata
        self._widget.addWidget(metadata.get_name_widget(), self._rows, 0)
        self._widget.addWidget(metadata.get_label_widget(), self._rows, 1)
        self._rows = self._rows + 1

    def get_widget(self) -> QGridLayout:
        return self._widget

    def update(self, image_infos: list[ImageInfo]) -> None:
        for metadata in self._metadata_dict.values():
            metadata.update(image_infos)

    def clear(self) -> None:
        for metadata in self._metadata_dict.values():
            metadata.clear()

    def __getattr__(self, name: str) -> Metadata:
        return self._metadata_dict[name]
