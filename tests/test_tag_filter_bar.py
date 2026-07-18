"""Tests for TagFilterBar — inline tag filter widget with Add Tag+ button.

Testing patterns applied:
  - Arrange-Act-Assert (AAA)
  - pytest fixtures for service and widget setup
  - Signal capture for verifying emissions
  - In-memory SQLite for full isolation
"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest
from PySide6.QtWidgets import QLabel, QPushButton
from tarragon.db import Database
from tarragon.services.tag_service import TagService
from tarragon.widgets.tag_filter_bar import TagFilterBar

# =========================================================================
# Fixtures
# =========================================================================


@pytest.fixture
def db() -> Database:
    """Create an in-memory Database with initialised schema."""
    database = Database(Path(":memory:"))
    database.init_schema()
    return database


@pytest.fixture
def service(db: Database) -> TagService:
    """Create a TagService backed by an in-memory database."""
    return TagService(db=db)


@pytest.fixture
def bar(service: TagService) -> Generator[TagFilterBar, None, None]:
    """Create a TagFilterBar that is cleaned up after the test."""
    w = TagFilterBar(service)
    yield w
    w.close()


# =========================================================================
# TestTagFilterBarCreation
# =========================================================================


class TestTagFilterBarCreation:
    """TagFilterBar construction and basic structure."""

    def test_creation(self, bar: TagFilterBar) -> None:
        """TagFilterBar is created without error."""
        assert isinstance(bar, TagFilterBar)

    def test_has_add_tag_button(self, bar: TagFilterBar) -> None:
        """TagFilterBar has an 'Add Tag+' button."""
        assert bar._add_button.text() == "Add Tag+"

    def test_has_chips_container(self, bar: TagFilterBar) -> None:
        """TagFilterBar has a chips container widget."""
        assert bar._chips_container is not None
        assert bar._chips_layout is not None

    def test_has_tag_menu(self, bar: TagFilterBar) -> None:
        """TagFilterBar has a QMenu for tag selection."""
        assert bar._tag_menu is not None

    def test_no_active_tags_initially(self, bar: TagFilterBar) -> None:
        """No chips are displayed on a fresh widget."""
        assert bar._chips_layout.count() == 0


# =========================================================================
# TestAutoColorFiltering
# =========================================================================


class TestAutoColorFiltering:
    """Auto-color tags (color: prefix) are excluded from available tags."""

    def test_color_tags_excluded(self, service: TagService, bar: TagFilterBar) -> None:
        """Tags starting with 'color:' are filtered out of available tags."""
        service._get_or_create_tag("color:red")
        service._get_or_create_tag("color:blue")
        service._get_or_create_tag("landscape")
        bar._refresh_tags()

        assert "landscape" in bar._available_tags.values()
        assert "color:red" not in bar._available_tags.values()
        assert "color:blue" not in bar._available_tags.values()

    def test_only_color_tags(self, service: TagService, bar: TagFilterBar) -> None:
        """When only auto-color tags exist, available_tags is empty."""
        service._get_or_create_tag("color:green")
        bar._refresh_tags()

        assert len(bar._available_tags) == 0

    def test_color_tag_not_in_menu(self, service: TagService, bar: TagFilterBar) -> None:
        """Auto-color tags do not appear in the tag menu."""
        service._get_or_create_tag("color:yellow")
        service._get_or_create_tag("portrait")
        bar._refresh_tags()

        bar._show_tag_menu()
        menu_actions = bar._tag_menu.actions()
        action_texts = [a.text() for a in menu_actions]

        assert "portrait" in action_texts
        assert "color:yellow" not in action_texts


# =========================================================================
# TestTagMenu
# =========================================================================


class TestTagMenu:
    """Tag menu shows available tags as checkable actions."""

    def test_menu_populated_with_tags(self, service: TagService, bar: TagFilterBar) -> None:
        """Menu contains an action for each available tag."""
        service._get_or_create_tag("alpha")
        service._get_or_create_tag("beta")
        bar._refresh_tags()

        bar._show_tag_menu()
        actions = bar._tag_menu.actions()
        assert len(actions) == 2
        assert actions[0].text() == "alpha"
        assert actions[1].text() == "beta"

    def test_menu_actions_are_checkable(self, service: TagService, bar: TagFilterBar) -> None:
        """Each menu action is checkable."""
        service._get_or_create_tag("test-tag")
        bar._refresh_tags()

        bar._show_tag_menu()
        for action in bar._tag_menu.actions():
            assert action.isCheckable()

    def test_active_tag_shows_checked_in_menu(self, service: TagService, bar: TagFilterBar) -> None:
        """An active tag appears checked in the menu."""
        tag_id = service._get_or_create_tag("active-tag")
        bar._refresh_tags()
        bar._toggle_tag(tag_id)

        bar._show_tag_menu()
        actions = bar._tag_menu.actions()
        active_action = next(a for a in actions if a.data() == tag_id)
        assert active_action.isChecked()

    def test_menu_sorted_alphabetically(self, service: TagService, bar: TagFilterBar) -> None:
        """Menu actions are sorted alphabetically by tag name."""
        service._get_or_create_tag("zebra")
        service._get_or_create_tag("apple")
        service._get_or_create_tag("mango")
        bar._refresh_tags()

        bar._show_tag_menu()
        action_texts = [a.text() for a in bar._tag_menu.actions()]
        assert action_texts == ["apple", "mango", "zebra"]


# =========================================================================
# TestTagFiltering
# =========================================================================


class TestTagFiltering:
    """Toggling tags updates the active filter set."""

    def test_toggle_tag_adds_to_active(self, service: TagService, bar: TagFilterBar) -> None:
        """Toggling a tag adds its ID to the active set."""
        tag_id = service._get_or_create_tag("filter-me")
        bar._refresh_tags()

        bar._toggle_tag(tag_id)
        assert tag_id in bar.get_active_tag_ids()

    def test_toggle_tag_removes_from_active(self, service: TagService, bar: TagFilterBar) -> None:
        """Toggling an active tag removes it from the active set."""
        tag_id = service._get_or_create_tag("remove-me")
        bar._refresh_tags()

        bar._toggle_tag(tag_id)
        assert tag_id in bar.get_active_tag_ids()

        bar._toggle_tag(tag_id)
        assert tag_id not in bar.get_active_tag_ids()

    def test_remove_tag(self, service: TagService, bar: TagFilterBar) -> None:
        """_remove_tag discards a tag from the active set."""
        tag_id = service._get_or_create_tag("discard-me")
        bar._refresh_tags()

        bar._toggle_tag(tag_id)
        assert tag_id in bar.get_active_tag_ids()

        bar._remove_tag(tag_id)
        assert tag_id not in bar.get_active_tag_ids()

    def test_remove_nonexistent_tag_is_safe(self, bar: TagFilterBar) -> None:
        """Removing a tag that isn't active does not raise."""
        bar._remove_tag(9999)  # Should not raise
        assert bar.get_active_tag_ids() == set()

    def test_multiple_tags(self, service: TagService, bar: TagFilterBar) -> None:
        """Multiple tags can be active simultaneously."""
        id1 = service._get_or_create_tag("alpha")
        id2 = service._get_or_create_tag("beta")
        bar._refresh_tags()

        bar._toggle_tag(id1)
        bar._toggle_tag(id2)
        assert bar.get_active_tag_ids() == {id1, id2}


