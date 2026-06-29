"""Tests for TagFilterBar — tag filter dropdown widget.

Testing patterns applied:
  - Arrange-Act-Assert (AAA)
  - pytest fixtures for service and widget setup
  - Signal capture for verifying emissions
  - In-memory SQLite for full isolation
"""

from __future__ import annotations

from pathlib import Path

import pytest
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication, QCheckBox

from tarragon.db import Database
from tarragon.services.tag_service import TagService
from tarragon.widgets.tag_filter_bar import TagFilterBar

# =========================================================================
# Fixtures
# =========================================================================


@pytest.fixture(autouse=True)
def qapp():
    """Provide a shared QApplication instance for all Qt tests."""
    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


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
def bar(service: TagService) -> TagFilterBar:
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

    def test_has_toggle_button(self, bar: TagFilterBar) -> None:
        """TagFilterBar has a toggle button labeled 'Tags'."""
        assert bar._toggle_btn.text() == "Tags"

    def test_popup_hidden_by_default(self, bar: TagFilterBar) -> None:
        """The popup panel is hidden on creation."""
        assert bar._popup.isVisible() is False

    def test_shows_tags_from_service(self, service: TagService, bar: TagFilterBar) -> None:
        """Tags from the service appear as checkboxes in the popup."""
        service.get_or_create_tag("landscape")
        service.get_or_create_tag("portrait")
        bar._refresh_tags()

        checkboxes = [cb for cb in bar.findChildren(QCheckBox) if cb.property("tag_id") is not None]
        assert len(checkboxes) == 2


# =========================================================================
# TestTogglePopup
# =========================================================================


class TestTogglePopup:
    """Toggle button shows/hides the popup."""

    def test_toggle_shows_popup(self, bar: TagFilterBar) -> None:
        """Clicking the toggle button sets the popup to visible."""
        bar._toggle_btn.setChecked(True)
        # In offscreen mode, isVisible() requires parent to be visible too,
        # so check the internal flag instead
        assert bar._popup_visible is True

    def test_toggle_hides_popup(self, bar: TagFilterBar) -> None:
        """Clicking the toggle button again hides the popup."""
        bar._toggle_btn.setChecked(True)
        bar._toggle_btn.setChecked(False)
        assert bar._popup_visible is False


# =========================================================================
# TestTagFiltering
# =========================================================================


class TestTagFiltering:
    """Checking/unchecking tags in the popup updates the active set."""

    def test_checking_tag_adds_to_active(
        self, service: TagService, bar: TagFilterBar
    ) -> None:
        """Checking a tag checkbox adds its ID to the active set."""
        tag_id = service.get_or_create_tag("filter-me")
        bar._refresh_tags()

        cb = bar._tag_checkboxes[tag_id]
        cb.setCheckState(Qt.CheckState.Checked)

        assert tag_id in bar.get_active_tag_ids()

    def test_unchecking_tag_removes_from_active(
        self, service: TagService, bar: TagFilterBar
    ) -> None:
        """Unchecking a tag checkbox removes its ID from the active set."""
        tag_id = service.get_or_create_tag("remove-me")
        bar._refresh_tags()

        cb = bar._tag_checkboxes[tag_id]
        cb.setCheckState(Qt.CheckState.Checked)
        assert tag_id in bar.get_active_tag_ids()

        cb.setCheckState(Qt.CheckState.Unchecked)
        assert tag_id not in bar.get_active_tag_ids()

    def test_multiple_tags(self, service: TagService, bar: TagFilterBar) -> None:
        """Multiple tags can be active simultaneously."""
        id1 = service.get_or_create_tag("alpha")
        id2 = service.get_or_create_tag("beta")
        bar._refresh_tags()

        bar._tag_checkboxes[id1].setCheckState(Qt.CheckState.Checked)
        bar._tag_checkboxes[id2].setCheckState(Qt.CheckState.Checked)

        assert bar.get_active_tag_ids() == {id1, id2}


# =========================================================================
# TestSignalEmission
# =========================================================================


