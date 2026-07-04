"""Tests for FilterBar — combined filter row widget.

Covers:
    - Widget creation with sub-widgets (ColorFilterBar, TagFilterBar, QComboBox)
    - Signal forwarding from child widgets
    - Folder dropdown visibility toggle via set_scope()
    - Folder dropdown population from database
    - Folder filter signal emission
"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QComboBox

from tarragon.db import Database
from tarragon.services.tag_service import TagService
from tarragon.widgets.color_filter_bar import ColorFilterBar
from tarragon.widgets.filter_bar import FilterBar, _short_folder_name
from tarragon.widgets.tag_filter_bar import TagFilterBar

# =========================================================================
# Fixtures
# =========================================================================


@pytest.fixture()
def db() -> Generator[Database, None, None]:
    """Provide an in-memory Database with initialised schema."""
    database = Database(Path(":memory:"))
    database.init_schema()
    yield database
    database.close()


@pytest.fixture()
def tag_service(db: Database) -> TagService:
    """Create a TagService backed by the in-memory database."""
    return TagService(db=db)


@pytest.fixture()
def bar(tag_service: TagService, db: Database) -> Generator[FilterBar, None, None]:
    """Provide a FilterBar that is closed after the test."""
    w = FilterBar(tag_service, db)
    yield w
    w.close()


# =========================================================================
# TestFilterBarCreation
# =========================================================================


class TestFilterBarCreation:
    """FilterBar construction and basic structure."""

    def test_creation(self, bar: FilterBar) -> None:
        """FilterBar is created without error."""
        assert isinstance(bar, FilterBar)

    def test_has_color_filter_bar(self, bar: FilterBar) -> None:
        """FilterBar contains a ColorFilterBar sub-widget."""
        assert isinstance(bar.color_filter_bar, ColorFilterBar)

    def test_has_tag_filter_bar(self, bar: FilterBar) -> None:
        """FilterBar contains a TagFilterBar sub-widget."""
        assert isinstance(bar.tag_filter_bar, TagFilterBar)

    def test_has_folder_combo(self, bar: FilterBar) -> None:
        """FilterBar contains a QComboBox for folder filtering."""
        combo = bar.findChildren(QComboBox)
        assert len(combo) == 1

    def test_folder_combo_has_all_folders_item(self, bar: FilterBar) -> None:
        """The folder combo has 'All Folders' as the first item."""
        combo = bar.findChildren(QComboBox)[0]
        assert combo.count() >= 1
        assert combo.itemText(0) == "All Folders"
        assert combo.itemData(0) == ""


# =========================================================================
# TestSignalForwarding
# =========================================================================


class TestSignalForwarding:
    """Signals from child widgets are forwarded through FilterBar."""

    def test_color_filter_changed_forwarded(self, bar: FilterBar) -> None:
        """color_filter_changed signal is forwarded from ColorFilterBar."""
        captured: list[set[str]] = []
        bar.color_filter_changed.connect(captured.append)

        bar.color_filter_bar.toggle_color("red")

        assert len(captured) == 1
        assert captured[0] == {"color:red"}

    def test_tag_filter_changed_forwarded(
        self, bar: FilterBar, tag_service: TagService
    ) -> None:
        """tag_filter_changed signal is forwarded from TagFilterBar."""
        tag_id = tag_service.get_or_create_tag("test-tag")
        bar.tag_filter_bar._refresh_tags()

        captured: list[set[int]] = []
        bar.tag_filter_changed.connect(captured.append)

        bar.tag_filter_bar._toggle_tag(tag_id)

        assert len(captured) == 1
        assert tag_id in captured[0]

    def test_folder_filter_changed_on_combo_change(
        self, bar: FilterBar, db: Database
    ) -> None:
        """folder_filter_changed signal is emitted when combo selection changes."""
        # Populate DB with folders
        db.upsert_thumbnail(
            "/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1"
        )
        db.upsert_thumbnail(
            "/photos/work/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2"
        )
        bar.refresh_folders()

        captured: list[str] = []
        bar.folder_filter_changed.connect(captured.append)

        combo = bar.findChildren(QComboBox)[0]
        # Select the second folder (index 1, since 0 is "All Folders")
        combo.setCurrentIndex(1)

        assert len(captured) == 1
        assert captured[0] in ("/photos/vacation", "/photos/work")


# =========================================================================
# TestFolderDropdownVisibility
# =========================================================================


class TestFolderDropdownVisibility:
    """Folder dropdown visibility is controlled by set_scope()."""

    def test_hidden_by_default(self, bar: FilterBar) -> None:
        """Folder combo is hidden by default (local/folder mode)."""
        combo = bar.findChildren(QComboBox)[0]
        assert combo.isHidden()

    def test_visible_in_global_mode(self, bar: FilterBar) -> None:
        """Folder combo becomes visible when set_scope(True) is called."""
        bar.set_scope(True)
        combo = bar.findChildren(QComboBox)[0]
        assert not combo.isHidden()

    def test_hidden_in_local_mode(self, bar: FilterBar) -> None:
        """Folder combo is hidden when set_scope(False) is called."""
        bar.set_scope(True)  # Show it first
        bar.set_scope(False)  # Then hide it
        combo = bar.findChildren(QComboBox)[0]
        assert combo.isHidden()

    def test_toggle_scope(self, bar: FilterBar) -> None:
        """Toggling scope multiple times correctly shows/hides the combo."""
        combo = bar.findChildren(QComboBox)[0]

        bar.set_scope(True)
        assert not combo.isHidden()

        bar.set_scope(False)
        assert combo.isHidden()

        bar.set_scope(True)
        assert not combo.isHidden()


# =========================================================================
# TestFolderDropdownPopulation
# =========================================================================


class TestFolderDropdownPopulation:
    """Folder dropdown is populated from the database."""

    def test_empty_db_has_only_all_folders(self, bar: FilterBar) -> None:
        """With no thumbnails, combo has only 'All Folders'."""
        combo = bar.findChildren(QComboBox)[0]
        assert combo.count() == 1
        assert combo.itemText(0) == "All Folders"

    def test_populated_with_distinct_folders(
        self, bar: FilterBar, db: Database
    ) -> None:
        """Combo is populated with distinct folder paths from the DB."""
        db.upsert_thumbnail(
            "/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1"
        )
        db.upsert_thumbnail(
            "/photos/vacation/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2"
        )
        db.upsert_thumbnail(
            "/photos/work/c.png", mtime=3, size=300, width=10, height=10, cache_uuid="u3"
        )
        bar.refresh_folders()

        combo = bar.findChildren(QComboBox)[0]
        # "All Folders" + 2 distinct folders
        assert combo.count() == 3

    def test_refresh_clears_old_entries(
        self, bar: FilterBar, db: Database
    ) -> None:
        """Refreshing replaces old folder entries with new ones."""
        db.upsert_thumbnail(
            "/old_folder/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1"
        )
        bar.refresh_folders()

        combo = bar.findChildren(QComboBox)[0]
        assert combo.count() == 2  # "All Folders" + 1 folder

        # Now change the data and refresh
        db._execute("DELETE FROM thumbnails")
        db._commit()
        db.upsert_thumbnail(
            "/new_folder_x/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2"
        )
        db.upsert_thumbnail(
            "/new_folder_y/c.png", mtime=3, size=300, width=10, height=10, cache_uuid="u3"
        )
        bar.refresh_folders()

        assert combo.count() == 3  # "All Folders" + 2 new folders

    def test_set_scope_refreshes_folders(
        self, bar: FilterBar, db: Database
    ) -> None:
        """Calling set_scope(True) refreshes the folder list."""
        db.upsert_thumbnail(
            "/dynamic/folder/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1"
        )

        combo = bar.findChildren(QComboBox)[0]
        initial_count = combo.count()

        bar.set_scope(True)
        assert combo.count() >= initial_count


# =========================================================================
# TestFolderFilterSignal
# =========================================================================


class TestFolderFilterSignal:
    """Folder filter signal emits correct values."""

    def test_all_folders_emits_empty_string(
        self, bar: FilterBar, db: Database
    ) -> None:
        """Selecting 'All Folders' emits an empty string."""
        db.upsert_thumbnail(
            "/some/folder/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1"
        )
        bar.refresh_folders()

        combo = bar.findChildren(QComboBox)[0]

        captured: list[str] = []
        bar.folder_filter_changed.connect(captured.append)

        # First select a real folder (index 1), then go back to "All Folders" (index 0)
        combo.setCurrentIndex(1)
        combo.setCurrentIndex(0)  # "All Folders"

        assert "" in captured

    def test_selecting_folder_emits_path(
        self, bar: FilterBar, db: Database
    ) -> None:
        """Selecting a specific folder emits its path."""
        db.upsert_thumbnail(
            "/alpha/beta/img.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1"
        )
        bar.refresh_folders()

        captured: list[str] = []
        bar.folder_filter_changed.connect(captured.append)

        combo = bar.findChildren(QComboBox)[0]
        # Find the index of the folder we added
        for i in range(combo.count()):
            if combo.itemData(i) == "/alpha/beta":
                combo.setCurrentIndex(i)
                break

        assert len(captured) == 1
        assert captured[0] == "/alpha/beta"


# =========================================================================
# TestShortFolderName
# =========================================================================


class TestShortFolderName:
    """_short_folder_name utility produces readable display names."""

    def test_short_path_unchanged(self) -> None:
        """Paths with 2 or fewer components are returned as-is."""
        assert _short_folder_name("/photos") == "/photos"
        assert _short_folder_name("photos") == "photos"

    def test_long_path_shows_last_two(self) -> None:
        """Long paths show only the last two components."""
        assert _short_folder_name("/home/user/photos/vacation") == "photos/vacation"

    def test_three_components(self) -> None:
        """Three-component paths show the last two."""
        assert _short_folder_name("/a/b/c") == "b/c"


# =========================================================================
# TestSignalBlockingDuringRefresh — regression for signal flooding bug
# =========================================================================


class TestSignalBlockingDuringRefresh:
    """Signals are blocked during _refresh_folder_list to prevent flooding."""

    def test_no_spurious_signals_during_refresh(
        self, bar: FilterBar, db: Database
    ) -> None:
        """refresh_folders() does not emit folder_filter_changed during rebuild."""
        # Populate DB with folders
        db.upsert_thumbnail(
            "/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1"
        )
        db.upsert_thumbnail(
            "/photos/work/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2"
        )
        bar.refresh_folders()

        # Select a specific folder (not "All Folders")
        combo = bar.findChildren(QComboBox)[0]
        combo.setCurrentIndex(1)

        # Now connect the signal and refresh — no emissions should occur
        captured: list[str] = []
        bar.folder_filter_changed.connect(captured.append)

        bar.refresh_folders()

        assert len(captured) == 0, (
            "folder_filter_changed was emitted during refresh_folders() — "
            "signals should be blocked during rebuild"
        )

    def test_selection_preserved_during_refresh(
        self, bar: FilterBar, db: Database
    ) -> None:
        """The user's folder selection is preserved after refresh_folders()."""
        # Populate DB with folders
        db.upsert_thumbnail(
            "/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1"
        )
        db.upsert_thumbnail(
            "/photos/work/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2"
        )
        bar.refresh_folders()

        combo = bar.findChildren(QComboBox)[0]

        # Find and select /photos/work
        work_idx = combo.findData("/photos/work")
        assert work_idx >= 0, "Test setup: /photos/work should be in the combo"
        combo.setCurrentIndex(work_idx)
        assert combo.currentData() == "/photos/work"

        # Refresh — selection should be preserved
        bar.refresh_folders()

        assert combo.currentData() == "/photos/work", (
            "Folder selection was lost during refresh_folders()"
        )

    def test_selection_falls_back_when_folder_removed(
        self, bar: FilterBar, db: Database
    ) -> None:
        """If the selected folder no longer exists, combo falls back to index 0."""
        # Populate DB with a folder
        db.upsert_thumbnail(
            "/photos/temp/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1"
        )
        bar.refresh_folders()

        combo = bar.findChildren(QComboBox)[0]

        # Select the folder
        temp_idx = combo.findData("/photos/temp")
        assert temp_idx >= 0
        combo.setCurrentIndex(temp_idx)
        assert combo.currentData() == "/photos/temp"

        # Remove the folder from DB and refresh
        db._execute("DELETE FROM thumbnails")
        db._commit()
        db.upsert_thumbnail(
            "/photos/other/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2"
        )
        bar.refresh_folders()

        # Selection should fall back to "All Folders" (index 0)
        assert combo.currentIndex() == 0
        assert combo.currentData() == ""


