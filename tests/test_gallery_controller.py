"""Tests for GalleryController"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock

import pytest
from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QLineEdit

from tarragon.db.common.tag import Tag
from tarragon.db.database import Database
from tarragon.gallery_controller import GalleryController
from tarragon.models.filter_state import FilterState
from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.services.query_service import QueryService
from tarragon.services.tag_service import TagService
from tarragon.widgets.filter_bar import FilterBar
from tarragon.widgets.gallery_info_bar import GalleryInfoBar
from tarragon.widgets.gallery_tabs import GalleryTabs
from tarragon.widgets.preview_panel import PreviewPanel

_TEST_TAG = Tag(id=1, name="beach")
_DEBOUNCE_MS = 300


class _SpyController(GalleryController):
    """GalleryController subclass that counts run_filtered_query invocations."""

    def __init__(self, **kwargs: Any) -> None:
        super().__init__(**kwargs)
        self.query_calls = 0

    def run_filtered_query(self) -> None:
        """Count invocations instead of executing the real query pipeline."""
        self.query_calls += 1


def _fire_debounce(controller: _SpyController) -> None:
    """Emit the debounce timeout and consume the timer as the event loop would."""
    controller._tags_query_timer.timeout.emit()
    controller._tags_query_timer.stop()


@pytest.fixture
def controller(qapp: Any) -> Generator[_SpyController, None, None]:
    """Provide a GalleryController spy wired to real widgets and an in-memory DB."""
    db = Database(Path(":memory:"))
    db.init_schema()
    tag_service = TagService(db=db)
    gallery_tabs = GalleryTabs()
    gallery_info_bar = GalleryInfoBar()
    filter_bar = FilterBar(tag_service=tag_service, db=db)
    search_edit = QLineEdit()
    preview_panel = PreviewPanel(settings_service=MagicMock())
    ctl = _SpyController(
        query_service=QueryService(db=db),
        filter_state=FilterState(),
        thumbnail_model=ThumbnailModel(),
        gallery_tabs=gallery_tabs,
        gallery_info_bar=gallery_info_bar,
        filter_bar=filter_bar,
        search_edit=search_edit,
        search_timer=QTimer(),
        preview_panel=preview_panel,
        tag_service=tag_service,
        db=db,
    )
    yield ctl
    search_edit.close()
    preview_panel.close()
    filter_bar.close()
    gallery_info_bar.close()
    gallery_tabs.close()


class TestPreviewTagsDebounce:
    """Debounce behavior for preview tag change emissions."""

    @pytest.mark.parametrize(
        "filter_attr",
        ["tags", "color_tags"],
        ids=["tag-filter", "color-tag-filter"],
    )
    def test_preview_tags_changed_queries_once_after_debounce(
        self,
        controller: _SpyController,
        filter_attr: str,
    ) -> None:
        """Preview tag changes with an active filter arm one single-shot 300 ms timer that queries once on expiry."""
        setattr(controller._filter_state, filter_attr, {_TEST_TAG})
        controller.on_preview_tags_changed()
        assert controller.query_calls == 0
        assert controller._tags_query_timer.isActive()
        assert controller._tags_query_timer.interval() == _DEBOUNCE_MS
        assert controller._tags_query_timer.isSingleShot()
        _fire_debounce(controller)
        assert controller.query_calls == 1
        assert controller._tags_query_timer.isActive() is False

    def test_burst_emissions_coalesce_into_single_query(self, controller: _SpyController) -> None:
        """A burst of preview tag emissions arms one timer and produces exactly one query on expiry."""
        controller._filter_state.tags = {_TEST_TAG}
        controller.on_preview_tags_changed()
        controller.on_preview_tags_changed()
        controller.on_preview_tags_changed()
        assert controller.query_calls == 0
        assert controller._tags_query_timer.isActive()
        _fire_debounce(controller)
        assert controller.query_calls == 1

    def test_single_emission_with_tag_filter_queries_exactly_once(self, controller: _SpyController) -> None:
        """A single preview tag emission with a tag filter produces exactly one query on expiry."""
        controller._filter_state.tags = {_TEST_TAG}
        controller.on_preview_tags_changed()
        assert controller.query_calls == 0
        _fire_debounce(controller)
        assert controller.query_calls == 1
        assert controller._tags_query_timer.isActive() is False


class TestPreviewTagsGuard:
    """Guard that skips the debounce when no tag or color filter is active."""

    def test_no_query_when_no_tag_or_color_filter_active(self, controller: _SpyController) -> None:
        """Preview tag changes with no tag or color filter never arm the debounce timer."""
        controller.on_preview_tags_changed()
        assert controller._tags_query_timer.isActive() is False
        assert controller.query_calls == 0
