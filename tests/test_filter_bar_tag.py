"""Tests for FilterBarTag"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest
from PySide6.QtWidgets import QLabel, QPushButton

from tarragon.db.common.tag import TagSource
from tarragon.db.database import Database
from tarragon.services.tag_service import TagService
from tarragon.widgets.filter_bar_tag import FilterBarTag


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
def bar(service: TagService) -> Generator[FilterBarTag, None, None]:
    """Create a FilterBarTag that is cleaned up after the test."""
    w = FilterBarTag(service)
    yield w
    w.close()


TEST_TAG_NAME_1 = "test tag 1"
TEST_TAG_NAME_2 = "test tag 2"
TEST_TAG_NAME_3 = "test tag 3"
TEST_TAG_NAME_4 = "test tag 4"


class TestFilterBarTagCreation:
    """FilterBarTag construction and basic structure."""

    def test_creation(self, bar: FilterBarTag) -> None:
        """FilterBarTag is created without error."""
        assert isinstance(bar, FilterBarTag)

    def test_has_chips_container(self, bar: FilterBarTag) -> None:
        """FilterBarTag has a chips container widget."""
        assert bar._chips_container is not None
        assert bar._chips_layout is not None

    def test_has_tag_menu(self, bar: FilterBarTag) -> None:
        """FilterBarTag has a QMenu for tag selection."""
        assert bar._tag_menu is not None

    def test_no_active_tags_initially(self, bar: FilterBarTag) -> None:
        """No chips are displayed on a fresh widget."""
        assert bar._chips_layout.count() == 0


class TestAutoColorFiltering:
    """Auto-color tags are excluded from available tags."""

    def test_color_tags_excluded(self, service: TagService, bar: FilterBarTag) -> None:
        """Tags with auto color source are filtered out of available tags."""
        tag_1 = service.create_tag(TEST_TAG_NAME_1, TagSource.AUTO_COLOR)
        tag_1.set_usage_count(0)
        tag_2 = service.create_tag(TEST_TAG_NAME_2, TagSource.AUTO_COLOR)
        tag_2.set_usage_count(0)
        tag_3 = service.create_tag(TEST_TAG_NAME_3, TagSource.USER)
        tag_3.set_usage_count(0)
        bar._refresh_tags()

        assert tag_1 not in bar._available_tags
        assert tag_2 not in bar._available_tags
        assert tag_3 in bar._available_tags

    def test_only_color_tags(self, service: TagService, bar: FilterBarTag) -> None:
        """When only auto-color tags exist, available_tags is empty."""
        _ = service.create_tag("green", TagSource.AUTO_COLOR)
        bar._refresh_tags()

        assert len(bar._available_tags) == 0

    def test_color_tag_not_in_menu(self, service: TagService, bar: FilterBarTag) -> None:
        """Auto-color tags do not appear in the tag menu."""
        tag_1 = service.create_tag(TEST_TAG_NAME_1, TagSource.AUTO_COLOR)
        tag_1.set_usage_count(0)
        tag_2 = service.create_tag(TEST_TAG_NAME_2, TagSource.USER)
        tag_2.set_usage_count(0)
        bar._refresh_tags()

        bar._show_menu()
        menu_actions = bar._tag_menu.actions()
        action_texts = [a.text() for a in menu_actions]

        assert TEST_TAG_NAME_2 in action_texts
        assert TEST_TAG_NAME_1 not in action_texts


class TestTagMenu:
    """Tag menu shows available tags as checkable actions."""

    def test_menu_populated_with_tags(self, service: TagService, bar: FilterBarTag) -> None:
        """Menu contains an action for each available tag."""
        service.create_tag(TEST_TAG_NAME_1)
        service.create_tag(TEST_TAG_NAME_2)
        bar._refresh_tags()

        bar._show_menu()
        actions = bar._tag_menu.actions()
        assert len(actions) == 2
        assert actions[0].text() == TEST_TAG_NAME_1
        assert actions[1].text() == TEST_TAG_NAME_2

    def test_menu_actions_are_checkable(self, service: TagService, bar: FilterBarTag) -> None:
        """Each menu action is checkable."""
        service.create_tag(TEST_TAG_NAME_1)
        bar._refresh_tags()

        bar._show_menu()
        for action in bar._tag_menu.actions():
            assert action.isCheckable()

    def test_active_tag_shows_checked_in_menu(self, service: TagService, bar: FilterBarTag) -> None:
        """An active tag appears checked in the menu."""
        tag = service.create_tag(TEST_TAG_NAME_1)
        tag.set_usage_count(0)
        bar._refresh_tags()
        bar._toggle_tag(tag)

        bar._show_menu()
        actions = bar._tag_menu.actions()
        active_action = next(a for a in actions if a.data() == tag)
        assert active_action.isChecked()

    def test_menu_sorted_alphabetically(self, service: TagService, bar: FilterBarTag) -> None:
        """Menu actions are sorted alphabetically by tag name."""
        service.create_tag(TEST_TAG_NAME_3)
        service.create_tag(TEST_TAG_NAME_1)
        service.create_tag(TEST_TAG_NAME_2)
        bar._refresh_tags()

        bar._show_menu()
        action_texts = [a.text() for a in bar._tag_menu.actions()]
        assert action_texts == [TEST_TAG_NAME_1, TEST_TAG_NAME_2, TEST_TAG_NAME_3]


class TestTagFiltering:
    """Toggling tags updates the active filter set."""

    def test_toggle_tag_removes_from_active(self, service: TagService, bar: FilterBarTag) -> None:
        """Toggling an active tag removes it from the active set."""
        tag = service.create_tag(TEST_TAG_NAME_1)
        bar._refresh_tags()

        bar._toggle_tag(tag)
        assert tag in bar.get_active_tags()

        bar._toggle_tag(tag)
        assert tag not in bar.get_active_tags()

    def test_remove_tag(self, service: TagService, bar: FilterBarTag) -> None:
        """_remove_tag discards a tag from the active set."""
        tag = service.create_tag(TEST_TAG_NAME_1)
        bar._refresh_tags()

        bar._toggle_tag(tag)
        assert tag in bar.get_active_tags()

        bar._remove_tag(tag)
        assert tag not in bar.get_active_tags()

    def test_remove_nonexistent_tag_is_safe(self, bar: FilterBarTag) -> None:
        """Removing a tag that isn't active does not raise."""
        bar._remove_tag(9999)  # Should not raise
        assert bar.get_active_tags() == set()

    def test_multiple_tags(self, service: TagService, bar: FilterBarTag) -> None:
        """Multiple tags can be active simultaneously."""
        tag_1 = service.create_tag(TEST_TAG_NAME_1)
        tag_2 = service.create_tag(TEST_TAG_NAME_2)
        bar._refresh_tags()

        bar._toggle_tag(tag_1)
        bar._toggle_tag(tag_2)
        assert bar.get_active_tags() == {tag_1, tag_2}


