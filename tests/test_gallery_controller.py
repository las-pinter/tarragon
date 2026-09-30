"""Tests for GalleryController"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock

import pytest
from PIL import Image
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


class TestLoadPreviewImage:
    """Loading preview images through the cached-preview fast path."""

    def test_load_preview_image_returns_cached_preview(self, controller: _SpyController, tmp_path: Path) -> None:
        """_load_preview_image loads the cached 1024px preview with original dimensions from the DB record."""
        source = tmp_path / "source.png"
        preview_path = tmp_path / "preview.png"
        Image.new("RGB", (1024, 768), color="red").save(preview_path)
        controller._db.upsert_thumbnail(
            str(source),
            mtime=1,
            size=100,
            width=1024,
            height=768,
            cache_uuid="u1",
            preview_cache_path=str(preview_path),
        )

        img, orig_w, orig_h = controller._load_preview_image(source)

        assert img.size == (1024, 768)
        assert img.getpixel((10, 10)) == (255, 0, 0)
        assert getattr(img, "_from_cache", False) is True
        assert orig_w == 1024
        assert orig_h == 768


class TestActiveFilterCount:
    """Scope-aware active filter counting for the info bar pill."""

    @staticmethod
    def _populate_all_dimensions(controller: _SpyController) -> None:
        """Set every FilterState dimension to a non-empty value."""
        controller._filter_state.filename_filter = "photo"
        controller._filter_state.tags = {_TEST_TAG}
        controller._filter_state.color_tags = {_TEST_TAG}
        controller._filter_state.folder_filters = {"/a", "/b"}

    def test_global_scope_includes_folder_filters(self, controller: _SpyController) -> None:
        """In global scope, active_filter_count() counts folder filters."""
        self._populate_all_dimensions(controller)
        assert controller.active_filter_count(is_global=True) == 5

    def test_local_scope_excludes_folder_filters(self, controller: _SpyController) -> None:
        """In local scope, active_filter_count() excludes folder filters."""
        self._populate_all_dimensions(controller)
        assert controller.active_filter_count(is_global=False) == 3

    def test_local_scope_zero_when_only_folder_filters_active(self, controller: _SpyController) -> None:
        """Local scope counts zero filters when only folder filters are set."""
        controller._filter_state.folder_filters = {"/a"}
        assert controller.active_filter_count(is_global=False) == 0

    def test_info_bar_pill_matches_local_scope_count(self, controller: _SpyController) -> None:
        """update_gallery_info_bar feeds the scope-aware count to set_active_filter_count."""
        self._populate_all_dimensions(controller)
        controller._gallery_tabs.setCurrentIndex(0)  # local scope
        controller.update_gallery_info_bar()
        assert controller._gallery_info_bar._filter_pill.isHidden() is False
        assert controller._gallery_info_bar._filter_pill.text() == "3 filters active"

    def test_info_bar_pill_matches_global_scope_count(self, controller: _SpyController) -> None:
        """update_gallery_info_bar feeds the scope-aware count to set_active_filter_count."""
        self._populate_all_dimensions(controller)
        controller._gallery_tabs.setCurrentIndex(1)  # global scope
        controller.update_gallery_info_bar()
        assert controller._gallery_info_bar._filter_pill.isHidden() is False
        assert controller._gallery_info_bar._filter_pill.text() == "5 filters active"

    def test_info_bar_pill_hidden_when_no_filters(self, controller: _SpyController) -> None:
        """update_gallery_info_bar hides the pill when the scope-aware count is zero."""
        controller.update_gallery_info_bar()
        assert controller._gallery_info_bar._filter_pill.isHidden() is True
