"""Tests for FilterBar — combined filter row widget.

Covers:
    - Widget creation with sub-widgets (ColorFilterBar, TagFilterBar, folder chips)
    - Signal forwarding from child widgets
    - Folder chip visibility toggle via set_scope()
    - Folder chip creation and removal
    - Folder filter signal emission (set[str])
    - Add Folder+ menu population
"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest
from PySide6.QtWidgets import QPushButton, QWidget

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

    def test_has_add_folder_button(self, bar: FilterBar) -> None:
        """FilterBar contains an 'Add Folder+' button."""
        btns = bar.findChildren(QPushButton)
        folder_btns = [b for b in btns if b.text() == "Add Folder+"]
        assert len(folder_btns) == 1

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
        """color_filter_changed signal is forwarded from ColorFilterBar."""
        captured: list[set[str]] = []
        bar.color_filter_changed.connect(captured.append)

        bar.color_filter_bar.toggle_color("red")

        assert len(captured) == 1
        assert captured[0] == {"color:red"}

    def test_tag_filter_changed_forwarded(self, bar: FilterBar, tag_service: TagService) -> None:
        """tag_filter_changed signal is forwarded from TagFilterBar."""
        tag_id = tag_service._get_or_create_tag("test-tag")
        bar.tag_filter_bar._refresh_tags()

        captured: list[set[int]] = []
        bar.tag_filter_changed.connect(captured.append)

        bar.tag_filter_bar._toggle_tag(tag_id)

        assert len(captured) == 1
        assert tag_id in captured[0]

    def test_folder_filter_changed_on_chip_add(self, bar: FilterBar, db: Database) -> None:
        """folder_filter_changed signal is emitted when a folder chip is added."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/photos/work/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")

        captured: list[set[str]] = []
        bar.folder_filter_changed.connect(captured.append)

        bar._add_folder_chip("/photos/vacation")

        assert len(captured) == 1
        assert captured[0] == {"/photos/vacation"}

    def test_folder_filter_changed_emits_set(self, bar: FilterBar, db: Database) -> None:
        """folder_filter_changed emits a set of strings, not a single string."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/photos/work/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")

        captured: list[set[str]] = []
        bar.folder_filter_changed.connect(captured.append)

        bar._add_folder_chip("/photos/vacation")
        bar._add_folder_chip("/photos/work")

        assert len(captured) == 2
        assert captured[1] == {"/photos/vacation", "/photos/work"}


# =========================================================================
# TestFolderChipVisibility
# =========================================================================


class TestFolderChipVisibility:
    """Folder widgets visibility is controlled by set_scope() and selection."""

    def test_hidden_by_default(self, bar: FilterBar) -> None:
        """Add Folder+ button is hidden by default (local/folder mode)."""
        assert bar._add_folder_btn.isHidden()

    def test_visible_in_global_mode(self, bar: FilterBar) -> None:
        """Add Folder+ button becomes visible when set_scope(True) is called."""
        bar.set_scope(True)
        assert not bar._add_folder_btn.isHidden()

    def test_hidden_in_local_mode(self, bar: FilterBar) -> None:
        """Add Folder+ button is hidden when set_scope(False) is called."""
        bar.set_scope(True)  # Show it first
        bar.set_scope(False)  # Then hide it
        assert bar._add_folder_btn.isHidden()

    def test_toggle_scope(self, bar: FilterBar) -> None:
        """Toggling scope multiple times correctly shows/hides the button."""
        bar.set_scope(True)
        assert not bar._add_folder_btn.isHidden()

        bar.set_scope(False)
        assert bar._add_folder_btn.isHidden()

        bar.set_scope(True)
        assert not bar._add_folder_btn.isHidden()

    def test_chips_container_hidden_when_no_selection(self, bar: FilterBar) -> None:
        """Folder chips container is hidden when no folders are selected."""
        assert bar._folder_chips_container.isHidden()

    def test_chips_container_visible_when_folder_selected(self, bar: FilterBar, db: Database) -> None:
        """Folder chips container becomes visible when a folder is added."""
        db.upsert_thumbnail("/photos/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        bar._add_folder_chip("/photos")
        assert not bar._folder_chips_container.isHidden()

    def test_chips_container_hidden_after_all_removed(self, bar: FilterBar, db: Database) -> None:
        """Folder chips container hides again when all chips are removed."""
        db.upsert_thumbnail("/photos/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        bar._add_folder_chip("/photos")
        assert not bar._folder_chips_container.isHidden()

        bar._remove_folder_chip("/photos")
        assert bar._folder_chips_container.isHidden()


# =========================================================================
# TestFolderChipCreation
# =========================================================================


class TestFolderChipCreation:
    """Folder chips are created and tracked correctly."""

    def test_add_folder_chip_creates_widget(self, bar: FilterBar, db: Database) -> None:
        """Adding a folder creates a chip widget."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        bar._add_folder_chip("/photos/vacation")

        assert "/photos/vacation" in bar._folder_chips
        assert bar._folder_chips["/photos/vacation"] is not None

    def test_add_duplicate_folder_is_noop(self, bar: FilterBar, db: Database) -> None:
        """Adding the same folder twice does not create duplicate chips."""
        db.upsert_thumbnail("/photos/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        bar._add_folder_chip("/photos")
        bar._add_folder_chip("/photos")

        assert len(bar._selected_folders) == 1
        assert len(bar._folder_chips) == 1

    def test_remove_folder_chip(self, bar: FilterBar, db: Database) -> None:
        """Removing a folder chip removes it from tracking."""
        db.upsert_thumbnail("/photos/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        bar._add_folder_chip("/photos")
        assert "/photos" in bar._folder_chips

        bar._remove_folder_chip("/photos")
        assert "/photos" not in bar._folder_chips
        assert "/photos" not in bar._selected_folders

    def test_remove_nonexistent_folder_is_noop(self, bar: FilterBar) -> None:
        """Removing a folder that isn't selected is a no-op."""
        bar._remove_folder_chip("/nonexistent")
        assert len(bar._selected_folders) == 0

    def test_multiple_folders(self, bar: FilterBar, db: Database) -> None:
        """Multiple folders can be selected simultaneously."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/photos/work/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")
        db.upsert_thumbnail("/photos/home/c.png", mtime=3, size=300, width=10, height=10, cache_uuid="u3")

        bar._add_folder_chip("/photos/vacation")
        bar._add_folder_chip("/photos/work")
        bar._add_folder_chip("/photos/home")

        assert bar._selected_folders == {"/photos/vacation", "/photos/work", "/photos/home"}
        assert len(bar._folder_chips) == 3


# =========================================================================
# TestFolderMenu
# =========================================================================


class TestFolderMenu:
    """Add Folder+ menu shows available (unselected) folders."""

    def test_menu_shows_unselected_folders(self, bar: FilterBar, db: Database) -> None:
        """Menu only shows folders not already selected as chips."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/photos/work/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")

        bar._add_folder_chip("/photos/vacation")

        # Simulate opening the menu
        bar._on_add_folder_clicked()

        actions = bar._folder_menu.actions()
        # Should only show /photos/work (not /photos/vacation which is already selected)
        folder_names = [a.text() for a in actions if a.isEnabled()]
        assert any("work" in name for name in folder_names)
        assert not any("vacation" in name for name in folder_names)

    def test_menu_shows_disabled_when_all_added(self, bar: FilterBar, db: Database) -> None:
        """Menu shows disabled message when all folders are already selected."""
        db.upsert_thumbnail("/photos/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")

        bar._add_folder_chip("/photos")
        bar._on_add_folder_clicked()

        actions = bar._folder_menu.actions()
        assert len(actions) == 1
        assert not actions[0].isEnabled()

    def test_menu_empty_db(self, bar: FilterBar) -> None:
        """Menu shows disabled message when DB has no folders."""
        bar._on_add_folder_clicked()

        actions = bar._folder_menu.actions()
        assert len(actions) == 1
        assert not actions[0].isEnabled()


# =========================================================================
# TestFolderFilterSignal
# =========================================================================


class TestFolderFilterSignal:
    """Folder filter signal emits correct set values."""

    def test_empty_set_when_no_folders(self, bar: FilterBar) -> None:
        """No chips selected means empty set emitted."""
        captured: list[set[str]] = []
        bar.folder_filter_changed.connect(captured.append)

        # Remove a nonexistent folder — should emit empty set
        bar._remove_folder_chip("/nonexistent")

        assert len(captured) == 1
        assert captured[0] == set()

    def test_adding_folder_emits_updated_set(self, bar: FilterBar, db: Database) -> None:
        """Adding a folder emits the updated set including the new folder."""
        db.upsert_thumbnail("/alpha/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/beta/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")

        captured: list[set[str]] = []
        bar.folder_filter_changed.connect(captured.append)

        bar._add_folder_chip("/alpha")
        assert captured[-1] == {"/alpha"}

        bar._add_folder_chip("/beta")
        assert captured[-1] == {"/alpha", "/beta"}

    def test_removing_folder_emits_updated_set(self, bar: FilterBar, db: Database) -> None:
        """Removing a folder emits the updated set without the removed folder."""
        db.upsert_thumbnail("/alpha/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/beta/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")

        bar._add_folder_chip("/alpha")
        bar._add_folder_chip("/beta")

        captured: list[set[str]] = []
        bar.folder_filter_changed.connect(captured.append)

        bar._remove_folder_chip("/alpha")
        assert len(captured) == 1
        assert captured[0] == {"/beta"}


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
# TestRefreshFolders
# =========================================================================


class TestRefreshFolders:
    """refresh_folders() prunes stale chips."""

    def test_prune_stale_chips(self, bar: FilterBar, db: Database) -> None:
        """refresh_folders() removes chips for folders no longer in DB."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/photos/work/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")

        bar._add_folder_chip("/photos/vacation")
        bar._add_folder_chip("/photos/work")
        assert len(bar._selected_folders) == 2

        # Remove vacation from DB
        db.delete_thumbnails_by_folder("/photos/vacation")

        bar.refresh_folders()

        assert "/photos/vacation" not in bar._selected_folders
        assert "/photos/work" in bar._selected_folders

    def test_no_spurious_signals_during_refresh(self, bar: FilterBar, db: Database) -> None:
        """refresh_folders() only emits for actually removed chips."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        bar._add_folder_chip("/photos/vacation")

        # Connect signal after adding — should not fire during refresh
        # since the folder still exists in DB
        captured: list[set[str]] = []
        bar.folder_filter_changed.connect(captured.append)

        bar.refresh_folders()

        assert len(captured) == 0


# =========================================================================
# TestFolderChipTooltips
# =========================================================================


class TestFolderChipTooltips:
    """Folder chips have tooltips showing the full path."""

    def test_chip_has_tooltip(self, bar: FilterBar, db: Database) -> None:
        """Each folder chip label has a tooltip with the full path."""
        db.upsert_thumbnail(
            "/home/user/photos/vacation/a.png",
            mtime=1,
            size=100,
            width=10,
            height=10,
            cache_uuid="u1",
        )
        bar._add_folder_chip("/home/user/photos/vacation")

        chip = bar._folder_chips["/home/user/photos/vacation"]
        # Find the QLabel inside the chip
        from PySide6.QtWidgets import QLabel

        labels = chip.findChildren(QLabel)
        assert len(labels) >= 1
        assert labels[0].toolTip() == "/home/user/photos/vacation"


# =========================================================================
# TestFlowLayout — wrapping layout
# =========================================================================


class TestFlowLayout:
    """FlowLayout arranges items left-to-right and wraps when width is exceeded."""

    def test_single_line_when_width_sufficient(self) -> None:
        """All items stay on one line when there is enough width."""
        from PySide6.QtCore import QRect
        from PySide6.QtWidgets import QLabel

        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container, margin=0, spacing=6)

        # Three 100px-wide labels — need 100+6+100+6+100 = 312px on one line
        labels = [QLabel(f"Item {i}") for i in range(3)]
        for lbl in labels:
            lbl.setFixedSize(100, 30)
            layout.addWidget(lbl)

        # Give plenty of width
        layout.setGeometry(QRect(0, 0, 500, 400))

        # All items should be on the same y-coordinate
        y_positions = {lbl.y() for lbl in labels}
        assert len(y_positions) == 1, "All items should be on the same line"
        container.close()

    def test_wraps_when_width_narrow(self) -> None:
        """Items wrap to the next line when width is too narrow."""
        from PySide6.QtCore import QRect
        from PySide6.QtWidgets import QLabel

        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container, margin=0, spacing=6)

        # Three 100px-wide labels
        labels = [QLabel(f"Item {i}") for i in range(3)]
        for lbl in labels:
            lbl.setFixedSize(100, 30)
            layout.addWidget(lbl)

        # Only enough width for ~2 items: 100+6+100 = 206, but not 3rd (206+6+100=312)
        layout.setGeometry(QRect(0, 0, 210, 400))

        # Third item should be on a different (lower) y-coordinate
        y_positions = [lbl.y() for lbl in labels]
        assert y_positions[0] == y_positions[1], "First two items on same line"
        assert y_positions[2] > y_positions[0], "Third item wrapped to next line"
        container.close()

    def test_height_for_width(self) -> None:
        """heightForWidth returns more height when width is narrower."""
        from PySide6.QtWidgets import QLabel

        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container, margin=0, spacing=6)

        labels = [QLabel(f"Item {i}") for i in range(4)]
        for lbl in labels:
            lbl.setFixedSize(100, 30)
            layout.addWidget(lbl)

        # Wide: all 4 on one line → height ≈ 30
        wide_height = layout.heightForWidth(500)
        # Narrow: forces wrapping → height > 30
        narrow_height = layout.heightForWidth(150)

        assert narrow_height > wide_height
        container.close()

    def test_minimum_size(self) -> None:
        """minimumSize returns the largest single item plus margins."""
        from PySide6.QtWidgets import QLabel

        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container, margin=4, spacing=6)

        lbl_small = QLabel("S")
        lbl_small.setFixedSize(50, 20)
        layout.addWidget(lbl_small)

        lbl_large = QLabel("L")
        lbl_large.setFixedSize(120, 40)
        layout.addWidget(lbl_large)

        min_size = layout.minimumSize()
        # Should be at least as large as the biggest item + 2*margin
        assert min_size.width() >= 120 + 2 * 4
        assert min_size.height() >= 40 + 2 * 4
        container.close()

    def test_count_and_item_at(self) -> None:
        """count() and itemAt() correctly track added items."""
        from PySide6.QtWidgets import QLabel

        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container)

        assert layout.count() == 0
        assert layout.itemAt(0) is None

        lbl = QLabel("test")
        lbl.setFixedSize(50, 20)
        layout.addWidget(lbl)

        assert layout.count() == 1
        assert layout.itemAt(0) is not None
        assert layout.itemAt(1) is None
        container.close()

    def test_take_at_removes_item(self) -> None:
        """takeAt() removes and returns the item at the given index."""
        from PySide6.QtWidgets import QLabel

        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container)

        lbl = QLabel("test")
        lbl.setFixedSize(50, 20)
        layout.addWidget(lbl)
        assert layout.count() == 1

        removed = layout.takeAt(0)
        assert removed is not None
        assert layout.count() == 0
        container.close()

    def test_take_at_out_of_range(self) -> None:
        """takeAt() returns None for out-of-range indices."""
        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container)

        assert layout.takeAt(0) is None
        assert layout.takeAt(-1) is None
        assert layout.takeAt(100) is None
        container.close()

    def test_empty_layout_height(self) -> None:
        """heightForWidth on an empty layout returns a sensible value."""
        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container, margin=4, spacing=6)

        height = layout.heightForWidth(200)
        # Should be at least 2*margin (top + bottom)
        assert height >= 2 * 4
        container.close()

    def test_default_spacing(self) -> None:
        """Default spacing is 6 when not explicitly provided."""
        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container)

        assert layout._spacing == 6
        container.close()

    def test_explicit_spacing(self) -> None:
        """Explicit spacing value is respected."""
        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container, spacing=12)

        assert layout._spacing == 12
        container.close()

    def test_has_height_for_width(self) -> None:
        """hasHeightForWidth returns True."""
        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container)

        assert layout.hasHeightForWidth() is True
        container.close()

    def test_multiple_lines_wrapping(self) -> None:
        """Items wrap across multiple lines when width is very narrow."""
        from PySide6.QtCore import QRect
        from PySide6.QtWidgets import QLabel

        from tarragon.widgets.flow_layout import FlowLayout

        container = QWidget()
        layout = FlowLayout(container, margin=0, spacing=6)

        # Six 100px items — only room for 1 per line at width=100
        labels = [QLabel(f"Item {i}") for i in range(6)]
        for lbl in labels:
            lbl.setFixedSize(100, 30)
            layout.addWidget(lbl)

        # Width exactly fits one item
        layout.setGeometry(QRect(0, 0, 100, 600))

        y_positions = sorted({lbl.y() for lbl in labels})
        # Should have 6 separate lines
        assert len(y_positions) == 6
        container.close()