class TestSignalEmission:
    """tag_filter_changed signal is emitted correctly."""

    def test_signal_on_toggle_on(self, service: TagService, bar: FilterBarTag) -> None:
        """Toggling a tag on emits tag_filter_changed with the tag ID."""
        tag = service.create_tag("signal-tag")
        bar._refresh_tags()

        captured: list[set[int]] = []
        bar.filter_changed.connect(captured.append)

        bar._toggle_tag(tag)

        assert len(captured) == 1
        assert tag in captured[0]

    def test_signal_on_toggle_off(self, service: TagService, bar: FilterBarTag) -> None:
        """Toggling a tag off emits tag_filter_changed without the tag ID."""
        tag = service.create_tag(TEST_TAG_NAME_1)
        bar._refresh_tags()

        bar._toggle_tag(tag)  # Turn on first

        captured: list[set[int]] = []
        bar.filter_changed.connect(captured.append)

        bar._toggle_tag(tag)  # Turn off

        assert len(captured) == 1
        assert tag not in captured[0]

    def test_signal_on_remove(self, service: TagService, bar: FilterBarTag) -> None:
        """Removing a tag emits tag_filter_changed without the tag ID."""
        tag = service.create_tag(TEST_TAG_NAME_1)
        bar._refresh_tags()

        bar._toggle_tag(tag)  # Activate first

        captured: list[set[int]] = []
        bar.filter_changed.connect(captured.append)

        bar._remove_tag(tag)

        assert len(captured) == 1
        assert tag not in captured[0]