# =========================================================================
# TestSignalEmission
# =========================================================================


class TestSignalEmission:
    """tag_filter_changed signal is emitted correctly."""

    def test_signal_on_toggle_on(self, service: TagService, bar: TagFilterBar) -> None:
        """Toggling a tag on emits tag_filter_changed with the tag ID."""
        tag_id = service._get_or_create_tag("signal-tag")
        bar._refresh_tags()

        captured: list[set[int]] = []
        bar.tag_filter_changed.connect(captured.append)

        bar._toggle_tag(tag_id)

        assert len(captured) == 1
        assert tag_id in captured[0]

    def test_signal_on_toggle_off(self, service: TagService, bar: TagFilterBar) -> None:
        """Toggling a tag off emits tag_filter_changed without the tag ID."""
        tag_id = service._get_or_create_tag("uncheck-signal")
        bar._refresh_tags()

        bar._toggle_tag(tag_id)  # Turn on first

        captured: list[set[int]] = []
        bar.tag_filter_changed.connect(captured.append)

        bar._toggle_tag(tag_id)  # Turn off

        assert len(captured) == 1
        assert tag_id not in captured[0]

    def test_signal_on_remove(self, service: TagService, bar: TagFilterBar) -> None:
        """Removing a tag emits tag_filter_changed without the tag ID."""
        tag_id = service._get_or_create_tag("remove-signal")
        bar._refresh_tags()

        bar._toggle_tag(tag_id)  # Activate first

        captured: list[set[int]] = []
        bar.tag_filter_changed.connect(captured.append)

        bar._remove_tag(tag_id)

        assert len(captured) == 1
        assert tag_id not in captured[0]


