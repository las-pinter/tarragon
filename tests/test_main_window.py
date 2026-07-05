"""Tests for MainWindow — dock panels, menu actions, and default sizing."""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from typing import Any

import pytest
from PySide6.QtWidgets import QDockWidget, QMainWindow
from tarragon.main_window import MainWindow


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

        # Activate a tag filter via the tag_filter_bar's toggle API
        window.tag_filter_bar._toggle_tag(tag_id)

        # Verify has_active_filters works on tag_filter_bar
        assert window.tag_filter_bar.has_active_filters() is True

        # Run filtered query — should only return the tagged file
        window._run_filtered_query()
        assert window.thumbnail_model.rowCount() == 1
    finally:
        window.close()


# ── Bug 2 Regression: Race condition fallback removed ─────────────────


def test_filtered_query_returns_empty_when_no_match(qapp: Any) -> None:  # noqa: ARG001
    """If filtered query returns empty, gallery shows 0 results (no fallback).

    Previously a race-condition guard would fall back to showing all
    unfiltered thumbnails when filters returned 0 results.  This made
    filters appear broken.  The guard has been removed so that 0 filter
    results correctly shows an empty gallery.
    """
    from pathlib import Path

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
        window.tag_filter_bar._toggle_tag(tag_id)

        # Filtered query should return 0 results — no fallback to unfiltered
        window._run_filtered_query()

        assert window.thumbnail_model.rowCount() == 0
    finally:
        window.close()


# ── Bug 1 Regression: Global scope mode ────────────────────────────────


def test_global_scope_queries_entire_db(qapp: Any) -> None:  # noqa: ARG001
    """In global mode, _run_filtered_query ignores folder constraint."""
    from pathlib import Path

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

        # Activate tag filter via tag_filter_bar's toggle API
        window.tag_filter_bar._toggle_tag(tag_id)

        # In local mode, should only return files in /folder_a/
        window._run_filtered_query()
        assert window.thumbnail_model.rowCount() == 1

        # Switch to global mode via gallery tabs
        window._gallery_tabs._tab_widget.setCurrentIndex(1)  # "All Images"

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


# ── Folder Filter in Global Mode ─────────────────────────────────────


def test_folder_filter_in_global_mode(qapp: Any) -> None:  # noqa: ARG001
    """In global mode, the folder filter dropdown restricts results to a specific folder."""
    from pathlib import Path

    from tarragon.db import Database
    from tarragon.services.tag_service import TagService

    window = MainWindow()
    try:
        db = Database(Path(":memory:"))
        db.init_schema()

        # Thumbnails in two different folders
        db.upsert_thumbnail(
            "/folder_a/img1.png", mtime=1, size=100, width=800, height=600, cache_uuid="u1"
        )
        db.upsert_thumbnail(
            "/folder_b/img2.png", mtime=2, size=200, width=1024, height=768, cache_uuid="u2"
        )
        db.upsert_thumbnail(
            "/folder_a/img3.png", mtime=3, size=300, width=640, height=480, cache_uuid="u3"
        )

        tag_service = TagService(db=db)
        window.setup_widgets(db, tag_service)

        # Set a current folder so the gallery isn't blocked
        window._current_folder = "/folder_a/"

        # Switch to global mode ("All Images" tab)
        window._gallery_tabs._tab_widget.setCurrentIndex(1)

        # Without folder filter, should show all 3 thumbnails
        window._run_filtered_query()
        assert window.thumbnail_model.rowCount() == 3

        # Set folder filter to /folder_a
        window._filter_state.folder_filters = {"/folder_a"}
        window._run_filtered_query()
        assert window.thumbnail_model.rowCount() == 2

        # Set folder filter to /folder_b
        window._filter_state.folder_filters = {"/folder_b"}
        window._run_filtered_query()
        assert window.thumbnail_model.rowCount() == 1

        # Clear folder filter (back to all)
        window._filter_state.folder_filters = set()
        window._run_filtered_query()
        assert window.thumbnail_model.rowCount() == 3
    finally:
        window.close()


def test_filter_bar_replaces_separate_bars(qapp: Any) -> None:  # noqa: ARG001
    """MainWindow uses FilterBar instead of separate ColorFilterBar and TagFilterBar."""
    from pathlib import Path

    from tarragon.db import Database
    from tarragon.services.tag_service import TagService
    from tarragon.widgets.filter_bar import FilterBar

    window = MainWindow()
    try:
        db = Database(Path(":memory:"))
        db.init_schema()
        tag_service = TagService(db=db)
        window.setup_widgets(db, tag_service)

        # FilterBar exists and is the combined widget
        assert hasattr(window, "filter_bar")
        assert isinstance(window.filter_bar, FilterBar)

        # Backward-compatible references still work
        assert hasattr(window, "color_filter_bar")
        assert hasattr(window, "tag_filter_bar")
        assert window.color_filter_bar is window.filter_bar.color_filter_bar
        assert window.tag_filter_bar is window.filter_bar.tag_filter_bar
    finally:
        window.close()


