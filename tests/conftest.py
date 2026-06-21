"""Pytest configuration — ensures headless operation for Qt tests."""

import os

# Must be set BEFORE any Qt imports or QApplication creation
os.environ["QT_QPA_PLATFORM"] = "offscreen"