# =========================================================================
# TestFolderComboTooltips
# =========================================================================


class TestFolderComboTooltips:
    """Folder combo items have tooltips showing the full path."""

    def test_folder_items_have_tooltip(
        self, bar: FilterBar, db: Database
    ) -> None:
        """Each folder item in the combo has a tooltip with the full path."""
        db.upsert_thumbnail(
            "/home/user/photos/vacation/a.png",
            mtime=1, size=100, width=10, height=10, cache_uuid="u1",
        )
        bar.refresh_folders()

        combo = bar.findChildren(QComboBox)[0]

        # Find the folder item (not "All Folders" at index 0)
        vacation_idx = combo.findData("/home/user/photos/vacation")
        assert vacation_idx >= 0

        tooltip = combo.itemData(vacation_idx, Qt.ItemDataRole.ToolTipRole)
        assert tooltip == "/home/user/photos/vacation", (
            "Tooltip should contain the full folder path"
        )

    def test_all_folders_has_no_tooltip(
        self, bar: FilterBar, db: Database
    ) -> None:
        """The 'All Folders' item does not have a tooltip."""
        combo = bar.findChildren(QComboBox)[0]

        tooltip = combo.itemData(0, Qt.ItemDataRole.ToolTipRole)
        # Should be None or empty — no tooltip for "All Folders"
        assert not tooltip
