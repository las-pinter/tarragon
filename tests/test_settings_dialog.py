"""Tests for SettingsDialog — widget creation, save/load roundtrip, and enable/disable logic."""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from typing import Any

import pytest
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDoubleSpinBox,
    QGroupBox,
    QLineEdit,
    QPushButton,
    QSpinBox,
)
from tarragon.db import Database
from tarragon.services.settings_service import SettingsService
from tarragon.settings import Settings
from tarragon.widgets.settings_dialog import SettingsDialog

# ── Fixtures ──────────────────────────────────────────────────────────────────


@pytest.fixture()
def service() -> SettingsService:
    """SettingsService backed by in-memory database."""
    db = Database(Path(":memory:"))
    db.init_schema()
    settings = Settings(db)
    settings.init_defaults()
    return SettingsService(settings)


@pytest.fixture()
def dialog(qapp: Any, service: SettingsService) -> Generator[SettingsDialog, None, None]:
    """SettingsDialog instance backed by the in-memory service."""
    d = SettingsDialog(service)
    yield d
    d.close()


# ── A. Dialog Creation ────────────────────────────────────────────────────────


class TestDialogCreation:
    def test_dialog_creates_without_error(self, dialog: SettingsDialog) -> None:
        """Basic instantiation succeeds and window title is correct."""
        assert dialog is not None
        assert dialog.windowTitle() == "Preferences"

    def test_dialog_has_five_sections(self, dialog: SettingsDialog) -> None:
        """Verify 5 QGroupBox sections exist with correct titles."""
        groups = dialog.findChildren(QGroupBox)
        assert len(groups) == 5
        titles = {g.title() for g in groups}
        assert titles == {"Performance", "Grid & Layout", "Color Tagging", "Cache", "Debug"}

    def test_dialog_loads_current_settings(self, dialog: SettingsDialog) -> None:
        """Verify widgets are populated from SettingsService defaults."""
        assert dialog._psd_workers_spin.value() == 3
        assert dialog._multi_preview_spin.value() == 9
        assert dialog._canvas_threshold_spin.value() == pytest.approx(20.0)
        assert dialog._grid_combo.currentText() == "2x2"
        assert dialog._color_enabled_check.isChecked() is True
        assert dialog._palette_size_spin.value() == 8
        assert dialog._min_share_spin.value() == pytest.approx(0.10)
        assert dialog._neutral_s_spin.value() == pytest.approx(0.15)
        assert dialog._format_combo.currentIndex() == 0  # PNG
        assert dialog._debug_checkbox.isChecked() is False


# ── B. Widget Types ───────────────────────────────────────────────────────────


class TestWidgetTypes:
    def test_performance_section_has_correct_widgets(self, dialog: SettingsDialog) -> None:
        """QSpinBox for ints, QDoubleSpinBox for floats in Performance."""
        assert isinstance(dialog._psd_workers_spin, QSpinBox)
        assert isinstance(dialog._multi_preview_spin, QSpinBox)
        assert isinstance(dialog._canvas_threshold_spin, QDoubleSpinBox)

    def test_grid_section_has_combobox(self, dialog: SettingsDialog) -> None:
        """tile_grid_size is a QComboBox."""
        assert isinstance(dialog._grid_combo, QComboBox)

    def test_color_tagging_section_has_checkbox_and_spinboxes(self, dialog: SettingsDialog) -> None:
        """Verify widget types in the Color Tagging section."""
        assert isinstance(dialog._color_enabled_check, QCheckBox)
        assert isinstance(dialog._palette_size_spin, QSpinBox)
        assert isinstance(dialog._min_share_spin, QDoubleSpinBox)
        assert isinstance(dialog._neutral_s_spin, QDoubleSpinBox)

    def test_cache_section_has_browse_button(self, dialog: SettingsDialog) -> None:
        """cache_dir has QLineEdit + Browse QPushButton."""
        assert isinstance(dialog._cache_dir_edit, QLineEdit)
        assert isinstance(dialog._browse_btn, QPushButton)
        assert dialog._browse_btn.text() == "Browse..."

    def test_debug_section_has_checkbox(self, dialog: SettingsDialog) -> None:
        """debug_mode is a QCheckBox."""
        assert isinstance(dialog._debug_checkbox, QCheckBox)


# ── C. Save/Load Roundtrip ────────────────────────────────────────────────────


