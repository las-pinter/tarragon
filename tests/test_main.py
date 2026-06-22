"""Tests for the application entry point."""

import pytest


@pytest.fixture(autouse=True)
def qapp():
    """Provide a shared QApplication instance for all Qt tests."""
    from PySide6.QtWidgets import QApplication

    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


def test_main_window_is_qmainwindow():
    """MainWindow is a subclass of QMainWindow."""
    from PySide6.QtWidgets import QMainWindow
    from tarragon.main import MainWindow

    assert issubclass(MainWindow, QMainWindow)


def test_main_window_has_title(qapp):  # noqa: ARG001
    """MainWindow sets a title on initialization."""
    from tarragon.main import MainWindow

    window = MainWindow()

    assert window.windowTitle() == "Tarragon"


def test_main_function_exists():
    """main() function is callable."""
    from tarragon.main import main

    assert callable(main)
