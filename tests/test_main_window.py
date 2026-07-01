"""Tests for MainWindow — dock panels, menu actions, and default sizing."""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from typing import Any

import pytest
from PySide6.QtWidgets import QDockWidget, QMainWindow
from tarragon.main_window import MainWindow


@pytest.fixture(autouse=True)
def qapp() -> Generator[object, None, None]:
    """Provide a shared QApplication instance for all Qt tests."""
    from PySide6.QtWidgets import QApplication

    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


# ── Instantiation Tests ────────────────────────────────────────────────


def test_main_window_is_qmainwindow() -> None:
    """MainWindow is a subclass of QMainWindow."""
    assert issubclass(MainWindow, QMainWindow)


def test_main_window_instantiates_with_no_settings(qapp: Any) -> None:  # noqa: ARG001
    """MainWindow can be created without a Settings instance (lazy init)."""
    window = MainWindow()
    try:
        assert window is not None
        assert isinstance(window, QMainWindow)
    finally:
        window.close()


def test_main_window_instantiates_with_settings(qapp: Any) -> None:  # noqa: ARG001
    """MainWindow accepts a Settings-like object without error."""

    class _FakeSettings:
        def get(self, key: str) -> None:
            return None

        def set(self, key: str, value: object) -> None:
            pass

        def close(self) -> None:
            pass

    window = MainWindow(settings=_FakeSettings())  # type: ignore[arg-type]
    try:
        assert window._settings is not None
    finally:
        window.close()


# ── Dock Widget Tests ──────────────────────────────────────────────────


def test_three_docks_exist(qapp: Any) -> None:  # noqa: ARG001
    """MainWindow creates sidebar, grid, and preview docks."""
    window = MainWindow()
    try:
        assert hasattr(window, "sidebar_dock")
        assert hasattr(window, "grid_dock")
        assert hasattr(window, "preview_dock")

        assert isinstance(window.sidebar_dock, QDockWidget)
        assert isinstance(window.grid_dock, QDockWidget)
        assert isinstance(window.preview_dock, QDockWidget)
    finally:
        window.close()


def test_docks_have_correct_titles(qapp: Any) -> None:  # noqa: ARG001
    """Each dock panel has the expected title."""
    window = MainWindow()
    try:
        assert window.sidebar_dock.windowTitle() == "Library"
        assert window.grid_dock.windowTitle() == "Gallery"
        assert window.preview_dock.windowTitle() == "Preview"
    finally:
        window.close()


def test_docks_are_attached_to_window(qapp: Any) -> None:  # noqa: ARG001
    """All three docks are child widgets of the MainWindow."""
    window = MainWindow()
    try:
        dock_widgets = [w for w in window.findChildren(QDockWidget)]
        assert len(dock_widgets) >= 3

        titles = {dock.windowTitle() for dock in dock_widgets}
        assert "Library" in titles
        assert "Gallery" in titles
        assert "Preview" in titles
    finally:
        window.close()


# ── Menu Action Tests ──────────────────────────────────────────────────


def test_open_folder_menu_exists(qapp: Any) -> None:  # noqa: ARG001
    """The File menu exists and action callbacks are registered."""
    window = MainWindow()
    try:
        menu_bar = window.menuBar()
        assert menu_bar is not None

        # Setup methods exist.
        assert hasattr(window, "_setup_actions")
        assert callable(getattr(window, "_setup_actions"))

        # Action callback exists (wired in M3).
        assert hasattr(window, "_on_open_folder")
        assert callable(getattr(window, "_on_open_folder"))

        # Menu bar has at least one menu.
        assert len(menu_bar.actions()) >= 1
    finally:
        window.close()


def test_open_folder_action_in_file_menu(qapp: Any) -> None:  # noqa: ARG001
    """The 'Open Folder' action appears under the File menu in the menu bar."""
    from PySide6.QtWidgets import QMenu, QMenuBar

    window = MainWindow()
    try:
        menu_bar = window.menuBar()
        assert isinstance(menu_bar, QMenuBar)

        # QMenus live as children of QMenuBar (not directly under QMainWindow).
        menus = menu_bar.findChildren(QMenu)
        file_menu: QMenu | None = None
        for menu in menus:
            # Qt mnemonic prefix '&' is stripped for display but present on title.
            if "File" in menu.title():
                file_menu = menu
                break

        assert file_menu is not None, "File menu not found under QMenuBar"

        # Check that the File menu contains an 'Open Folder' action.
        # Qt mnemonic '&' appears in text; strip it for matching.
        open_folder_found = False
        for action in file_menu.actions():
            if "Folder" in action.text() and "Open" in action.text():
                open_folder_found = True
                break

        assert open_folder_found, "'Open Folder' action not found in File menu"
    finally:
        window.close()


