"""Tests for FilterBarFolder"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest

from tarragon.db.database import Database
from tarragon.widgets.filter_bar_folder import FilterBarFolder


@pytest.fixture
def db() -> Generator[Database, None, None]:
    """Create an in-memory Database with initialised schema."""
    database = Database(Path(":memory:"))
    database.init_schema()
    yield database
    database.close()


@pytest.fixture
def bar(db: Database) -> Generator[FilterBarFolder, None, None]:
    """Provide a FilterBarFolder that is closed after the test."""
    w = FilterBarFolder(db)
    yield w
    w.close()


class TestFolderChipVisibility:
    """Folder widgets visibility is controlled by set_scope() and selection."""

    def test_hidden_by_default(self, bar: FilterBarFolder) -> None:
        """Add Folder button is hidden by default (local/folder mode)."""
        assert bar._add_folder_btn.isHidden()

    def test_visible_in_global_mode(self, bar: FilterBarFolder) -> None:
        """Add Folder button becomes visible when set_scope(True) is called."""
        bar.set_scope(True)
        assert not bar._add_folder_btn.isHidden()

    def test_hidden_in_local_mode(self, bar: FilterBarFolder) -> None:
        """Add Folder button is hidden when set_scope(False) is called."""
        bar.set_scope(True)  # Show it first
        bar.set_scope(False)  # Then hide it
        assert bar._add_folder_btn.isHidden()

    def test_toggle_scope(self, bar: FilterBarFolder) -> None:
        """Toggling scope multiple times correctly shows/hides the button."""
        bar.set_scope(True)
        assert not bar._add_folder_btn.isHidden()

        bar.set_scope(False)
        assert bar._add_folder_btn.isHidden()

        bar.set_scope(True)
        assert not bar._add_folder_btn.isHidden()

    def test_chips_container_hidden_when_no_selection(self, bar: FilterBarFolder) -> None:
        """Folder chips container is hidden when no folders are selected."""
        assert bar._chips_container.isHidden()

    def test_chips_container_visible_when_folder_selected(self, bar: FilterBarFolder, db: Database) -> None:
        """Folder chips container becomes visible when a folder is added."""
        db.upsert_thumbnail("/photos/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        bar._toggle_folder("/photos")
        assert not bar._chips_container.isHidden()

    def test_chips_container_hidden_after_all_removed(self, bar: FilterBarFolder, db: Database) -> None:
        """Folder chips container hides again when all chips are removed."""
        db.upsert_thumbnail("/photos/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        bar._toggle_folder("/photos")
        assert not bar._chips_container.isHidden()

        bar._remove_folder("/photos")
        assert bar._chips_container.isHidden()


class TestFolderChipCreation:
    """Folder chips are created and tracked correctly."""

    def test_add_folder_chip_creates_widget(self, bar: FilterBarFolder, db: Database) -> None:
        """Adding a folder creates a chip widget."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        bar._toggle_folder("/photos/vacation")

        assert "/photos/vacation" in bar._folder_chips
        assert bar._folder_chips["/photos/vacation"] is not None

    def test_remove_folder_chip(self, bar: FilterBarFolder, db: Database) -> None:
        """Removing a folder chip removes it from tracking."""
        db.upsert_thumbnail("/photos/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        bar._toggle_folder("/photos")
        assert "/photos" in bar._folder_chips

        bar._remove_folder("/photos")
        assert "/photos" not in bar._folder_chips
        assert "/photos" not in bar._selected_folders

    def test_remove_nonexistent_folder_is_noop(self, bar: FilterBarFolder) -> None:
        """Removing a folder that isn't selected is a no-op."""
        bar._remove_folder("/nonexistent")
        assert len(bar._selected_folders) == 0

    def test_multiple_folders(self, bar: FilterBarFolder, db: Database) -> None:
        """Multiple folders can be selected simultaneously."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/photos/work/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")
        db.upsert_thumbnail("/photos/home/c.png", mtime=3, size=300, width=10, height=10, cache_uuid="u3")

        bar._toggle_folder("/photos/vacation")
        bar._toggle_folder("/photos/work")
        bar._toggle_folder("/photos/home")

        assert bar._selected_folders == {"/photos/vacation", "/photos/work", "/photos/home"}
        assert len(bar._folder_chips) == 3


class TestFolderFilterSignal:
    """Folder filter signal emits correct set values."""

    def test_adding_folder_emits_updated_set(self, bar: FilterBarFolder, db: Database) -> None:
        """Adding a folder emits the updated set including the new folder."""
        db.upsert_thumbnail("/alpha/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/beta/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")

        captured: list[set[str]] = []
        bar.filter_changed.connect(captured.append)

        bar._toggle_folder("/alpha")
        assert captured[-1] == {"/alpha"}

        bar._toggle_folder("/beta")
        assert captured[-1] == {"/alpha", "/beta"}

    def test_removing_folder_emits_updated_set(self, bar: FilterBarFolder, db: Database) -> None:
        """Removing a folder emits the updated set without the removed folder."""
        db.upsert_thumbnail("/alpha/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/beta/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")

        bar._toggle_folder("/alpha")
        bar._toggle_folder("/beta")

        captured: list[set[str]] = []
        bar.filter_changed.connect(captured.append)

        bar._remove_folder("/alpha")
        assert len(captured) == 1
        assert captured[0] == {"/beta"}


class TestShortFolderName:
    """_short_folder_name utility produces readable display names."""

    def test_short_path_unchanged(self, bar: FilterBarFolder) -> None:
        """Paths with 2 or fewer components are returned as-is."""
        assert bar._short_folder_name("/photos") == "/photos"
        assert bar._short_folder_name("photos") == "photos"

    def test_long_path_shows_last_two(self, bar: FilterBarFolder) -> None:
        """Long paths show only the last two components."""
        assert bar._short_folder_name("/home/user/photos/vacation") == str(Path("photos/vacation"))

    def test_three_components(self, bar: FilterBarFolder) -> None:
        """Three-component paths show the last two."""
        assert bar._short_folder_name("/a/b/c") == str(Path("b/c"))


class TestRefreshFolders:
    """refresh_folders() prunes stale chips."""

    def test_prune_stale_chips(self, bar: FilterBarFolder, db: Database) -> None:
        """refresh_folders() removes chips for folders no longer in DB."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/photos/work/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")

        bar._toggle_folder("/photos/vacation")
        bar._toggle_folder("/photos/work")
        assert len(bar._selected_folders) == 2

        # Remove vacation from DB
        db.delete_thumbnails_by_folder("/photos/vacation")

        bar.refresh_folders()

        assert "/photos/vacation" not in bar._selected_folders
        assert "/photos/work" in bar._selected_folders

    def test_no_spurious_signals_during_refresh(self, bar: FilterBarFolder, db: Database) -> None:
        """refresh_folders() only emits for actually removed chips."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        bar._toggle_folder("/photos/vacation")

        # Connect signal after adding - should not fire during refresh
        # since the folder still exists in DB
        captured: list[set[str]] = []
        bar.filter_changed.connect(captured.append)

        bar.refresh_folders()

        assert len(captured) == 0
