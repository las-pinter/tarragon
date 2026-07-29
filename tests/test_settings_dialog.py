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
from tarragon.db.database import Database
from tarragon.services.settings_service import SettingsService
from tarragon.widgets.settings_dialog import SettingsDialog

# ── Fixtures ──────────────────────────────────────────────────────────────────


@pytest.fixture()
def service() -> SettingsService:
    """SettingsService backed by in-memory database."""
    db = Database(Path(":memory:"))
    db.init_schema()
    return SettingsService(db)


@pytest.fixture()
def dialog(qapp: Any, service: SettingsService) -> Generator[SettingsDialog, None, None]:
    """SettingsDialog instance backed by the in-memory service."""
    d = SettingsDialog(service)
    yield d
    d.close()


# ── Helpers ───────────────────────────────────────────────────────────────────


def _get_widget(dialog: SettingsDialog, attr: str) -> Any:
    """Return the Qt widget from a setting wrapper attribute."""
    wrapper = getattr(dialog, attr)
    return wrapper.get_widget()


# ── A. Dialog Creation ────────────────────────────────────────────────────────


class TestDialogCreation:
    def test_dialog_creates_without_error(self, dialog: SettingsDialog) -> None:
        """Basic instantiation succeeds and window title is correct."""
        assert dialog is not None
        assert dialog.windowTitle() == "Preferences"

    def test_dialog_has_four_sections(self, dialog: SettingsDialog) -> None:
        """Verify 4 QGroupBox sections exist with correct titles."""
        groups = dialog.findChildren(QGroupBox)
        assert len(groups) == 4
        titles = {g.title() for g in groups}
        assert titles == {"Performance", "Color Tagging", "Cache", "Debug"}

    def test_dialog_loads_current_settings(self, dialog: SettingsDialog) -> None:
        """Verify widgets are populated from SettingsService defaults."""
        psd_workers = _get_widget(dialog, "_max_psd_workers_setting")
        multi_preview = _get_widget(dialog, "_max_multi_preview_setting")
        canvas_threshold = _get_widget(dialog, "_large_canvas_threshold_setting")
        grid_combo = _get_widget(dialog, "_tile_grid_size_setting")
        color_enabled = _get_widget(dialog, "_color_tag_enabled_setting")
        palette_size = _get_widget(dialog, "_color_tag_palette_size_setting")
        min_share = _get_widget(dialog, "_color_tag_min_share_setting")
        neutral_s = _get_widget(dialog, "_color_tag_neutral_s_threshold_setting")
        format_combo = _get_widget(dialog, "_cache_format_setting")
        debug_check = _get_widget(dialog, "_debug_mode_setting")

        assert psd_workers.value() == 3
        assert multi_preview.value() == 9
        assert canvas_threshold.value() == pytest.approx(20.0)
        assert grid_combo.currentText() == "2x2"
        assert color_enabled.isChecked() is True
        assert palette_size.value() == 8
        assert min_share.value() == pytest.approx(0.10)
        assert neutral_s.value() == pytest.approx(0.15)
        assert format_combo.currentIndex() == 0  # PNG
        assert debug_check.isChecked() is False


# ── B. Widget Types ───────────────────────────────────────────────────────────


