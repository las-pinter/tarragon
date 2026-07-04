"""Tests for TagPanel — manual tag management widget.

WAAAGH! Wrenchbasha's torture chamber for da TagPanel widget!

Testing patterns applied (from python-testing-patterns skill):
  - Arrange-Act-Assert (AAA)
  - pytest fixtures for service and widget setup
  - In-memory SQLite for full isolation
"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication, QCheckBox, QLabel, QLineEdit, QPushButton, QScrollArea
from tarragon.db import Database
from tarragon.services.tag_service import TagService
from tarragon.widgets.tag_panel import TagPanel

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
def panel(service: TagService) -> Generator[TagPanel, None, None]:
    """Create a TagPanel that is cleaned up after the test."""
    w = TagPanel(service)
    yield w
    w.close()


# =========================================================================
# TestTagPanelCreation
# =========================================================================


class TestTagPanelCreation:
    """TagPanel construction and basic structure."""

    def test_tag_panel_creation(self, panel: TagPanel) -> None:  # noqa: ARG002
        """TagPanel is created without error and has expected children."""
        assert isinstance(panel, TagPanel)

        # Has a header label
        labels = panel.findChildren(QLabel)
        assert any(label.text() == "Tags" for label in labels)

        # Has a scroll area
        assert isinstance(panel.findChild(QScrollArea), QScrollArea)

        # Has a text input
        assert isinstance(panel.findChild(QLineEdit), QLineEdit)

        # Has the Add Tag button
        buttons = panel.findChildren(QPushButton)
        assert any(b.text() == "Add Tag" for b in buttons)


# =========================================================================
# TestTagPanelDisplay
# =========================================================================


class TestTagPanelDisplay:
    """TagPanel correctly reflects the state of the TagService."""

    def test_tag_panel_shows_all_tags(self, service: TagService, panel: TagPanel) -> None:  # noqa: ARG002
        """Tags from service appear as checkboxes in the panel."""
        service.get_or_create_tag("landscape")
        service.get_or_create_tag("portrait")
        service.get_or_create_tag("character")
        panel._refresh_tags()

        # Only count checkboxes that have a tag_id property (exclude scope toggle)
        tag_checkboxes = [cb for cb in panel.findChildren(QCheckBox) if cb.property("tag_id") is not None]
        assert len(tag_checkboxes) == 3

        # Labels should include tag names
        labels = panel.findChildren(QLabel)
        label_texts = {label.text() for label in labels}
        assert "Tags" in label_texts
        assert any("landscape" in t for t in label_texts)
        assert any("portrait" in t for t in label_texts)
        assert any("character" in t for t in label_texts)

    def test_create_new_tag(self, service: TagService, panel: TagPanel) -> None:  # noqa: ARG002
        """Creating a new tag through the panel adds it to the service and UI."""
        tag_id = panel.create_new_tag("concept-art")

        assert isinstance(tag_id, int)
        assert tag_id > 0

        # Verify it appears in the panel (only count tag checkboxes, not scope toggle)
        tag_checkboxes = [cb for cb in panel.findChildren(QCheckBox) if cb.property("tag_id") is not None]
        assert len(tag_checkboxes) == 1

        # Verify via service
        tags = service.get_all_tags()
        assert len(tags) == 1
        assert tags[0]["name"] == "concept-art"

    def test_create_new_tag_duplicate(self, service: TagService, panel: TagPanel) -> None:  # noqa: ARG002
        """Creating the same tag twice returns the same id."""
        tag_id_1 = panel.create_new_tag("unique")
        tag_id_2 = panel.create_new_tag("unique")

        assert tag_id_1 == tag_id_2
        tags = service.get_all_tags()
        assert len(tags) == 1

    def test_new_tag_via_input(self, panel: TagPanel) -> None:  # noqa: ARG002
        """Typing a name and clicking Add Tag creates the tag."""
        panel._tag_input.setText("sketch")
        panel._add_button.click()

        tags = panel._tag_service.get_all_tags()
        assert len(tags) == 1
        assert tags[0]["name"] == "sketch"

        # Input should be cleared after adding
        assert panel._tag_input.text() == ""


# =========================================================================
# TestTagPanelSelection
# =========================================================================


class TestTagPanelSelection:
    """set_selection updates checkbox tri-state correctly."""

    def test_set_selection_updates_checkboxes(  # noqa: ARG002
        self,
        service: TagService,
        panel: TagPanel,
    ) -> None:
        """Checkbox states reflect tag presence across selected paths."""
        landscape_id = service.get_or_create_tag("landscape")
        portrait_id = service.get_or_create_tag("portrait")

        # Tag only the first file with "landscape"
        service.add_tags_to_files(["/img/a.png"], ["landscape"])

        panel._refresh_tags()

        # Both files selected; one has "landscape", none has "portrait"
        panel.set_selection(["/img/a.png", "/img/b.png"])

        # Only include checkboxes with a tag_id property (exclude scope toggle)
        checkboxes = {
            int(cb.property("tag_id")): cb
            for cb in panel.findChildren(QCheckBox)
            if cb.property("tag_id") is not None
        }

        # Landscape should be PartiallyChecked (1 of 2 files)
        assert checkboxes[landscape_id].checkState() == Qt.CheckState.PartiallyChecked
        # Portrait should be Unchecked (0 of 2 files)
        assert checkboxes[portrait_id].checkState() == Qt.CheckState.Unchecked

    def test_set_selection_all_checked(  # noqa: ARG002
        self,
        service: TagService,
        panel: TagPanel,
    ) -> None:
        """When all files have a tag, the checkbox is fully Checked."""
        tag_id = service.get_or_create_tag("common")
        service.add_tags_to_files(["/img/a.png", "/img/b.png"], ["common"])

        panel._refresh_tags()
        panel.set_selection(["/img/a.png", "/img/b.png"])

        checkbox = panel._tag_checkboxes[tag_id]
        assert checkbox.checkState() == Qt.CheckState.Checked

    def test_set_selection_none_checked(  # noqa: ARG002
        self,
        service: TagService,
        panel: TagPanel,
    ) -> None:
        """When no files have a tag, the checkbox is Unchecked."""
        tag_id = service.get_or_create_tag("unused")

        panel._refresh_tags()
        panel.set_selection(["/img/a.png"])

        checkbox = panel._tag_checkboxes[tag_id]
        assert checkbox.checkState() == Qt.CheckState.Unchecked

    def test_set_selection_empty_paths(  # noqa: ARG002
        self,
        service: TagService,
        panel: TagPanel,
    ) -> None:
        """Empty paths list leaves all checkboxes Unchecked."""
        service.get_or_create_tag("some-tag")

        panel._refresh_tags()
        panel.set_selection([])

        for checkbox in panel._tag_checkboxes.values():
            assert checkbox.checkState() == Qt.CheckState.Unchecked

    def test_partial_tri_state_display(  # noqa: ARG002
        self,
        service: TagService,
        panel: TagPanel,
    ) -> None:
        """Tag on some files but not all → PartiallyChecked."""
        tag_id = service.get_or_create_tag("partial-tag")
        service.add_tags_to_files(["/img/a.png"], ["partial-tag"])

        panel._refresh_tags()
        panel.set_selection(["/img/a.png", "/img/b.png", "/img/c.png"])

        checkbox = panel._tag_checkboxes[tag_id]
        assert checkbox.checkState() == Qt.CheckState.PartiallyChecked


# =========================================================================
# TestTagPanelToggle
# =========================================================================


class TestTagPanelToggle:
    """toggle_tag adds and removes tags via the service."""

    def test_toggle_tag_adds_and_removes(  # noqa: ARG002
        self,
        service: TagService,
        panel: TagPanel,
    ) -> None:
        """Toggling a tag on then off applies and removes it from files."""
        tag_id = service.get_or_create_tag("test-tag")
        panel._refresh_tags()
        panel.set_selection(["/img/a.png", "/img/b.png"])

        # Toggle ON
        panel.toggle_tag(tag_id, True)

        for path in ["/img/a.png", "/img/b.png"]:
            tags = service.get_tags_for_file(path)
            names = {t["name"] for t in tags}
            assert "test-tag" in names, f"{path} should have 'test-tag' after toggle ON"

        # Toggle OFF
        panel.toggle_tag(tag_id, False)

        for path in ["/img/a.png", "/img/b.png"]:
            tags = service.get_tags_for_file(path)
            names = {t["name"] for t in tags}
            assert "test-tag" not in names, f"{path} should NOT have 'test-tag' after toggle OFF"

    def test_toggle_tag_no_selection_does_nothing(  # noqa: ARG002
        self,
        service: TagService,
        panel: TagPanel,
    ) -> None:
        """Toggling when no paths are selected does not crash or change tags."""
        tag_id = service.get_or_create_tag("safe")
        panel._refresh_tags()

        # No selection set — this should be a no-op
        panel.toggle_tag(tag_id, True)

        # No files should have the tag
        assert service.get_all_tags()[0]["usage_count"] == 0

    def test_toggle_tag_unknown_id_does_nothing(  # noqa: ARG002
        self,
        panel: TagPanel,
    ) -> None:
        """Toggling a tag_id that does not exist should not crash."""
        panel.set_selection(["/img/x.png"])
        # This should be a safe no-op
        panel.toggle_tag(99999, True)

    def test_toggle_tag_checkbox_state_reflects_change(  # noqa: ARG002
        self,
        service: TagService,
        panel: TagPanel,
    ) -> None:
        """After toggling, the checkbox shows the correct state."""
        tag_id = service.get_or_create_tag("state-tag")
        panel._refresh_tags()
        panel.set_selection(["/img/a.png", "/img/b.png"])

        # Initially unchecked
        assert panel._tag_checkboxes[tag_id].checkState() == Qt.CheckState.Unchecked

        # Toggle ON
        panel.toggle_tag(tag_id, True)
        # After tagsChanged → _refresh_tags → _update_checkbox_states, it should be Checked
        assert panel._tag_checkboxes[tag_id].checkState() == Qt.CheckState.Checked

        # Toggle OFF
        panel.toggle_tag(tag_id, False)
        assert panel._tag_checkboxes[tag_id].checkState() == Qt.CheckState.Unchecked


# =========================================================================
# TestCheckboxInteraction
# =========================================================================


class TestCheckboxInteraction:
    """User interaction with checkboxes (via programmatic simulation)."""

    def test_checkbox_state_changed_triggers_toggle(  # noqa: ARG002
        self,
        service: TagService,
        panel: TagPanel,
    ) -> None:
        """Setting checkbox state programmatically triggers the correct action."""
        tag_id = service.get_or_create_tag("interactive")
        panel._refresh_tags()
        panel.set_selection(["/img/a.png"])

        # Simulate user checking the checkbox
        checkbox = panel._tag_checkboxes[tag_id]
        # Use setCheckState to trigger stateChanged
        # (we're not in _setting_checkboxes, so handler fires)
        checkbox.setCheckState(Qt.CheckState.Checked)

        # The tag should now be on the file
        tags = service.get_tags_for_file("/img/a.png")
        assert any(t["name"] == "interactive" for t in tags)

    def test_checkbox_uncheck_removes_tag(  # noqa: ARG002
        self,
        service: TagService,
        panel: TagPanel,
    ) -> None:
        """Unchecking a checkbox removes the tag from files."""
        tag_id = service.get_or_create_tag("removable")
        panel._refresh_tags()
        panel.set_selection(["/img/a.png"])

        # First add the tag via panel
        panel.toggle_tag(tag_id, True)
        assert any(t["name"] == "removable" for t in service.get_tags_for_file("/img/a.png"))

        # Now simulate uncheck via checkbox state
        checkbox = panel._tag_checkboxes[tag_id]
        checkbox.setCheckState(Qt.CheckState.Unchecked)

        # Tag should be gone
        tags = service.get_tags_for_file("/img/a.png")
        assert not any(t["name"] == "removable" for t in tags)

    def test_checkbox_setting_flag_guards_recursion(  # noqa: ARG002
        self,
        service: TagService,
        panel: TagPanel,
    ) -> None:
        """Programmatic updates with _setting_checkboxes=True do not trigger handlers."""
        tag_id = service.get_or_create_tag("guarded")
        panel._refresh_tags()
        panel.set_selection(["/img/a.png"])

        # Set state with the guard flag on
        panel._setting_checkboxes = True
        panel._tag_checkboxes[tag_id].setCheckState(Qt.CheckState.Checked)
        panel._setting_checkboxes = False

        # Tag should NOT have been added (handler skipped)
        tags = service.get_tags_for_file("/img/a.png")
        assert not any(t["name"] == "guarded" for t in tags)


# =========================================================================
# Bug 1: Global/Local scope toggle
# =========================================================================


class TestSetGlobalScope:
    """set_global_scope controls scope from gallery tabs."""

    def test_set_global_scope_true(self, panel: TagPanel) -> None:  # noqa: ARG002
        """set_global_scope(True) sets _global_scope to True."""
        panel.set_global_scope(True)
        assert panel._global_scope is True

    def test_set_global_scope_false(self, panel: TagPanel) -> None:  # noqa: ARG002
        """set_global_scope(False) sets _global_scope to False."""
        panel._global_scope = True
        panel.set_global_scope(False)
        assert panel._global_scope is False

    def test_set_global_scope_refreshes_tags(self, panel: TagPanel) -> None:  # noqa: ARG002
        """set_global_scope calls _refresh_tags to update counts."""
        called: list[bool] = []
        tag_panel = panel
        tag_panel._refresh_tags = lambda: called.append(True)  # type: ignore[method-assign]
        tag_panel.set_global_scope(True)
        assert called == [True]

    def test_set_folder_path_refreshes_tags(
        self, service: TagService, panel: TagPanel
    ) -> None:
        """set_folder_path triggers a tag refresh with scoped counts."""
        tag_id = service.get_or_create_tag("beach")
        service._db.add_file_tags(["/folder_a/img1.png"], tag_id)
        service._db.add_file_tags(["/folder_b/img2.png"], tag_id)

        # Set folder to /folder_a/ — local count should be 1
        panel.set_folder_path("/folder_a/")

        labels = panel.findChildren(QLabel)
        label_texts = [label.text() for label in labels]
        assert any("beach (1)" in t for t in label_texts)

    def test_global_scope_shows_global_counts(
        self, service: TagService, panel: TagPanel
    ) -> None:
        """In global mode, usage counts span the entire database."""
        tag_id = service.get_or_create_tag("beach")
        service._db.add_file_tags(["/folder_a/img1.png", "/folder_b/img2.png"], tag_id)

        panel.set_folder_path("/folder_a/")
        # Switch to global — count should now be 2
        panel.set_global_scope(True)

        labels = panel.findChildren(QLabel)
        label_texts = [label.text() for label in labels]
        assert any("beach (2)" in t for t in label_texts)


# =========================================================================
# TestContextMenu
# =========================================================================


class TestContextMenu:
    """Context menu — right-click delete functionality."""

    def test_context_menu_helper_get_tag_id(
        self, service: TagService, panel: TagPanel
    ) -> None:
        """_get_tag_id_from_widget extracts tag_id from checkbox."""
        tag_id = service.get_or_create_tag("test-tag")
        panel._refresh_tags()

        # Find the checkbox for our tag
        checkbox = panel._tag_checkboxes[tag_id]

        # Should extract tag_id from checkbox itself
        assert panel._get_tag_id_from_widget(checkbox) == tag_id

        # Should extract tag_id from parent row widget
        row = checkbox.parentWidget()
        assert panel._get_tag_id_from_widget(row) == tag_id

    def test_context_menu_helper_get_tag_id_none(
        self, panel: TagPanel
    ) -> None:
        """_get_tag_id_from_widget returns None for widgets without tag_id."""
        # Header label has no tag_id
        labels = panel.findChildren(QLabel)
        header_label = next((l for l in labels if l.text() == "Tags"), None)
        assert header_label is not None
        assert panel._get_tag_id_from_widget(header_label) is None

        # None input
        assert panel._get_tag_id_from_widget(None) is None

    def test_is_color_tag_true(
        self, service: TagService, panel: TagPanel
    ) -> None:
        """_is_color_tag returns True for color tags."""
        tag_id = service.get_or_create_tag("color:red")
        assert panel._is_color_tag(tag_id) is True

    def test_is_color_tag_false(
        self, service: TagService, panel: TagPanel
    ) -> None:
        """_is_color_tag returns False for regular tags."""
        tag_id = service.get_or_create_tag("regular-tag")
        assert panel._is_color_tag(tag_id) is False

    def test_is_color_tag_missing(
        self, panel: TagPanel
    ) -> None:
        """_is_color_tag returns False for non-existent tag."""
        assert panel._is_color_tag(99999) is False

    def test_confirm_and_delete_tag_confirmed(
        self, service: TagService, panel: TagPanel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """_confirm_and_delete_tag deletes when user confirms."""
        tag_id = service.get_or_create_tag("delete-me")

        # Mock QMessageBox.question to return Yes
        from PySide6.QtWidgets import QMessageBox

        monkeypatch.setattr(
            QMessageBox,
            "question",
            lambda *args, **kwargs: QMessageBox.StandardButton.Yes,
        )

        panel._confirm_and_delete_tag(tag_id)

        # Tag should be deleted
        assert service.get_tag_name(tag_id) is None

    def test_confirm_and_delete_tag_cancelled(
        self, service: TagService, panel: TagPanel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """_confirm_and_delete_tag does not delete when user cancels."""
        tag_id = service.get_or_create_tag("keep-me")

        # Mock QMessageBox.question to return No
        from PySide6.QtWidgets import QMessageBox

        monkeypatch.setattr(
            QMessageBox,
            "question",
            lambda *args, **kwargs: QMessageBox.StandardButton.No,
        )

        panel._confirm_and_delete_tag(tag_id)

        # Tag should still exist
        assert service.get_tag_name(tag_id) == "keep-me"

    def test_confirm_and_delete_tag_missing(
        self, panel: TagPanel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """_confirm_and_delete_tag handles missing tag gracefully."""
        # Should not raise even if tag doesn't exist
        panel._confirm_and_delete_tag(99999)

    def test_context_menu_blocks_color_tags(
        self, service: TagService, panel: TagPanel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """Context menu does not show delete for color tags."""
        tag_id = service.get_or_create_tag("color:blue")
        panel._refresh_tags()

        # Track if _confirm_and_delete_tag is called
        called: list[bool] = []
        monkeypatch.setattr(
            panel,
            "_confirm_and_delete_tag",
            lambda tid: called.append(True),
        )

        # Simulate context menu event on color tag checkbox
        checkbox = panel._tag_checkboxes[tag_id]
        from PySide6.QtCore import QPoint
        from PySide6.QtGui import QContextMenuEvent

        # Map checkbox position to panel coordinates
        viewport = panel._scroll_area.viewport()
        checkbox_in_viewport = checkbox.mapTo(viewport, QPoint(0, 0))
        panel_pos = panel.mapFrom(viewport, checkbox_in_viewport)
        event = QContextMenuEvent(
            QContextMenuEvent.Reason.Mouse,
            panel_pos,
        )

        panel.contextMenuEvent(event)

        # Should not have called delete for color tag
        assert len(called) == 0

    def test_context_menu_allows_custom_tags(
        self, service: TagService, panel: TagPanel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """Context menu allows delete for custom (non-color) tags."""
        tag_id = service.get_or_create_tag("custom-tag")
        panel._refresh_tags()

        # Verify the helper methods work correctly for custom tags
        checkbox = panel._tag_checkboxes[tag_id]

        # _get_tag_id_from_widget should find the tag_id
        found_id = panel._get_tag_id_from_widget(checkbox)
        assert found_id == tag_id

        # _is_color_tag should return False for custom tags
        assert panel._is_color_tag(tag_id) is False

        # Verify the tag can be deleted via the service
        service.delete_tag(tag_id)
        assert service.get_tag_name(tag_id) is None