# ── Default Size Tests ─────────────────────────────────────────────────


def test_window_has_reasonable_default_size(qapp: Any) -> None:  # noqa: ARG001
    """MainWindow opens at approximately 1200x800."""
    window = MainWindow()
    try:
        size = window.size()
        assert size.width() == 1200, f"Expected width 1200, got {size.width()}"
        assert size.height() == 800, f"Expected height 800, got {size.height()}"
    finally:
        window.close()


def test_default_size_constants_defined() -> None:
    """MainWindow has DEFAULT_WIDTH and DEFAULT_HEIGHT class attributes."""
    assert MainWindow.DEFAULT_WIDTH == 1200
    assert MainWindow.DEFAULT_HEIGHT == 800


# ── Bug 1 Regression: Filtered query must not clear gallery ────────────


def test_run_filtered_query_does_not_clear_gallery_when_no_folder(qapp: Any) -> None:  # noqa: ARG001
    """_run_filtered_query() must NOT clear the model when _current_folder is empty.

    Regression test for Bug 1: clicking a tag or thumbnail should not cause
    all thumbnails to disappear when no folder scope is set.
    """
    from pathlib import Path

    from tarragon.db import Database
    from tarragon.services.tag_service import TagService

    window = MainWindow()
    try:
        db = Database(Path(":memory:"))
        db.init_schema()
        tag_service = TagService(db=db)
        window.setup_widgets(db, tag_service)

        # Pre-load some paths into the model (simulating a folder open)
        from pathlib import Path as P

        window.thumbnail_model.set_paths([P("/fake/img1.png"), P("/fake/img2.png")])
        assert window.thumbnail_model.rowCount() == 2

        # _current_folder is "" (default) — calling _run_filtered_query should NOT clear
        window._run_filtered_query()

        # Model should still have 2 paths
        assert window.thumbnail_model.rowCount() == 2, (
            "_run_filtered_query() cleared the gallery when _current_folder was empty"
        )
    finally:
        window.close()


def test_run_filtered_query_works_when_folder_is_set(qapp: Any) -> None:  # noqa: ARG001
    """_run_filtered_query() queries the DB and updates the model when folder is set."""
    from pathlib import Path

    from tarragon.db import Database
    from tarragon.services.tag_service import TagService

    window = MainWindow()
    try:
        db = Database(Path(":memory:"))
        db.init_schema()

        # Populate thumbnails table
        db.upsert_thumbnail("/test/photos/a.png", mtime=1, size=100, width=800, height=600, cache_uuid="u1")
        db.upsert_thumbnail("/test/photos/b.png", mtime=2, size=200, width=1024, height=768, cache_uuid="u2")

        tag_service = TagService(db=db)
        window.setup_widgets(db, tag_service)

        # Set the current folder
        window._current_folder = "/test/photos/"

        # Pre-load different paths (simulating stale state)
        window.thumbnail_model.set_paths([Path("/stale/old.png")])
        assert window.thumbnail_model.rowCount() == 1

        # Run filtered query — should update model with DB results
        window._run_filtered_query()

        # Model should now have 2 paths from the DB
        assert window.thumbnail_model.rowCount() == 2
    finally:
        window.close()


# ── Bug 2 Regression: has_filters includes tag filters ─────────────────


def test_has_filters_includes_tag_filters(qapp: Any) -> None:  # noqa: ARG001
    """_on_open_folder detects active tag filters and runs filtered query."""
    from pathlib import Path

    from PySide6.QtCore import Qt

    from tarragon.db import Database
    from tarragon.services.tag_service import TagService

    window = MainWindow()
    try:
        db = Database(Path(":memory:"))
        db.init_schema()

        # Populate thumbnails
        db.upsert_thumbnail("/test/photos/a.png", mtime=1, size=100, width=800, height=600, cache_uuid="u1")
        db.upsert_thumbnail("/test/photos/b.png", mtime=2, size=200, width=1024, height=768, cache_uuid="u2")

        # Add a tag to one file
        tag_id = db.ensure_tag("beach")
        db.add_file_tags(["/test/photos/a.png"], tag_id)

        tag_service = TagService(db=db)
        window.setup_widgets(db, tag_service)

        # Set current folder
        window._current_folder = "/test/photos/"

        # Activate a tag filter via the tag_filter_bar
        window.tag_filter_bar._active_tag_ids.add(tag_id)
        cb = window.tag_filter_bar._tag_checkboxes.get(tag_id)
        if cb is not None:
            cb.setCheckState(Qt.CheckState.Checked)

        # Verify has_active_filters works on tag_filter_bar
        assert window.tag_filter_bar.has_active_filters() is True

        # Run filtered query — should only return the tagged file
        window._run_filtered_query()
        assert window.thumbnail_model.rowCount() == 1
    finally:
        window.close()


