"""Tests for GalleryController"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock

import pytest
from PIL import Image, ImageOps
from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QLineEdit

from tarragon.db.common.tag import Tag, TagSource
from tarragon.db.database import Database
from tarragon.gallery_controller import GalleryController
from tarragon.models.filter_state import FilterState
from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.renderers.cache import load_image
from tarragon.services.query_service import QueryService
from tarragon.services.tag_service import TagService
from tarragon.theme.color_buckets import ColorBucket
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


@pytest.fixture
def real_controller(qapp: Any) -> Generator[GalleryController, None, None]:
    """Provide a real GalleryController wired to a real QueryService.

    Unlike the spy fixture, ``run_filtered_query`` executes the real query
    pipeline so the color emit → handler → QueryService chain runs end to end.
    """
    db = Database(Path(":memory:"))
    db.init_schema()
    tag_service = TagService(db=db)
    gallery_tabs = GalleryTabs()
    gallery_info_bar = GalleryInfoBar()
    filter_bar = FilterBar(tag_service=tag_service, db=db)
    search_edit = QLineEdit()
    preview_panel = PreviewPanel(settings_service=MagicMock())
    ctl = GalleryController(
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

    @staticmethod
    def _save_rotated_jpeg(path: Path) -> None:
        """Write a 50x100 JPEG whose EXIF orientation tag demands a 90° rotation.

        EXIF orientation 6 means "rotate 90° clockwise to view upright", so
        an oriented load must report (100, 50) instead of the stored (50, 100).
        """
        img = Image.new("RGB", (50, 100), color="red")
        exif = img.getexif()
        exif[0x0112] = 6
        img.save(path, format="JPEG", exif=exif)

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

        loaded = controller._load_preview_image(source)

        assert loaded.image.size == (1024, 768)
        assert loaded.image.getpixel((10, 10)) == (255, 0, 0)
        assert loaded.from_cache is True
        assert loaded.original_width == 1024
        assert loaded.original_height == 768

    def test_load_preview_image_cached_exif_orientation_is_not_transposed(
        self, controller: _SpyController, tmp_path: Path
    ) -> None:
        """A cached image with an EXIF orientation tag loads byte-identical (caches are already oriented)."""
        source = tmp_path / "source.jpg"
        preview_path = tmp_path / "preview.jpg"
        self._save_rotated_jpeg(preview_path)
        controller._db.upsert_thumbnail(
            str(source),
            mtime=1,
            size=100,
            width=50,
            height=100,
            cache_uuid="u1",
            preview_cache_path=str(preview_path),
        )

        loaded = controller._load_preview_image(source)

        assert loaded.from_cache is True
        assert loaded.image.size == (50, 100)
        assert loaded.image.tobytes() == load_image(preview_path).tobytes()
        # Provenance travels in the LoadedImage wrapper, never on the PIL image.
        assert not hasattr(loaded.image, "_from_cache")

    def test_load_preview_image_original_exif_orientation_is_transposed(
        self, controller: _SpyController, tmp_path: Path
    ) -> None:
        """The original-file fallback applies exif_transpose so uncached loads are display-oriented."""
        source = tmp_path / "uncached.jpg"
        self._save_rotated_jpeg(source)

        loaded = controller._load_preview_image(source)

        assert loaded.from_cache is False
        assert loaded.image.size == (100, 50)
        with Image.open(source) as raw:
            raw.load()
            expected = ImageOps.exif_transpose(raw) or raw
            assert loaded.image.tobytes() == expected.tobytes()
        assert not hasattr(loaded.image, "_from_cache")

    def test_single_selection_uncached_image_is_displayed_rotated(
        self, controller: _SpyController, tmp_path: Path
    ) -> None:
        """Single selection of an uncached EXIF-oriented file shows the upright (transposed) image."""
        source = tmp_path / "portrait.jpg"
        img = Image.new("RGB", (50, 100), color=(0, 0, 255))
        # A 12px red band along the bottom edge; JPEG chroma subsampling smears
        # sharp boundaries, so the band must be thick enough for reliable sampling.
        img.paste(Image.new("RGB", (50, 12), color=(255, 0, 0)), (0, 88))
        exif = img.getexif()
        exif[0x0112] = 6
        img.save(source, format="JPEG", quality=95, exif=exif)

        controller.on_selection_changed([str(source)])

        pixmap = controller._preview_panel._cached_pixmap
        assert pixmap is not None
        assert pixmap.width() == 100
        assert pixmap.height() == 50
        # Orientation 6 rotates 90° CW: the original bottom band lands on the left edge.
        red = pixmap.toImage().pixelColor(0, 0)
        assert red.red() > 200 and red.green() < 80 and red.blue() < 80
        blue = pixmap.toImage().pixelColor(20, 0)
        assert blue.red() < 80 and blue.green() < 80 and blue.blue() > 200


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


class TestColorFilterChain:
    """End-to-end color filter chain: emit → handler → QueryService.

    Regression coverage for finding #17: the live color-filter path stored
    ``ColorBucket`` values verbatim into ``filter_state.color_tags``, and the
    query color branch called ``get_name()`` on them, raising AttributeError
    at runtime. The handler now converts buckets to ``AUTO_COLOR`` tags at the
    controller boundary.
    """

    def test_handler_converts_buckets_to_auto_color_tags(self, controller: _SpyController) -> None:
        """on_color_filter_changed stores Tag objects (name=bucket value, source=AUTO_COLOR)."""
        controller.on_color_filter_changed({ColorBucket.RED, ColorBucket.BLUE})

        assert controller._filter_state.color_tags == {
            Tag(id=0, name="red", source=TagSource.AUTO_COLOR),
            Tag(id=0, name="blue", source=TagSource.AUTO_COLOR),
        }

    def test_color_bucket_toggle_filters_through_real_query_service(self, real_controller: GalleryController) -> None:
        """Toggling a swatch runs the real emit→handler→QueryService chain without AttributeError."""
        db = real_controller._db
        db.upsert_thumbnail("/test/photos/red_rose.png", mtime=1, size=100, width=10, height=10, cache_uuid="c1")
        db.upsert_thumbnail("/test/photos/blue_sky.png", mtime=2, size=200, width=10, height=10, cache_uuid="c2")
        red = db.ensure_tag("red")
        red.set_source(TagSource.AUTO_COLOR)
        db.add_tag_to_files(["/test/photos/red_rose.png"], red)
        real_controller.current_folder = "/test/photos/"

        real_controller._filter_bar.filter_bar_color.toggle_color(ColorBucket.RED)

        assert real_controller._filter_state.color_tags == {Tag(id=0, name="red", source=TagSource.AUTO_COLOR)}
        assert real_controller._thumbnail_model._paths == [Path("/test/photos/red_rose.png")]