class TestWidgetTypes:
    def test_performance_section_has_correct_widgets(self, dialog: SettingsDialog) -> None:
        """QSpinBox for ints, QDoubleSpinBox for floats in Performance."""
        assert isinstance(_get_widget(dialog, "_max_psd_workers_setting"), QSpinBox)
        assert isinstance(_get_widget(dialog, "_max_multi_preview_setting"), QSpinBox)
        assert isinstance(_get_widget(dialog, "_large_canvas_threshold_setting"), QDoubleSpinBox)

    def test_tile_grid_size_in_performance_section(self, dialog: SettingsDialog) -> None:
        """tile_grid_size is a QComboBox inside the Performance section."""
        assert isinstance(_get_widget(dialog, "_tile_grid_size_setting"), QComboBox)

    def test_color_tagging_section_has_checkbox_and_spinboxes(self, dialog: SettingsDialog) -> None:
        """Verify widget types in the Color Tagging section."""
        assert isinstance(_get_widget(dialog, "_color_tag_enabled_setting"), QCheckBox)
        assert isinstance(_get_widget(dialog, "_color_tag_palette_size_setting"), QSpinBox)
        assert isinstance(_get_widget(dialog, "_color_tag_min_share_setting"), QDoubleSpinBox)
        assert isinstance(_get_widget(dialog, "_color_tag_neutral_s_threshold_setting"), QDoubleSpinBox)

    def test_cache_section_has_browse_button(self, dialog: SettingsDialog) -> None:
        """cache_dir has a container with QLineEdit + Browse QPushButton."""
        container = _get_widget(dialog, "_cache_dir_setting")
        line_edit = container.findChild(QLineEdit)
        browse_btn = container.findChild(QPushButton)
        assert line_edit is not None
        assert browse_btn is not None
        assert browse_btn.text() == "Browse..."

    def test_cache_format_is_combobox(self, dialog: SettingsDialog) -> None:
        """cache_format is a QComboBox."""
        assert isinstance(_get_widget(dialog, "_cache_format_setting"), QComboBox)

    def test_debug_section_has_checkbox(self, dialog: SettingsDialog) -> None:
        """debug_mode is a QCheckBox."""
        assert isinstance(_get_widget(dialog, "_debug_mode_setting"), QCheckBox)


# ── C. Save/Load Roundtrip ────────────────────────────────────────────────────