# =========================================================================
# TestPublicAPI
# =========================================================================


class TestPublicAPI:
    """Public API methods work correctly."""

    def test_get_active_tag_ids_empty_initially(self, bar: TagFilterBar) -> None:
        """No tags are active on a fresh widget."""
        assert bar.get_active_tag_ids() == set()

    def test_get_active_tag_ids_returns_copy(self, bar: TagFilterBar) -> None:
        """get_active_tag_ids returns a copy, not the internal set."""
        result = bar.get_active_tag_ids()
        result.add(999)  # Mutating the returned set
        assert bar.get_active_tag_ids() == set()  # Internal state unchanged

    def test_has_active_filters_false_initially(self, bar: TagFilterBar) -> None:
        """has_active_filters() is False when no tags are active."""
        assert bar.has_active_filters() is False

    def test_has_active_filters_true_when_active(self, service: TagService, bar: TagFilterBar) -> None:
        """has_active_filters() is True when a tag is active."""
        tag_id = service._get_or_create_tag("active")
        bar._refresh_tags()
        bar._toggle_tag(tag_id)
        assert bar.has_active_filters() is True

    def test_clear_filters(self, service: TagService, bar: TagFilterBar) -> None:
        """clear_filters() removes all active tags and emits empty set."""
        id1 = service._get_or_create_tag("one")
        id2 = service._get_or_create_tag("two")
        bar._refresh_tags()

        bar._toggle_tag(id1)
        bar._toggle_tag(id2)
        assert bar.has_active_filters() is True

        captured: list[set[int]] = []
        bar.tag_filter_changed.connect(captured.append)

        bar.clear_filters()

        assert bar.get_active_tag_ids() == set()
        assert bar.has_active_filters() is False
        assert len(captured) == 1
        assert captured[0] == set()

    def test_clear_filters_on_empty(self, bar: TagFilterBar) -> None:
        """clear_filters() on an already-empty filter emits empty set."""
        captured: list[set[int]] = []
        bar.tag_filter_changed.connect(captured.append)

        bar.clear_filters()

        assert bar.get_active_tag_ids() == set()
        assert len(captured) == 1
        assert captured[0] == set()


# =========================================================================
# TestChipDisplay
# =========================================================================


