"""Tests for LogPanel debug checkbox behavior"""

from __future__ import annotations

from typing import Any
from unittest.mock import MagicMock

from PySide6.QtWidgets import QCheckBox

from tarragon.widgets.log_panel import LogPanel


class TestDebugCheckboxInit:
    """LogPanel reads the debug setting on initialization."""

    def test_checkbox_checked_when_setting_true(self, qapp: Any) -> None:
        """Checkbox is checked when debug_mode setting is True."""
        mock_settings = MagicMock()
        mock_settings.debug_mode.get.return_value = True
        panel = LogPanel(settings_service=mock_settings)
        try:
            assert panel._debug_checkbox.isChecked() is True
        finally:
            panel.close()

    def test_checkbox_unchecked_when_setting_false(self, qapp: Any) -> None:
        """Checkbox is unchecked when debug_mode setting is False."""
        mock_settings = MagicMock()
        mock_settings.debug_mode.get.return_value = False
        panel = LogPanel(settings_service=mock_settings)
        try:
            assert panel._debug_checkbox.isChecked() is False
        finally:
            panel.close()

    def test_checkbox_unchecked_without_settings_service(self, qapp: Any) -> None:
        """Checkbox defaults to unchecked when no settings service is passed."""
        panel = LogPanel()
        try:
            assert panel._debug_checkbox.isChecked() is False
        finally:
            panel.close()


class TestDebugCheckboxToggle:
    """LogPanel writes the debug setting when the checkbox is toggled."""

    def test_toggle_on_writes_setting(self, qapp: Any) -> None:
        """Toggling the checkbox on persists True to the setting."""
        mock_settings = MagicMock()
        mock_settings.debug_mode.get.return_value = False
        panel = LogPanel(settings_service=mock_settings)
        try:
            panel._debug_checkbox.setChecked(True)
            mock_settings.debug_mode.set.assert_called_once_with(True)
        finally:
            panel.close()

    def test_toggle_off_writes_setting(self, qapp: Any) -> None:
        """Toggling the checkbox off persists False to the setting."""
        mock_settings = MagicMock()
        mock_settings.debug_mode.get.return_value = True
        panel = LogPanel(settings_service=mock_settings)
        try:
            panel._debug_checkbox.setChecked(False)
            mock_settings.debug_mode.set.assert_called_once_with(False)
        finally:
            panel.close()

    def test_toggle_does_not_write_without_settings_service(self, qapp: Any) -> None:
        """Toggling works without a settings service and does not raise."""
        panel = LogPanel()
        try:
            panel._debug_checkbox.setChecked(True)
            assert panel._debug_checkbox.isChecked() is True
        finally:
            panel.close()


class TestDebugCheckboxWidget:
    """LogPanel debug checkbox is a QCheckBox."""

    def test_debug_checkbox_is_qcheckbox(self, qapp: Any) -> None:
        """The debug control is backed by a QCheckBox."""
        panel = LogPanel()
        try:
            assert isinstance(panel._debug_checkbox, QCheckBox)
        finally:
            panel.close()