class TestPublicAPI:
    """Public API methods work correctly."""

    def test_get_active_tags_returns_copy(self, bar: FilterBarTag) -> None:
        """get_active_tags returns a copy, not the internal set."""
        result = bar.get_active_tags()
        result.add(999)  # Mutating the returned set
        assert bar.get_active_tags() == set()  # Internal state unchanged

    def test_has_active_filters_false_initially(self, bar: FilterBarTag) -> None:
        """has_active_filters() is False when no tags are active."""
        assert bar.has_active_filters() is False

    def test_has_active_filters_true_when_active(self, service: TagService, bar: FilterBarTag) -> None:
        """has_active_filters() is True when a tag is active."""
        tag = service.create_tag("active")
        bar._refresh_tags()
        bar._toggle_tag(tag)
        assert bar.has_active_filters() is True

    def test_clear_filters(self, service: TagService, bar: FilterBarTag) -> None:
        """clear_filters() removes all active tags and emits empty set."""
        tag_1 = service.create_tag("one")
        tag_2 = service.create_tag("two")
        bar._refresh_tags()

        bar._toggle_tag(tag_1)
        bar._toggle_tag(tag_2)
        assert bar.has_active_filters() is True

        captured: list[set[int]] = []
        bar.filter_changed.connect(captured.append)

        bar.clear_filters()

        assert bar.get_active_tags() == set()
        assert bar.has_active_filters() is False
        assert len(captured) == 1
        assert captured[0] == set()

    def test_clear_filters_on_empty(self, bar: FilterBarTag) -> None:
        """clear_filters() on an already-empty filter emits empty set."""
        captured: list[set[int]] = []
        bar.filter_changed.connect(captured.append)

        bar.clear_filters()

        assert bar.get_active_tags() == set()
        assert len(captured) == 1
        assert captured[0] == set()


class TestChipDisplay:
    """Active tags are displayed as removable chips."""

    def test_chip_appears_on_toggle(self, service: TagService, bar: FilterBarTag) -> None:
        """A chip appears when a tag is toggled on."""
        tag = service.create_tag("chip-tag")
        bar._refresh_tags()

        assert bar._chips_layout.count() == 0

        bar._toggle_tag(tag)
        assert bar._chips_layout.count() == 1

    def test_chip_disappears_on_remove(self, service: TagService, bar: FilterBarTag) -> None:
        """A chip disappears when its tag is removed."""
        tag = service.create_tag("removable-chip")
        bar._refresh_tags()

        bar._toggle_tag(tag)
        assert bar._chips_layout.count() == 1

        bar._remove_tag(tag)
        assert bar._chips_layout.count() == 0

    def test_chip_shows_tag_name(self, service: TagService, bar: FilterBarTag) -> None:
        """Each chip displays the correct tag name."""
        tag = service.create_tag("my-special-tag")
        bar._refresh_tags()
        bar._toggle_tag(tag)

        # Find the QLabel in the chip
        item = bar._chips_layout.itemAt(0)
        assert item is not None
        chip_widget = item.widget()
        assert chip_widget is not None
        labels = chip_widget.findChildren(QLabel)
        assert any(lbl.text() == "my-special-tag" for lbl in labels)

    def test_chip_has_remove_button(self, service: TagService, bar: FilterBarTag) -> None:
        """Each chip has a remove button with 'x' text."""
        tag = service.create_tag("has-btn")
        bar._refresh_tags()
        bar._toggle_tag(tag)

        item = bar._chips_layout.itemAt(0)
        assert item is not None
        chip_widget = item.widget()
        assert chip_widget is not None
        remove_buttons = [btn for btn in chip_widget.findChildren(QPushButton) if btn.text() == "\u00d7"]
        assert len(remove_buttons) == 1

    def test_multiple_chips(self, service: TagService, bar: FilterBarTag) -> None:
        """Multiple active tags produce multiple chips."""
        tag_1 = service.create_tag("first")
        tag_2 = service.create_tag("second")
        tag_3 = service.create_tag("third")
        bar._refresh_tags()

        bar._toggle_tag(tag_1)
        bar._toggle_tag(tag_2)
        bar._toggle_tag(tag_3)

        assert bar._chips_layout.count() == 3

    def test_clear_filters_removes_chips(self, service: TagService, bar: FilterBarTag) -> None:
        """clear_filters() removes all chips."""
        tag_1 = service.create_tag("a")
        tag_2 = service.create_tag("b")
        bar._refresh_tags()

        bar._toggle_tag(tag_1)
        bar._toggle_tag(tag_2)
        assert bar._chips_layout.count() == 2

        bar.clear_filters()
        assert bar._chips_layout.count() == 0


class TestRefreshPreservesSelection:
    """Tag list refresh preserves active filter selections."""

    def test_active_tags_survive_refresh(self, service: TagService, bar: FilterBarTag) -> None:
        """Active tag IDs are preserved when the tag list is refreshed."""
        tag = service.create_tag("persistent")
        tag.set_usage_count(0)
        bar._refresh_tags()

        bar._toggle_tag(tag)
        assert tag in bar.get_active_tags()

        # Refresh (e.g., due to tags_changed signal)
        bar._refresh_tags()

        # The active set should still contain the tag
        assert tag in bar.get_active_tags()
