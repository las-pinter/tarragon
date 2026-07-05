"""Gallery info bar — shows folder name, file count, and active filter pill."""

from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QLabel, QWidget


class GalleryInfoBar(QWidget):
    """Horizontal bar above the thumbnail grid showing folder info and active filter count.

    Left side: ``"{folder_name} · {file_count} files"`` label (muted text).
    Right side: ``"{count} tags active"`` pill (amber on dark, hidden when count is 0).
    """

    def __init__(self, parent: QWidget | None = None) -> None:
        """Initialize the info bar with folder label and filter pill.

        Args:
            parent: Optional parent widget.
        """
        super().__init__(parent)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(8, 4, 8, 4)
        layout.setSpacing(0)

        # Left: folder info label
        self._folder_label = QLabel("")
        self._folder_label.setObjectName("galleryInfoLabel")
        self._folder_label.setAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter)
        layout.addWidget(self._folder_label)

        # Spacer pushes the pill to the right
        layout.addStretch(1)

        # Right: active filters pill
        self._filter_pill = QLabel("")
        self._filter_pill.setObjectName("galleryActiveFiltersPill")
        self._filter_pill.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        self._filter_pill.hide()  # Hidden when no filters active
        layout.addWidget(self._filter_pill)

    def set_folder_info(self, folder_name: str, file_count: int) -> None:
        """Update the folder info label.

        Args:
            folder_name: Display name for the folder (e.g. "Photos" or "All Images").
            file_count: Number of files currently shown in the grid.
        """
        self._folder_label.setText(f"{folder_name} \u00b7 {file_count} files")

    def set_active_filter_count(self, count: int) -> None:
        """Update the active filter pill visibility and text.

        Args:
            count: Number of active tag filters. Pill is hidden when 0.
        """
        if count > 0:
            self._filter_pill.setText(f"{count} tags active")
            self._filter_pill.show()
        else:
            self._filter_pill.hide()
