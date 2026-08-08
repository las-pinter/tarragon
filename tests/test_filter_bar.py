"""Tests for FilterBar — combined filter row widget.

Covers:
    - Widget creation with sub-widgets (FilterBarColor, FilterBarTag, folder chips)
    - Signal forwarding from child widgets
    - Folder chip visibility toggle via set_scope()
    - Folder chip creation and removal
    - Folder filter signal emission (set[str])
    - Add Folder menu population
"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest
from tarragon.db.database import Database
from tarragon.services.tag_service import TagService
from tarragon.widgets.filter_bar import FilterBar
from tarragon.widgets.filter_bar_color import FilterBarColor
from tarragon.widgets.filter_bar_tag import FilterBarTag

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
        """FilterBar contains a FilterBarColor sub-widget."""
        assert isinstance(bar.filter_bar_color, FilterBarColor)

    def test_has_tag_filter_bar(self, bar: FilterBar) -> None:
        """FilterBar contains a FilterBarTag sub-widget."""
        assert isinstance(bar.filter_bar_tag, FilterBarTag)

    def test_no_qcombobox(self, bar: FilterBar) -> None:
        """FilterBar no longer contains a QComboBox (replaced by chips)."""
        from PySide6.QtWidgets import QComboBox

        combos = bar.findChildren(QComboBox)
        assert len(combos) == 0


# =========================================================================
# TestSignalForwarding
# =========================================================================


class TestSignalForwarding:
    """Signals from child widgets are forwarded through FilterBar."""

    def test_color_filter_changed_forwarded(self, bar: FilterBar) -> None:
        """color_filter_changed signal is forwarded from FilterBarColor."""
        captured: list[set[str]] = []
        bar.color_filter_changed.connect(captured.append)

        bar.filter_bar_color.toggle_color("red")

        assert len(captured) == 1
        assert captured[0] == {"color:red"}

    def test_tag_filter_changed_forwarded(self, bar: FilterBar, tag_service: TagService) -> None:
        """tag_filter_changed signal is forwarded from FilterBarTag."""
        tag_id = tag_service._get_or_create_tag("test-tag")
        bar.filter_bar_tag._refresh_tags()

        captured: list[set[int]] = []
        bar.tag_filter_changed.connect(captured.append)

        bar.filter_bar_tag._toggle_tag(tag_id)

        assert len(captured) == 1
        assert tag_id in captured[0]

    def test_folder_filter_changed_forwarded(self, bar: FilterBar, db: Database) -> None:
        """tag_filter_changed signal is forwarded from FilterBarTag."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")

        captured: list[set[int]] = []
        bar.folder_filter_changed.connect(captured.append)

        bar.filter_bar_folder._toggle_folder("/photos/vacation")

        assert len(captured) == 1
        assert "/photos/vacation" in captured[0]
