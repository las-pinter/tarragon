"""Combined filter row for the gallery view."""

from __future__ import annotations

import logging

from PySide6.QtCore import Signal
from PySide6.QtWidgets import (
    QWidget,
)

from tarragon.db.database import Database
from tarragon.services.tag_service import TagService
from tarragon.widgets.filter_bar_color import FilterBarColor
from tarragon.widgets.filter_bar_folder import FilterBarFolder
from tarragon.widgets.filter_bar_tag import FilterBarTag
from tarragon.widgets.flow_layout import FlowLayout

logger = logging.getLogger(__name__)


class FilterBar(QWidget):
    """Combined filter bar containing colour, tag, and folder filters.

    Signals:
        color_filter_changed: Forwarded from ``FilterBarColor``.
        tag_filter_changed: Forwarded from ``FilterBarTag``.
        folder_filter_changed: Forwarded from `FilterBarFolder`
    """

    color_filter_changed = Signal(set)  # "color:<bucket>" strings
    tag_filter_changed = Signal(set)  # active filter tag IDs
    folder_filter_changed = Signal(set)  # selected folder paths

    def __init__(
        self,
        tag_service: TagService,
        db: Database,
        parent: QWidget | None = None,
    ) -> None:
        """Build the combined filter bar.

        Args:
            tag_service: TagService instance for the tag filter bar.
            db: Database instance for populating the folder menu.
            parent: Optional parent widget.
        """
        super().__init__(parent)

        # Sub widgets
        self._filter_bar_color = FilterBarColor()
        self._filter_bar_tag = FilterBarTag(tag_service)
        self._filter_bar_folder = FilterBarFolder(db)

        layout = FlowLayout(self, margin=4, spacing=6)
        layout.addWidget(self._filter_bar_color)
        layout.addWidget(self._filter_bar_tag)
        layout.addWidget(self._filter_bar_folder)

        self._filter_bar_color.filter_changed.connect(lambda colors: self.color_filter_changed.emit(colors))  # pyright: ignore[reportUnknownLambdaType, reportUnknownArgumentType]
        self._filter_bar_tag.filter_changed.connect(lambda tag_ids: self.tag_filter_changed.emit(tag_ids))  # pyright: ignore[reportUnknownLambdaType, reportUnknownArgumentType]
        self._filter_bar_folder.filter_changed.connect(lambda folders: self.folder_filter_changed.emit(folders))  # pyright: ignore[reportUnknownLambdaType, reportUnknownArgumentType]

    @property
    def filter_bar_color(self) -> FilterBarColor:
        """Access the embedded ``FilterBarColor`` widget."""
        return self._filter_bar_color

    @property
    def filter_bar_tag(self) -> FilterBarTag:
        """Access the embedded ``FilterBarTag`` widget."""
        return self._filter_bar_tag

    @property
    def filter_bar_folder(self) -> FilterBarFolder:
        """Access the embedded ``FilterBarFolder`` widget."""
        return self._filter_bar_folder

    def set_scope(self, is_global: bool) -> None:
        """Show or hide widgets based on gallery scope.

        Args:
            is_global: ``True`` when "All Images" tab is active.
        """
        self._filter_bar_folder.set_scope(is_global=is_global)