class TestSaveLoadRoundtrip:
    def test_save_updates_settings_service(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Change values, click OK, verify SettingsService has new values."""
        psd_workers = _get_widget(dialog, "_max_psd_workers_setting")
        debug_check = _get_widget(dialog, "_debug_mode_setting")

        psd_workers.setValue(5)
        debug_check.setChecked(True)
        dialog._on_accept()

        assert service.max_psd_workers.get() == 5
        assert service.debug_mode.get() is True

    def test_cancel_does_not_save(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Change values, click Cancel, verify SettingsService unchanged."""
        original_workers = service.max_psd_workers.get()
        original_debug = service.debug_mode.get()

        psd_workers = _get_widget(dialog, "_max_psd_workers_setting")
        debug_check = _get_widget(dialog, "_debug_mode_setting")

        psd_workers.setValue(7)
        debug_check.setChecked(True)
        dialog.reject()

        assert service.max_psd_workers.get() == original_workers
        assert service.debug_mode.get() == original_debug

    def test_spinbox_values_saved_correctly(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Set specific numeric values, save, verify via service .get()."""
        psd_workers = _get_widget(dialog, "_max_psd_workers_setting")
        multi_preview = _get_widget(dialog, "_max_multi_preview_setting")
        canvas_threshold = _get_widget(dialog, "_large_canvas_threshold_setting")

        psd_workers.setValue(4)
        multi_preview.setValue(50)
        canvas_threshold.setValue(42.0)
        dialog._on_accept()

        assert service.max_psd_workers.get() == 4
        assert service.max_multi_preview.get() == 50
        assert service.large_canvas_threshold_mp.get() == pytest.approx(42.0)

    def test_color_tagging_values_saved(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Color tagging sub-values persist through save."""
        palette_size = _get_widget(dialog, "_color_tag_palette_size_setting")
        min_share = _get_widget(dialog, "_color_tag_min_share_setting")
        neutral_s = _get_widget(dialog, "_color_tag_neutral_s_threshold_setting")

        palette_size.setValue(16)
        min_share.setValue(0.25)
        neutral_s.setValue(0.30)
        dialog._on_accept()

        assert service.color_tag_palette_size.get() == 16
        assert service.color_tag_min_share.get() == pytest.approx(0.25)
        assert service.color_tag_neutral_s_threshold.get() == pytest.approx(0.30)

    def test_cache_format_saved_correctly(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Selecting JPEG in the format combo persists as 'JPEG'."""
        format_combo = _get_widget(dialog, "_cache_format_setting")

        format_combo.setCurrentIndex(1)  # JPEG
        dialog._on_accept()

        assert service.cache_format.get() == "JPEG"

    def test_cache_dir_default_preserved_on_save(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """When cache_dir text matches platform default, service stores None."""
        # Don't change the cache_dir — it shows the platform default
        dialog._on_accept()

        assert service.cache_dir.get() is None

    def test_cache_dir_custom_path_saved(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """A custom cache directory path is persisted through save."""
        # Access the QLineEdit inside the cache_dir container
        line_edit = dialog._cache_dir_setting._widget
        assert isinstance(line_edit, QLineEdit)

        line_edit.setText("/tmp/custom_cache")
        dialog._on_accept()

        assert service.cache_dir.get() == "/tmp/custom_cache"


# ── D. Color Tagging Enable/Disable ───────────────────────────────────────────


class TestColorTagEnableDisable:
    def test_color_tagging_widgets_disabled_when_unchecked(self, dialog: SettingsDialog) -> None:
        """Uncheck color_tag_enabled → sub-widgets become disabled."""
        checkbox = _get_widget(dialog, "_color_tag_enabled_setting")
        checkbox.setChecked(False)

        palette_widget = _get_widget(dialog, "_color_tag_palette_size_setting")
        min_share_widget = _get_widget(dialog, "_color_tag_min_share_setting")
        neutral_s_widget = _get_widget(dialog, "_color_tag_neutral_s_threshold_setting")

        assert palette_widget.isEnabled() is False
        assert min_share_widget.isEnabled() is False
        assert neutral_s_widget.isEnabled() is False

    def test_color_tagging_widgets_enabled_when_checked(self, dialog: SettingsDialog) -> None:
        """Check color_tag_enabled → sub-widgets become enabled."""
        checkbox = _get_widget(dialog, "_color_tag_enabled_setting")

        # Toggle off first, then back on
        checkbox.setChecked(False)
        checkbox.setChecked(True)

        palette_widget = _get_widget(dialog, "_color_tag_palette_size_setting")
        min_share_widget = _get_widget(dialog, "_color_tag_min_share_setting")
        neutral_s_widget = _get_widget(dialog, "_color_tag_neutral_s_threshold_setting")

        assert palette_widget.isEnabled() is True
        assert min_share_widget.isEnabled() is True
        assert neutral_s_widget.isEnabled() is True

    def test_color_tagging_widgets_enabled_by_default(self, dialog: SettingsDialog) -> None:
        """Default state has color tagging enabled, so sub-widgets are enabled."""
        checkbox = _get_widget(dialog, "_color_tag_enabled_setting")
        assert checkbox.isChecked() is True

        palette_widget = _get_widget(dialog, "_color_tag_palette_size_setting")
        min_share_widget = _get_widget(dialog, "_color_tag_min_share_setting")
        neutral_s_widget = _get_widget(dialog, "_color_tag_neutral_s_threshold_setting")

        assert palette_widget.isEnabled() is True
        assert min_share_widget.isEnabled() is True
        assert neutral_s_widget.isEnabled() is True


# ── E. Validation/Clamping ────────────────────────────────────────────────────


class TestValidationClamping:
    def test_spinbox_ranges_match_service_constraints(self, dialog: SettingsDialog) -> None:
        """QSpinBox/QDoubleSpinBox ranges align with SettingsService limits."""
        psd_workers = _get_widget(dialog, "_max_psd_workers_setting")
        multi_preview = _get_widget(dialog, "_max_multi_preview_setting")
        canvas_threshold = _get_widget(dialog, "_large_canvas_threshold_setting")
        palette_size = _get_widget(dialog, "_color_tag_palette_size_setting")
        min_share = _get_widget(dialog, "_color_tag_min_share_setting")
        neutral_s = _get_widget(dialog, "_color_tag_neutral_s_threshold_setting")

        # max_psd_workers: [1, 8]
        assert psd_workers.minimum() == 1
        assert psd_workers.maximum() == 8

        # max_multi_preview: [1, 100]
        assert multi_preview.minimum() == 1
        assert multi_preview.maximum() == 100

        # large_canvas_threshold_mp: [0.1, 1000.0]
        assert canvas_threshold.minimum() == pytest.approx(0.1)
        assert canvas_threshold.maximum() == pytest.approx(1000.0)

        # palette_size: [2, 32]
        assert palette_size.minimum() == 2
        assert palette_size.maximum() == 32

        # min_share: [0.0, 1.0]
        assert min_share.minimum() == pytest.approx(0.0)
        assert min_share.maximum() == pytest.approx(1.0)

        # neutral_s_threshold: [0.0, 1.0]
        assert neutral_s.minimum() == pytest.approx(0.0)
        assert neutral_s.maximum() == pytest.approx(1.0)

    def test_service_clamps_out_of_range_values(self, service: SettingsService) -> None:
        """SettingsService clamps values even when bypassing the UI."""
        service.max_psd_workers.set(99)
        assert service.max_psd_workers.get() == 8

        service.max_psd_workers.set(0)
        assert service.max_psd_workers.get() == 1

        service.color_tag_palette_size.set(100)
        assert service.color_tag_palette_size.get() == 32


# ── F. Tile Grid Size Combo ───────────────────────────────────────────────────


class TestTileGridSizeCombo:
    def test_tile_grid_size_combobox_has_presets(self, dialog: SettingsDialog) -> None:
        """Verify combo has '1x1', '2x2', '3x3', '4x4'."""
        grid_combo = _get_widget(dialog, "_tile_grid_size_setting")
        items = [grid_combo.itemText(i) for i in range(grid_combo.count())]
        assert items == ["1x1", "2x2", "3x3", "4x4"]

    def test_tile_grid_size_default_selected(self, dialog: SettingsDialog) -> None:
        """Default tile_grid_size '2x2' is selected in the combo."""
        grid_combo = _get_widget(dialog, "_tile_grid_size_setting")
        assert grid_combo.currentText() == "2x2"

    def test_tile_grid_size_save_and_load(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Select different preset, save, verify via SettingsService."""
        grid_combo = _get_widget(dialog, "_tile_grid_size_setting")

        grid_combo.setCurrentIndex(2)  # "3x3"
        dialog._on_accept()

        assert service.tile_grid_size.get() == "3x3"

    def test_tile_grid_size_4x4_save(self, dialog: SettingsDialog, service: SettingsService) -> None:
        """Select '4x4' preset, save, verify."""
        grid_combo = _get_widget(dialog, "_tile_grid_size_setting")

        grid_combo.setCurrentIndex(3)  # "4x4"
        dialog._on_accept()

        assert service.tile_grid_size.get() == "4x4"


# ── G. Cache Format Combo ─────────────────────────────────────────────────────


class TestCacheFormatCombo:
    def test_cache_format_combobox_has_uppercase_items(self, dialog: SettingsDialog) -> None:
        """Verify cache format combo has ['PNG', 'JPEG'] (uppercase)."""
        format_combo = _get_widget(dialog, "_cache_format_setting")
        items = [format_combo.itemText(i) for i in range(format_combo.count())]
        assert items == ["PNG", "JPEG"]

    def test_cache_format_default_is_png(self, dialog: SettingsDialog) -> None:
        """Default cache format is PNG (index 0)."""
        format_combo = _get_widget(dialog, "_cache_format_setting")
        assert format_combo.currentText() == "PNG"