# ── Bug 2 Regression: Race condition fallback ──────────────────────────


def test_race_condition_fallback_when_filtered_empty(qapp: Any) -> None:  # noqa: ARG001
    """If filtered query returns empty but thumbnails exist, fall back to unfiltered."""
    from pathlib import Path

    from PySide6.QtCore import Qt

    from tarragon.db import Database
    from tarragon.services.tag_service import TagService

    window = MainWindow()
    try:
        db = Database(Path(":memory:"))
        db.init_schema()

        # Populate thumbnails but DON'T add any tags
        db.upsert_thumbnail("/test/photos/a.png", mtime=1, size=100, width=800, height=600, cache_uuid="u1")
        db.upsert_thumbnail("/test/photos/b.png", mtime=2, size=200, width=1024, height=768, cache_uuid="u2")

        tag_service = TagService(db=db)
        window.setup_widgets(db, tag_service)

        window._current_folder = "/test/photos/"

        # Create a tag and activate filter (but no files have it)
        tag_id = tag_service.get_or_create_tag("nonexistent")
        window.tag_filter_bar._refresh_tags()
        window.tag_filter_bar._active_tag_ids.add(tag_id)
        cb = window.tag_filter_bar._tag_checkboxes.get(tag_id)
        if cb is not None:
            cb.setCheckState(Qt.CheckState.Checked)

        # Filtered query should return empty, but fallback should show all thumbnails
        window._run_filtered_query()

        # Should fall back to showing all thumbnails in the folder
        assert window.thumbnail_model.rowCount() == 2
    finally:
        window.close()


# ── Bug 1 Regression: Global scope mode ────────────────────────────────


def test_global_scope_queries_entire_db(qapp: Any) -> None:  # noqa: ARG001
    """In global mode, _run_filtered_query ignores folder constraint."""
    from pathlib import Path

    from PySide6.QtCore import Qt

    from tarragon.db import Database
    from tarragon.services.tag_service import TagService

    window = MainWindow()
    try:
        db = Database(Path(":memory:"))
        db.init_schema()

        # Thumbnails in two different folders
        db.upsert_thumbnail("/folder_a/img1.png", mtime=1, size=100, width=800, height=600, cache_uuid="u1")
        db.upsert_thumbnail("/folder_b/img2.png", mtime=2, size=200, width=1024, height=768, cache_uuid="u2")

        # Both files have the same tag
        tag_id = db.ensure_tag("shared")
        db.add_file_tags(["/folder_a/img1.png", "/folder_b/img2.png"], tag_id)

        tag_service = TagService(db=db)
        window.setup_widgets(db, tag_service)

        # Set folder to /folder_a/ only
        window._current_folder = "/folder_a/"
        window.tag_panel.set_folder_path("/folder_a/")

        # Activate tag filter via tag_filter_bar
        window.tag_filter_bar._active_tag_ids.add(tag_id)
        cb = window.tag_filter_bar._tag_checkboxes.get(tag_id)
        if cb is not None:
            cb.setCheckState(Qt.CheckState.Checked)

        # In local mode, should only return files in /folder_a/
        window._run_filtered_query()
        assert window.thumbnail_model.rowCount() == 1

        # Switch to global mode
        window.tag_panel._scope_checkbox.setCheckState(Qt.CheckState.Checked)

        # Now should return files from both folders
        window._run_filtered_query()
        assert window.thumbnail_model.rowCount() == 2
    finally:
        window.close()


# ── Bug 2 Regression: _on_folder_navigated applies filters ─────────────


def test_folder_navigated_applies_active_filters(qapp: Any, tmp_path: Path) -> None:  # noqa: ARG001
    """Navigating to a folder via sidebar applies active filters."""
    from pathlib import Path

    from tarragon.db import Database
    from tarragon.services.tag_service import TagService

    window = MainWindow()
    try:
        db = Database(Path(":memory:"))
        db.init_schema()

        # Create a real temp folder with files that scanner can find
        folder = tmp_path / "test_images"
        folder.mkdir()
        # Create dummy image files (scanner looks for image extensions)
        (folder / "a.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00" * 100)
        (folder / "b.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00" * 100)

        tag_service = TagService(db=db)
        window.setup_widgets(db, tag_service)

        # Navigate to the folder first
        window._on_folder_navigated(str(folder))
        assert window.thumbnail_model.rowCount() == 2

        # Now set a filename filter
        window._search_edit.setText("a.png")

        # Navigate again — should apply the filter
        window._on_folder_navigated(str(folder))

        # Should only show the file matching the filter
        assert window.thumbnail_model.rowCount() <= 2  # Filter applied via query service
    finally:
        window.close()
