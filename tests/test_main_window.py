"""Tests for MainWindow — dock panels, menu actions, and default sizing."""

from __future__ import annotations

import pytest
from PySide6.QtWidgets import QDockWidget, QMainWindow
from tarragon.main_window import MainWindow


@pytest.fixture(autouse=True)
def qapp():
    """Provide a shared QApplication instance for all Qt tests."""
    from PySide6.QtWidgets import QApplication

    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


# ── Instantiation Tests ────────────────────────────────────────────────


def test_main_window_is_qmainwindow():
    """MainWindow is a subclass of QMainWindow."""
    assert issubclass(MainWindow, QMainWindow)


def test_main_window_instantiates_with_no_settings(qapp):  # noqa: ARG001
    """MainWindow can be created without a Settings instance (lazy init)."""
    window = MainWindow()
    try:
        assert window is not None
        assert isinstance(window, QMainWindow)
    finally:
        window.close()


def test_main_window_instantiates_with_settings(qapp):  # noqa: ARG001
    """MainWindow accepts a Settings-like object without error."""

    class _FakeSettings:
        def get(self, key: str) -> None:
            return None

        def set(self, key: str, value: object) -> None:
            pass

        def close(self) -> None:
            pass

    window = MainWindow(settings=_FakeSettings())
    try:
        assert window._settings is not None
    finally:
        window.close()


# ── Dock Widget Tests ──────────────────────────────────────────────────


def test_three_docks_exist(qapp):  # noqa: ARG001
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


def test_docks_have_correct_titles(qapp):  # noqa: ARG001
    """Each dock panel has the expected title."""
    window = MainWindow()
    try:
        assert window.sidebar_dock.windowTitle() == "Library"
        assert window.grid_dock.windowTitle() == "Gallery"
        assert window.preview_dock.windowTitle() == "Preview"
    finally:
        window.close()


def test_docks_are_attached_to_window(qapp):  # noqa: ARG001
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


def test_open_folder_menu_exists(qapp):  # noqa: ARG001
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


def test_open_folder_action_in_file_menu(qapp):  # noqa: ARG001
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


def test_window_has_reasonable_default_size(qapp):  # noqa: ARG001
    """MainWindow opens at approximately 1200x800."""
    window = MainWindow()
    try:
        size = window.size()
        assert size.width() == 1200, f"Expected width 1200, got {size.width()}"
        assert size.height() == 800, f"Expected height 800, got {size.height()}"
    finally:
        window.close()


def test_default_size_constants_defined():
    """MainWindow has DEFAULT_WIDTH and DEFAULT_HEIGHT class attributes."""
    assert MainWindow.DEFAULT_WIDTH == 1200
    assert MainWindow.DEFAULT_HEIGHT == 800
