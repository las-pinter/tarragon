"""Tests for the unified chip-filter base behaviour (audit #18).

Covers the API parity between ``FilterBarTag`` and ``FilterBarFolder``,
the incremental chip-sync fix (both bars keep the chips layout matched to
the active set instead of tag's old clear-and-rebuild), and the container
visibility contract shared by both bars.
"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest

from tarragon.db.database import Database
from tarragon.services.tag_service import TagService
from tarragon.widgets.filter_bar_folder import FilterBarFolder
from tarragon.widgets.filter_bar_tag import FilterBarTag


@pytest.fixture
def db() -> Generator[Database, None, None]:
    """Create an in-memory Database with initialised schema."""
    database = Database(Path(":memory:"))
    database.init_schema()
    yield database
    database.close()


@pytest.fixture
def tag_service(db: Database) -> TagService:
    """Create a TagService backed by an in-memory database."""
    return TagService(db=db)


@pytest.fixture
def tag_bar(tag_service: TagService) -> Generator[FilterBarTag, None, None]:
    """Provide a FilterBarTag that is closed after the test."""
    bar = FilterBarTag(tag_service)
    yield bar
    bar.close()


@pytest.fixture
def folder_bar(db: Database) -> Generator[FilterBarFolder, None, None]:
    """Provide a FilterBarFolder that is closed after the test."""
    bar = FilterBarFolder(db)
    yield bar
    bar.close()


class TestFolderStandardApi:
    """FilterBarFolder exposes the standard filter API (audit #18 parity)."""

    def test_has_active_filters_false_initially(self, folder_bar: FilterBarFolder) -> None:
        """has_active_filters() is False when no folders are selected."""
        assert folder_bar.has_active_filters() is False

    def test_has_active_filters_true_when_folder_active(self, folder_bar: FilterBarFolder, db: Database) -> None:
        """has_active_filters() is True after a folder is selected."""
        db.upsert_thumbnail("/photos/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        folder_bar._toggle_folder("/photos")
        assert folder_bar.has_active_filters() is True

    def test_active_items_returns_copy(self, folder_bar: FilterBarFolder, db: Database) -> None:
        """active_items() returns a copy, not the internal set."""
        db.upsert_thumbnail("/a/x.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/b/y.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")
        folder_bar._toggle_folder("/a")
        folder_bar._toggle_folder("/b")

        result = folder_bar.active_items()
        result.add("/c")  # Mutating the returned set
        assert folder_bar.active_items() == {"/a", "/b"}  # Internal state unchanged

    def test_clear_filters_removes_all_and_emits_once(self, folder_bar: FilterBarFolder, db: Database) -> None:
        """clear_filters() empties the selection and emits a single empty set."""
        db.upsert_thumbnail("/a/x.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/b/y.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")
        folder_bar._toggle_folder("/a")
        folder_bar._toggle_folder("/b")

        captured: list[set[str]] = []
        folder_bar.filter_changed.connect(captured.append)

        folder_bar.clear_filters()

        assert folder_bar._selected_folders == set()
        assert folder_bar._folder_chips == {}
        assert folder_bar.has_active_filters() is False
        assert len(captured) == 1
        assert captured[0] == set()

    def test_clear_filters_hides_container(self, folder_bar: FilterBarFolder, db: Database) -> None:
        """clear_filters() hides the chips container again."""
        db.upsert_thumbnail("/photos/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        folder_bar._toggle_folder("/photos")
        assert not folder_bar._chips_container.isHidden()

        folder_bar.clear_filters()
        assert folder_bar._chips_container.isHidden()


class TestChipCountParity:
    """Both bars keep the chips layout in sync with the active set (spacing-leak fix)."""

    def test_tag_chips_match_active_set_after_toggle_and_remove(
        self, tag_bar: FilterBarTag, tag_service: TagService
    ) -> None:
        """Tag chips are added/removed incrementally, matching the active set."""
        tags = [tag_service.create_tag(f"parity-{i}") for i in range(3)]
        tag_bar._refresh_tags()

        for tag in tags:
            tag_bar._toggle_tag(tag)
        assert tag_bar._chips_layout.count() == 3
        assert tag_bar._chips_layout.count() == len(tag_bar.get_active_tags())
        assert tag_bar._chips_layout.count() == len(tag_bar._chips)

        tag_bar._toggle_tag(tags[1])  # Off
        assert tag_bar._chips_layout.count() == 2
        assert tag_bar._chips_layout.count() == len(tag_bar.get_active_tags())

        tag_bar._toggle_tag(tags[1])  # Back on
        tag_bar._remove_tag(tags[0])
        assert tag_bar._chips_layout.count() == 2
        assert tag_bar._chips_layout.count() == len(tag_bar.get_active_tags())
        assert tag_bar._chips_layout.count() == len(tag_bar._chips)

    def test_folder_chips_match_active_set_after_toggle_and_remove(
        self, folder_bar: FilterBarFolder, db: Database
    ) -> None:
        """Folder chips stay in sync with the selected set."""
        for i in range(3):
            db.upsert_thumbnail(f"/f{i}/x.png", mtime=i, size=100, width=10, height=10, cache_uuid=f"u{i}")

        for i in range(3):
            folder_bar._toggle_folder(f"/f{i}")
        assert folder_bar._chips_layout.count() == 3
        assert folder_bar._chips_layout.count() == len(folder_bar._selected_folders)
        assert folder_bar._chips_layout.count() == len(folder_bar._folder_chips)

        folder_bar._toggle_folder("/f1")  # Off
        assert folder_bar._chips_layout.count() == 2
        assert folder_bar._chips_layout.count() == len(folder_bar._selected_folders)

        folder_bar._toggle_folder("/f1")  # Back on
        folder_bar._remove_folder("/f0")
        assert folder_bar._chips_layout.count() == 2
        assert folder_bar._chips_layout.count() == len(folder_bar._selected_folders)
        assert folder_bar._chips_layout.count() == len(folder_bar._folder_chips)

    def test_clear_filters_empties_tag_chips_layout(self, tag_bar: FilterBarTag, tag_service: TagService) -> None:
        """clear_filters() removes every chip widget from the layout."""
        tags = [tag_service.create_tag(f"clear-{i}") for i in range(3)]
        tag_bar._refresh_tags()
        for tag in tags:
            tag_bar._toggle_tag(tag)
        assert tag_bar._chips_layout.count() == 3

        tag_bar.clear_filters()
        assert tag_bar._chips_layout.count() == 0
        assert len(tag_bar._chips) == 0

    def test_tag_chips_survive_refresh_without_orphan_widgets(
        self, tag_bar: FilterBarTag, tag_service: TagService, db: Database
    ) -> None:
        """Refreshing the tag list silently drops chips for pruned tags."""
        tag_keep = tag_service.create_tag("keep")
        tag_drop = tag_service.create_tag("drop")
        tag_keep.set_usage_count(0)
        tag_drop.set_usage_count(0)
        tag_bar._refresh_tags()
        tag_bar._toggle_tag(tag_keep)
        tag_bar._toggle_tag(tag_drop)
        assert tag_bar._chips_layout.count() == 2

        db.delete_tag(tag_drop)
        tag_bar._refresh_tags()

        assert tag_bar._chips_layout.count() == 1
        assert tag_bar._chips_layout.count() == len(tag_bar.get_active_tags())
        assert tag_bar._chips_layout.count() == len(tag_bar._chips)


class TestContainerVisibilityParity:
    """Both bars hide the chips container when empty and show it when active."""

    def test_tag_container_hidden_initially(self, tag_bar: FilterBarTag) -> None:
        """The tag chips container is hidden with no active tags."""
        assert tag_bar._chips_container.isHidden()

    def test_tag_container_tracks_active_set(self, tag_bar: FilterBarTag, tag_service: TagService) -> None:
        """The tag chips container shows and hides with the active set."""
        tag = tag_service.create_tag("visible")
        tag_bar._refresh_tags()

        tag_bar._toggle_tag(tag)
        assert not tag_bar._chips_container.isHidden()

        tag_bar.clear_filters()
        assert tag_bar._chips_container.isHidden()