class TestSignalEmission:
    """tag_filter_changed signal is emitted correctly."""

    def test_signal_on_check(
        self, service: TagService, bar: TagFilterBar
    ) -> None:
        """Checking a tag emits tag_filter_changed with the tag ID."""
        tag_id = service.get_or_create_tag("signal-tag")
        bar._refresh_tags()

        captured: list[set[int]] = []
        bar.tag_filter_changed.connect(captured.append)

        bar._tag_checkboxes[tag_id].setCheckState(Qt.CheckState.Checked)

        assert len(captured) == 1
        assert tag_id in captured[0]

    def test_signal_on_uncheck(
        self, service: TagService, bar: TagFilterBar
    ) -> None:
        """Unchecking a tag emits tag_filter_changed without the tag ID."""
        tag_id = service.get_or_create_tag("uncheck-signal")
        bar._refresh_tags()

        # First check it
        bar._tag_checkboxes[tag_id].setCheckState(Qt.CheckState.Checked)

        captured: list[set[int]] = []
        bar.tag_filter_changed.connect(captured.append)

        # Now uncheck
        bar._tag_checkboxes[tag_id].setCheckState(Qt.CheckState.Unchecked)

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

    def test_has_active_filters_false_initially(self, bar: TagFilterBar) -> None:
        """has_active_filters() is False when no tags are checked."""
        assert bar.has_active_filters() is False

    def test_has_active_filters_true_when_checked(
        self, service: TagService, bar: TagFilterBar
    ) -> None:
        """has_active_filters() is True when a tag is checked."""
        tag_id = service.get_or_create_tag("active")
        bar._refresh_tags()
        bar._tag_checkboxes[tag_id].setCheckState(Qt.CheckState.Checked)
        assert bar.has_active_filters() is True

    def test_clear_filters(
        self, service: TagService, bar: TagFilterBar
    ) -> None:
        """clear_filters() unchecks all tags and emits empty set."""
        id1 = service.get_or_create_tag("one")
        id2 = service.get_or_create_tag("two")
        bar._refresh_tags()

        bar._tag_checkboxes[id1].setCheckState(Qt.CheckState.Checked)
        bar._tag_checkboxes[id2].setCheckState(Qt.CheckState.Checked)
        assert bar.has_active_filters() is True

        captured: list[set[int]] = []
        bar.tag_filter_changed.connect(captured.append)

        bar.clear_filters()

        assert bar.get_active_tag_ids() == set()
        assert bar.has_active_filters() is False
        assert len(captured) == 1
        assert captured[0] == set()

    def test_button_label_updates(
        self, service: TagService, bar: TagFilterBar
    ) -> None:
        """Toggle button label shows count of active filters."""
        tag_id = service.get_or_create_tag("counted")
        bar._refresh_tags()

        assert bar._toggle_btn.text() == "Tags"

        bar._tag_checkboxes[tag_id].setCheckState(Qt.CheckState.Checked)
        assert bar._toggle_btn.text() == "Tags (1)"

    def test_button_label_clears(
        self, service: TagService, bar: TagFilterBar
    ) -> None:
        """Toggle button label resets when filters are cleared."""
        tag_id = service.get_or_create_tag("temp")
        bar._refresh_tags()

        bar._tag_checkboxes[tag_id].setCheckState(Qt.CheckState.Checked)
        assert bar._toggle_btn.text() == "Tags (1)"

        bar.clear_filters()
        assert bar._toggle_btn.text() == "Tags"


# =========================================================================
# TestRefreshPreservesSelection
# =========================================================================


class TestRefreshPreservesSelection:
    """Tag list refresh preserves active filter selections."""

    def test_active_tags_survive_refresh(
        self, service: TagService, bar: TagFilterBar
    ) -> None:
        """Active tag IDs are preserved when the tag list is refreshed."""
        tag_id = service.get_or_create_tag("persistent")
        bar._refresh_tags()

        bar._tag_checkboxes[tag_id].setCheckState(Qt.CheckState.Checked)
        assert tag_id in bar.get_active_tag_ids()

        # Refresh (e.g., due to tagsChanged signal)
        bar._refresh_tags()

        # The active set should still contain the tag
        assert tag_id in bar.get_active_tag_ids()
        # And the checkbox should be re-checked
        assert bar._tag_checkboxes[tag_id].checkState() == Qt.CheckState.Checked