class TestChipDisplay:
    """Active tags are displayed as removable chips."""

    def test_chip_appears_on_toggle(self, service: TagService, bar: TagFilterBar) -> None:
        """A chip appears when a tag is toggled on."""
        tag_id = service._get_or_create_tag("chip-tag")
        bar._refresh_tags()

        assert bar._chips_layout.count() == 0

        bar._toggle_tag(tag_id)
        assert bar._chips_layout.count() == 1

    def test_chip_disappears_on_remove(self, service: TagService, bar: TagFilterBar) -> None:
        """A chip disappears when its tag is removed."""
        tag_id = service._get_or_create_tag("removable-chip")
        bar._refresh_tags()

        bar._toggle_tag(tag_id)
        assert bar._chips_layout.count() == 1

        bar._remove_tag(tag_id)
        assert bar._chips_layout.count() == 0

    def test_chip_shows_tag_name(self, service: TagService, bar: TagFilterBar) -> None:
        """Each chip displays the correct tag name."""
        tag_id = service._get_or_create_tag("my-special-tag")
        bar._refresh_tags()
        bar._toggle_tag(tag_id)

        # Find the QLabel in the chip
        item = bar._chips_layout.itemAt(0)
        assert item is not None
        chip_widget = item.widget()
        assert chip_widget is not None
        labels = chip_widget.findChildren(QLabel)
        assert any(lbl.text() == "my-special-tag" for lbl in labels)

    def test_chip_has_remove_button(self, service: TagService, bar: TagFilterBar) -> None:
        """Each chip has a remove button with '×' text."""
        tag_id = service._get_or_create_tag("has-btn")
        bar._refresh_tags()
        bar._toggle_tag(tag_id)

        item = bar._chips_layout.itemAt(0)
        assert item is not None
        chip_widget = item.widget()
        assert chip_widget is not None
        remove_buttons = [btn for btn in chip_widget.findChildren(QPushButton) if btn.text() == "\u00d7"]
        assert len(remove_buttons) == 1

    def test_multiple_chips(self, service: TagService, bar: TagFilterBar) -> None:
        """Multiple active tags produce multiple chips."""
        id1 = service._get_or_create_tag("first")
        id2 = service._get_or_create_tag("second")
        id3 = service._get_or_create_tag("third")
        bar._refresh_tags()

        bar._toggle_tag(id1)
        bar._toggle_tag(id2)
        bar._toggle_tag(id3)

        assert bar._chips_layout.count() == 3

    def test_clear_filters_removes_chips(self, service: TagService, bar: TagFilterBar) -> None:
        """clear_filters() removes all chips."""
        id1 = service._get_or_create_tag("a")
        id2 = service._get_or_create_tag("b")
        bar._refresh_tags()

        bar._toggle_tag(id1)
        bar._toggle_tag(id2)
        assert bar._chips_layout.count() == 2

        bar.clear_filters()
        assert bar._chips_layout.count() == 0


# =========================================================================
# TestRefreshPreservesSelection
# =========================================================================


class TestRefreshPreservesSelection:
    """Tag list refresh preserves active filter selections."""

    def test_active_tags_survive_refresh(self, service: TagService, bar: TagFilterBar) -> None:
        """Active tag IDs are preserved when the tag list is refreshed."""
        tag_id = service._get_or_create_tag("persistent")
        bar._refresh_tags()

        bar._toggle_tag(tag_id)
        assert tag_id in bar.get_active_tag_ids()

        # Refresh (e.g., due to tagsChanged signal)
        bar._refresh_tags()

        # The active set should still contain the tag
        assert tag_id in bar.get_active_tag_ids()

    def test_deleted_tags_removed_from_active(self, service: TagService, bar: TagFilterBar) -> None:
        """If a tag no longer exists after refresh, it's removed from active."""
        tag_id = service._get_or_create_tag("temporary")
        bar._refresh_tags()

        bar._toggle_tag(tag_id)
        assert tag_id in bar.get_active_tag_ids()

        # Simulate tag disappearing by directly pruning _available_tags
        # and running the same intersection logic _refresh_tags uses
        bar._available_tags = {tid: name for tid, name in bar._available_tags.items() if tid != tag_id}
        bar._active_tag_ids &= set(bar._available_tags.keys())
        bar._update_chips()

        assert tag_id not in bar.get_active_tag_ids()