class TestSaveLoadRoundtrip:
    def test_save_updates_settings_service(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Change values, click OK, verify SettingsService has new values."""
        dialog._psd_workers_spin.setValue(5)
        dialog._debug_checkbox.setChecked(True)
        dialog._on_accept()

        assert service.get_max_psd_workers() == 5
        assert service.get_debug_mode() is True

    def test_cancel_does_not_save(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Change values, click Cancel, verify SettingsService unchanged."""
        original_workers = service.get_max_psd_workers()
        original_debug = service.get_debug_mode()

        dialog._psd_workers_spin.setValue(7)
        dialog._debug_checkbox.setChecked(True)
        dialog.reject()

        assert service.get_max_psd_workers() == original_workers
        assert service.get_debug_mode() == original_debug

    def test_spinbox_values_saved_correctly(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Set specific numeric values, save, verify via getters."""
        dialog._psd_workers_spin.setValue(4)
        dialog._multi_preview_spin.setValue(50)
        dialog._canvas_threshold_spin.setValue(42.0)
        dialog._on_accept()

        assert service.get_max_psd_workers() == 4
        assert service.get_max_multi_preview() == 50
        assert service.get_large_canvas_threshold_mp() == pytest.approx(42.0)

    def test_color_tagging_values_saved(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Color tagging sub-values persist through save."""
        dialog._palette_size_spin.setValue(16)
        dialog._min_share_spin.setValue(0.25)
        dialog._neutral_s_spin.setValue(0.30)
        dialog._on_accept()

        assert service.get_color_tag_palette_size() == 16
        assert service.get_color_tag_min_share() == pytest.approx(0.25)
        assert service.get_color_tag_neutral_s_threshold() == pytest.approx(0.30)

    def test_cache_format_saved_correctly(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Selecting JPEG in the format combo persists as 'jpeg'."""
        dialog._format_combo.setCurrentIndex(1)  # JPEG
        dialog._on_accept()

        assert service.get_cache_format() == "jpeg"

    def test_cache_dir_default_preserved_on_save(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """When cache_dir text matches platform default, service stores None."""
        # Don't change the cache_dir_edit — it shows the platform default
        dialog._on_accept()

        assert service.get_cache_dir() is None

    def test_cache_dir_custom_path_saved(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """A custom cache directory path is persisted through save."""
        dialog._cache_dir_edit.setText("/tmp/custom_cache")
        dialog._on_accept()

        assert service.get_cache_dir() == "/tmp/custom_cache"


# ── D. Color Tagging Enable/Disable ───────────────────────────────────────────


class TestColorTagEnableDisable:
    def test_color_tagging_widgets_disabled_when_unchecked(self, dialog: SettingsDialog) -> None:
        """Uncheck color_tag_enabled → sub-widgets become disabled."""
        dialog._color_enabled_check.setChecked(False)

        assert dialog._palette_size_spin.isEnabled() is False
        assert dialog._min_share_spin.isEnabled() is False
        assert dialog._neutral_s_spin.isEnabled() is False

    def test_color_tagging_widgets_enabled_when_checked(self, dialog: SettingsDialog) -> None:
        """Check color_tag_enabled → sub-widgets become enabled."""
        # Toggle off first, then back on
        dialog._color_enabled_check.setChecked(False)
        dialog._color_enabled_check.setChecked(True)

        assert dialog._palette_size_spin.isEnabled() is True
        assert dialog._min_share_spin.isEnabled() is True
        assert dialog._neutral_s_spin.isEnabled() is True

    def test_color_tagging_widgets_enabled_by_default(self, dialog: SettingsDialog) -> None:
        """Default state has color tagging enabled, so sub-widgets are enabled."""
        assert dialog._color_enabled_check.isChecked() is True
        assert dialog._palette_size_spin.isEnabled() is True
        assert dialog._min_share_spin.isEnabled() is True
        assert dialog._neutral_s_spin.isEnabled() is True


# ── E. Validation/Clamping ────────────────────────────────────────────────────


class TestValidationClamping:
    def test_spinbox_ranges_match_service_constraints(self, dialog: SettingsDialog) -> None:
        """QSpinBox/QDoubleSpinBox ranges align with SettingsService limits."""
        # max_psd_workers: [1, 8]
        assert dialog._psd_workers_spin.minimum() == 1
        assert dialog._psd_workers_spin.maximum() == 8

        # max_multi_preview: [1, 100]
        assert dialog._multi_preview_spin.minimum() == 1
        assert dialog._multi_preview_spin.maximum() == 100

        # large_canvas_threshold_mp: [0.1, 1000.0]
        assert dialog._canvas_threshold_spin.minimum() == pytest.approx(0.1)
        assert dialog._canvas_threshold_spin.maximum() == pytest.approx(1000.0)

        # palette_size: [2, 32]
        assert dialog._palette_size_spin.minimum() == 2
        assert dialog._palette_size_spin.maximum() == 32

        # min_share: [0.0, 1.0]
        assert dialog._min_share_spin.minimum() == pytest.approx(0.0)
        assert dialog._min_share_spin.maximum() == pytest.approx(1.0)

        # neutral_s_threshold: [0.0, 1.0]
        assert dialog._neutral_s_spin.minimum() == pytest.approx(0.0)
        assert dialog._neutral_s_spin.maximum() == pytest.approx(1.0)

    def test_service_clamps_out_of_range_values(self, service: SettingsService) -> None:
        """SettingsService clamps values even when bypassing the UI."""
        service.set_max_psd_workers(99)
        assert service.get_max_psd_workers() == 8

        service.set_max_psd_workers(0)
        assert service.get_max_psd_workers() == 1

        service.set_color_tag_palette_size(100)
        assert service.get_color_tag_palette_size() == 32


# ── F. Tile Grid Size Combo ───────────────────────────────────────────────────


class TestTileGridSizeCombo:
    def test_tile_grid_size_combobox_has_presets(self, dialog: SettingsDialog) -> None:
        """Verify combo has '1x1', '2x2', '3x3', '4x4'."""
        items = [dialog._grid_combo.itemText(i) for i in range(dialog._grid_combo.count())]
        assert items == ["1x1", "2x2", "3x3", "4x4"]

    def test_tile_grid_size_default_selected(self, dialog: SettingsDialog) -> None:
        """Default tile_grid_size '2x2' is selected in the combo."""
        assert dialog._grid_combo.currentText() == "2x2"

    def test_tile_grid_size_save_and_load(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Select different preset, save, verify via SettingsService."""
        dialog._grid_combo.setCurrentIndex(2)  # "3x3"
        dialog._on_accept()

        assert service.get_tile_grid_size() == "3x3"

    def test_tile_grid_size_4x4_save(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Select '4x4' preset, save, verify."""
        dialog._grid_combo.setCurrentIndex(3)  # "4x4"
        dialog._on_accept()

        assert service.get_tile_grid_size() == "4x4"