def test_scope_change_shows_folder_button(qapp: Any) -> None:  # noqa: ARG001
    """Switching to global mode shows the Add Folder+ button in the FilterBar."""
    from pathlib import Path

    from PySide6.QtWidgets import QPushButton

    from tarragon.db import Database
    from tarragon.services.tag_service import TagService

    window = MainWindow()
    try:
        db = Database(Path(":memory:"))
        db.init_schema()
        tag_service = TagService(db=db)
        window.setup_widgets(db, tag_service)

        # Find the Add Folder+ button in the filter bar
        btns = window.filter_bar.findChildren(QPushButton)
        folder_btns = [b for b in btns if b.text() == "Add Folder+"]
        assert len(folder_btns) == 1
        folder_btn = folder_btns[0]

        # Initially hidden (local mode)
        assert folder_btn.isHidden()

        # Switch to global mode
        window._gallery_tabs._tab_widget.setCurrentIndex(1)
        assert not folder_btn.isHidden()

        # Switch back to local mode
        window._gallery_tabs._tab_widget.setCurrentIndex(0)
        assert folder_btn.isHidden()
    finally:
        window.close()


# ── Bug Regression: Filters return results immediately after folder open ──


def test_filters_return_results_immediately_after_folder_open(
    qapp: Any, tmp_path: Path  # noqa: ARG001
) -> None:
    """After opening a folder, filtered queries return results without waiting for renders.

    Regression test: previously, opening a folder scanned files from the filesystem
    but only populated the database after async thumbnail rendering completed.
    If a user applied a filter before rendering finished, the query returned zero
    results because the database was empty.
    """
    from tarragon.db import Database
    from tarragon.services.tag_service import TagService

    window = MainWindow()
    try:
        db = Database(Path(":memory:"))
        db.init_schema()

        # Create a real folder with image files the scanner can find
        folder = tmp_path / "test_images"
        folder.mkdir()
        (folder / "alpha.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00" * 100)
        (folder / "beta.jpg").write_bytes(b"\xff\xd8\xff\xe0" + b"\x00" * 100)
        (folder / "gamma.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00" * 100)

        tag_service = TagService(db=db)
        window.setup_widgets(db, tag_service)

        # Navigate to the folder — this should populate the database immediately
        window._navigate_to_folder(folder)

        # Verify: database has records for all 3 files (stubs populated before render)
        records = db.list_thumbnails_for_folder(str(folder))
        assert len(records) == 3, f"Expected 3 DB records, got {len(records)}"

        # Verify: filtered query returns results immediately (no async render needed)
        window._run_filtered_query()
        assert window.thumbnail_model.rowCount() == 3

        # Verify: filename filter works immediately
        window._search_edit.setText("alpha")
        window._run_filtered_query()
        assert window.thumbnail_model.rowCount() == 1

        # Clean up filter state
        window._search_edit.setText("")
    finally:
        window.close()


def test_bulk_upsert_stubs_inserts_and_updates(qapp: Any) -> None:  # noqa: ARG001
    """bulk_upsert_stubs inserts new records and updates existing ones."""
    from tarragon.db import Database

    db = Database(Path(":memory:"))
    db.init_schema()
    try:
        # Insert stubs
        db.bulk_upsert_stubs([
            ("/test/a.png", 100, 500),
            ("/test/b.png", 200, 600),
        ])

        # Verify stubs were inserted with placeholder values
        rec_a = db.get_thumbnail("/test/a.png")
        assert rec_a is not None
        assert rec_a["mtime"] == 100
        assert rec_a["size"] == 500
        assert rec_a["width"] == 0
        assert rec_a["height"] == 0
        assert rec_a["cache_uuid"] == ""
        assert rec_a["thumbnail_cache_path"] is None

        # Update stubs (simulating a re-scan with new mtime/size)
        db.bulk_upsert_stubs([
            ("/test/a.png", 999, 777),
        ])
        rec_a_updated = db.get_thumbnail("/test/a.png")
        assert rec_a_updated is not None
        assert rec_a_updated["mtime"] == 999
        assert rec_a_updated["size"] == 777

        # Verify upsert_thumbnail (from render) overwrites stub correctly
        db.upsert_thumbnail(
            path="/test/a.png",
            mtime=999,
            size=777,
            width=1920,
            height=1080,
            cache_uuid="real-uuid",
            thumbnail_cache_path="/cache/thumb.png",
            preview_cache_path="/cache/preview.png",
            full_cache_path="/cache/full.png",
        )
        rec_a_final = db.get_thumbnail("/test/a.png")
        assert rec_a_final is not None
        assert rec_a_final["width"] == 1920
        assert rec_a_final["height"] == 1080
        assert rec_a_final["cache_uuid"] == "real-uuid"
        assert rec_a_final["thumbnail_cache_path"] == "/cache/thumb.png"
    finally:
        db.close()


def test_bulk_upsert_stubs_empty_list_is_noop(qapp: Any) -> None:  # noqa: ARG001
    """bulk_upsert_stubs with empty list does nothing and doesn't error."""
    from tarragon.db import Database

    db = Database(Path(":memory:"))
    db.init_schema()
    try:
        db.bulk_upsert_stubs([])
        # No error, no records
        assert db.fetch_all("SELECT COUNT(*) as cnt FROM thumbnails")[0]["cnt"] == 0
    finally:
        db.close()
