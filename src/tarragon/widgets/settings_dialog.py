"""Preferences dialog for all application settings."""

from __future__ import annotations

import logging
from abc import abstractmethod
from collections.abc import Callable
from typing import Any, cast

from PySide6.QtGui import QShowEvent
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QDoubleSpinBox,
    QFileDialog,
    QFormLayout,
    QGroupBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QSpinBox,
    QVBoxLayout,
    QWidget,
)

from tarragon.app_paths import cache_dir, data_dir
from tarragon.services.settings_service import Setting, SettingsService
from tarragon.theme.constants import SPACING_M, SPACING_S

logger = logging.getLogger(__name__)

# -----------------------------------------------------------------------------
# Widgets for Settings
# -----------------------------------------------------------------------------


class _Setting:
    """A setting object which handles the display and update of a setting"""

    def __init__(self, name: str, tooltip: str, setting: Setting) -> None:
        self._name = name
        self._tooltip = tooltip
        self._setting = setting
        self._widget: QWidget

    def _get_setting(self) -> Any:
        """Returning the setting from the Database"""
        value = self._setting.get()
        logger.debug("Received setting from DB - name: %s, value: %s", self._name, value)
        return value

    def get_name(self) -> str:
        """Return the name of the setting"""
        return self._name

    def get_label(self) -> QLabel:
        """Return the label for the stting"""
        label = QLabel(self._name)
        label.setToolTip(self._tooltip)
        return label

    def get_widget(self) -> QWidget:
        """Return the widget object for the setting"""
        return self._widget

    def set_enabled(self, enabled: bool) -> None:
        """Enabling and disabling the widget for editing"""
        self._widget.setEnabled(enabled)

    @abstractmethod
    def store_setting(self) -> None:
        pass


class _SettingsFormLayout(QFormLayout):
    """Subclass for our custom functions"""

    def add_setting(self, setting: _Setting) -> None:
        label = setting.get_label()
        widget = setting.get_widget()
        return self.addRow(label, widget)


class _SettingWithMinMax(_Setting):
    """A setting object with Minimum and Maximum values"""

    def __init__(self, name: str, tooltip: str, setting: Setting) -> None:
        super().__init__(name, tooltip, setting)
        self._min = self._setting.get_min()
        if self._min is None:
            raise ValueError("Setting has no minimum defined")
        self._max = self._setting.get_max()
        if self._max is None:
            raise ValueError("Setting has no maximum defined")


class _Button:
    def __init__(self, text: str, tooltip: str) -> None:
        self._text = text
        self._tooltip = tooltip
        self._button = QPushButton(self._text)
        self._button.setToolTip(self._tooltip)

    def connect_clicked(self, callback: Callable[[], None]) -> None:
        """Connecting a callback for the buttons clicked function"""
        self._button.clicked.connect(callback)

    def get_widget(self) -> QPushButton:
        return self._button


class _LineEditWithButtonSetting(_Setting):
    """Line edit setting with a button"""

    def __init__(self, name: str, tooltip: str, setting: Setting) -> None:
        super().__init__(name, tooltip, setting)
        self._button = _Button("Browse...", self._tooltip)
        value = self._get_setting()
        self._widget = QLineEdit(value)
        self._widget.setCursorPosition(0)
        self._widget.setReadOnly(False)
        self._widget.setToolTip(self._tooltip)

    def get_widget(self) -> QWidget:
        container = QWidget()
        layout = QHBoxLayout(container)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.addWidget(self._widget)
        layout.addWidget(self._button.get_widget())
        return container

    def store_setting(self) -> None:
        assert isinstance(self._widget, QLineEdit)
        self._setting.set(self._widget.text().strip())


class _ComboBoxSetting(_Setting):
    """Combo box setting"""

    def __init__(self, name: str, tooltip: str, items: list[Any], setting: Setting) -> None:
        super().__init__(name, tooltip, setting)
        self._items = items
        value = self._get_setting()
        index = next((i for i, n in enumerate(self._items) if n == value), 0)
        self._widget = QComboBox()
        self._widget.setToolTip(self._tooltip)
        self._widget.addItems(self._items)
        self._widget.setCurrentIndex(index)
        logger.debug("Combo Box Setting init: name: %s, items: %s, value: %s, index: %s", name, items, value, index)

    def store_setting(self) -> None:
        assert isinstance(self._widget, QComboBox)
        index = self._widget.currentIndex()
        value = self._items[index] if index in range(len(self._items)) else self._items[0]
        self._setting.set(value)


class _CheckBoxSetting(_Setting):
    """Check box setting"""

    def __init__(self, name: str, tooltip: str, setting: Setting) -> None:
        super().__init__(name, tooltip, setting)
        value = self._get_setting()
        self._widget = QCheckBox()
        self._widget.setChecked(value)
        self._widget.setToolTip(self._tooltip)

    def is_checked(self) -> bool:
        assert isinstance(self._widget, QCheckBox)
        return self._widget.isChecked()

    def connect_toggled(self, callback: Callable[[bool], None]) -> None:
        """Connecting a callback for the toggling event of the check box"""
        assert isinstance(self._widget, QCheckBox)
        self._widget.toggled.connect(lambda checked=self._widget.isChecked(): callback(checked))

    def store_setting(self) -> None:
        assert isinstance(self._widget, QCheckBox)
        self._setting.set(self._widget.isChecked())


class _SpinBoxSetting(_SettingWithMinMax):
    """Spin box setting"""

    def __init__(self, name: str, tooltip: str, setting: Setting) -> None:
        super().__init__(name, tooltip, setting)
        value = self._get_setting()
        self._min = self._setting.get_min()
        self._widget = QSpinBox()
        self._widget.setRange(cast(int, self._min), cast(int, self._max))
        self._widget.setValue(value)
        self._widget.setToolTip(self._tooltip)

    def store_setting(self) -> None:
        assert isinstance(self._widget, QSpinBox)
        self._setting.set(self._widget.value())


class _DoubleSpinBoxSetting(_SettingWithMinMax):
    """Spin box setting with double values"""

    def __init__(self, name: str, tooltip: str, decimals: int, step_size: float, setting: Setting) -> None:
        super().__init__(name, tooltip, setting)
        value = self._get_setting()
        self._widget = QDoubleSpinBox()
        self._widget.setRange(cast(float, self._min), cast(float, self._max))
        self._widget.setDecimals(decimals)
        self._widget.setSingleStep(step_size)
        self._widget.setValue(value)
        self._widget.setToolTip(self._tooltip)

    def store_setting(self) -> None:
        assert isinstance(self._widget, QDoubleSpinBox)
        self._setting.set(self._widget.value())


# -----------------------------------------------------------------------------
# Settings
# -----------------------------------------------------------------------------


class _SettingCacheDir(_LineEditWithButtonSetting):
    def __init__(self, settings_service: SettingsService, parent: QDialog) -> None:
        super().__init__(
            "Cache Directory",
            "Custom directory for thumbnail cache. Leave empty to use default location. Changes require cache rebuild.",
            settings_service.cache_dir,
        )
        self._parent = parent
        self._button.connect_clicked(self._browse_cache_dir)

    def _browse_cache_dir(self) -> None:
        """Open a directory chooser and update the cache path display."""
        assert isinstance(self._widget, QLineEdit)
        logger.debug("Called")
        current_path = self._widget.text()
        chosen = QFileDialog.getExistingDirectory(
            self._parent,
            "Select Cache Directory",
            current_path,
        )
        if chosen:
            self._widget.setText(chosen)
            logger.debug("Cache dir path updated in the form - value: %s", chosen)

    def _get_setting(self) -> Any:
        current_cache = super()._get_setting()
        return str(current_cache) if current_cache else str(cache_dir())

    def store_setting(self) -> None:
        assert isinstance(self._widget, QLineEdit)
        platform_default = str(data_dir() / "cache")
        cache_text = self._widget.text().strip()
        if cache_text == platform_default:
            self._setting.set(None)
        else:
            self._setting.set(cache_text)


class _SettingCacheFormat(_ComboBoxSetting):
    def __init__(self, settings_service: SettingsService) -> None:
        super().__init__(
            "Cache Format",
            "Cache file format: PNG (lossless, larger files) or JPEG (lossy, smaller files)."
            "Changes require cache rebuild.",
            settings_service.cache_format.get_valid_formats(),
            settings_service.cache_format,
        )


class _SettingColorTagEnabled(_CheckBoxSetting):
    def __init__(self, settings_service: SettingsService) -> None:
        super().__init__(
            "Enable Color Tagging",
            "Enable automatic color tag extraction from images. "
            "Tags like 'color:red', 'color:blue' are added based on dominant colors.",
            settings_service.color_tag_enabled,
        )


class _SettingClearFullResOnExit(_CheckBoxSetting):
    def __init__(self, settings_service: SettingsService) -> None:
        super().__init__(
            "Clear Full-Res Cache on Exit",
            "Delete full-resolution cache files when the application exits. "
            "Full-res images are re-rendered on demand when a folder is opened.",
            settings_service.clear_full_res_on_exit,
        )


class _SettingColorTagPaletteSize(_SpinBoxSetting):
    def __init__(self, settings_service: SettingsService) -> None:
        super().__init__(
            "Color Tag Palette Size",
            "Number of dominant colors to extract per image. "
            "Higher values detect more color variations but may be slower.",
            settings_service.color_tag_palette_size,
        )


class _SettingColorTagMinShare(_DoubleSpinBoxSetting):
    def __init__(self, settings_service: SettingsService) -> None:
        super().__init__(
            "Color Tag Minimum Share",
            "Minimum percentage of image area a color must cover to be tagged "
            "(e.g., 0.10 = 10%). Lower values detect more colors.",
            2,
            0.05,
            settings_service.color_tag_min_share,
        )


class _SettingColorTagNeutralSThreshold(_DoubleSpinBoxSetting):
    def __init__(self, settings_service: SettingsService) -> None:
        super().__init__(
            "Neutral Saturation Threshold",
            "Saturation threshold for neutral colors. "
            "Colors with saturation below this are tagged as 'neutral' instead of a hue.",
            2,
            0.05,
            settings_service.color_tag_neutral_s_threshold,
        )


class _SettingDebugMode(_CheckBoxSetting):
    def __init__(self, settings_service: SettingsService) -> None:
        super().__init__(
            "Enable Debug Logging",
            "Enable verbose debug logging. Useful for troubleshooting but may slow down the application.",
            settings_service.debug_mode,
        )


class _SettingLargeCanvasThresholdMp(_DoubleSpinBoxSetting):
    def __init__(self, settings_service: SettingsService) -> None:
        super().__init__(
            "Large Canvas Threshold (MP)",
            "Canvas size threshold (in megapixels) for switching to tiled PSD rendering. "
            "Larger values use more memory but may be faster for big files.",
            1,
            1,
            settings_service.large_canvas_threshold_mp,
        )


class _SettingMaxMultiPreview(_SpinBoxSetting):
    def __init__(self, settings_service: SettingsService) -> None:
        super().__init__(
            "Max Multi-Preview",
            "Maximum number of images to show when multiple files are selected. "
            "Prevents memory issues with large selections.",
            settings_service.max_multi_preview,
        )


class _SettingMaxPsdWorkers(_SpinBoxSetting):
    def __init__(self, settings_service: SettingsService) -> None:
        super().__init__(
            "Max PSD Workers",
            "Number of parallel processes for rendering PSD/PSB files."
            "Higher values speed up batch processing but use more RAM.",
            settings_service.max_psd_workers,
        )


class _SettingTileGridSize(_ComboBoxSetting):
    def __init__(self, settings_service: SettingsService) -> None:
        super().__init__(
            "Tile Grid Size",
            "Grid size for tiled PSD rendering (e.g., '2x2' splits into 4 tiles). "
            "Higher values reduce memory usage but may be slower.",
            settings_service.tile_grid_size.get_valid_formats(),
            settings_service.tile_grid_size,
        )


# -----------------------------------------------------------------------------
# Dialog Implementation
# -----------------------------------------------------------------------------


class SettingsDialog(QDialog):
    """Modal preferences dialog for application settings.

    Settings are read from the SettingsService on construction and written
    back only when the user accepts the dialog via the OK button.
    """

    def __init__(self, settings_service: SettingsService, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._settings_service = settings_service
        self._settings: list[_Setting] = []

        self.setWindowTitle("Preferences")
        self.setMinimumWidth(450)

        # ---------------------------------------------------------------------
        # Main layout
        # ---------------------------------------------------------------------
        layout = QVBoxLayout(self)
        layout.setContentsMargins(SPACING_M, SPACING_M, SPACING_M, SPACING_M)
        layout.setSpacing(SPACING_S)

        # ---------------------------------------------------------------------
        # Performance Section
        # ---------------------------------------------------------------------
        perf_group = QGroupBox("Performance")
        perf_group.setToolTip("Settings that control rendering speed and memory usage.")
        perf_layout = _SettingsFormLayout()
        perf_layout.setSpacing(SPACING_S)

        self._max_psd_workers_setting = _SettingMaxPsdWorkers(settings_service)
        self._settings.append(self._max_psd_workers_setting)
        perf_layout.add_setting(self._max_psd_workers_setting)

        self._max_multi_preview_setting = _SettingMaxMultiPreview(settings_service)
        self._settings.append(self._max_multi_preview_setting)
        perf_layout.add_setting(self._max_multi_preview_setting)

        self._large_canvas_threshold_setting = _SettingLargeCanvasThresholdMp(settings_service)
        self._settings.append(self._large_canvas_threshold_setting)
        perf_layout.add_setting(self._large_canvas_threshold_setting)

        self._tile_grid_size_setting = _SettingTileGridSize(settings_service)
        self._settings.append(self._tile_grid_size_setting)
        perf_layout.add_setting(self._tile_grid_size_setting)

        perf_group.setLayout(perf_layout)
        layout.addWidget(perf_group)

        # ---------------------------------------------------------------------
        # Color Tagging Section
        # ---------------------------------------------------------------------
        color_group = QGroupBox("Color Tagging")
        color_group.setToolTip("Automatic color tag extraction from dominant image colors.")
        color_layout = _SettingsFormLayout()
        color_layout.setSpacing(SPACING_S)

        self._color_tag_enabled_setting = _SettingColorTagEnabled(settings_service)
        self._settings.append(self._color_tag_enabled_setting)
        color_layout.add_setting(self._color_tag_enabled_setting)

        self._color_tag_palette_size_setting = _SettingColorTagPaletteSize(settings_service)
        self._settings.append(self._color_tag_palette_size_setting)
        color_layout.add_setting(self._color_tag_palette_size_setting)

        self._color_tag_min_share_setting = _SettingColorTagMinShare(settings_service)
        self._settings.append(self._color_tag_min_share_setting)
        color_layout.add_setting(self._color_tag_min_share_setting)

        self._color_tag_neutral_s_threshold_setting = _SettingColorTagNeutralSThreshold(settings_service)
        self._settings.append(self._color_tag_neutral_s_threshold_setting)
        color_layout.add_setting(self._color_tag_neutral_s_threshold_setting)

        color_group.setLayout(color_layout)
        layout.addWidget(color_group)

        # ---------------------------------------------------------------------
        # Cache Section
        # ---------------------------------------------------------------------
        cache_group = QGroupBox("Cache")
        cache_group.setToolTip("Thumbnail cache storage location and image format.")
        cache_layout = _SettingsFormLayout()
        cache_layout.setSpacing(SPACING_S)

        self._cache_dir_setting = _SettingCacheDir(settings_service, self)
        self._settings.append(self._cache_dir_setting)
        cache_layout.add_setting(self._cache_dir_setting)

        self._cache_format_setting = _SettingCacheFormat(settings_service)
        self._settings.append(self._cache_format_setting)
        cache_layout.add_setting(self._cache_format_setting)

        self._clear_full_res_on_exit_setting = _SettingClearFullResOnExit(settings_service)
        self._settings.append(self._clear_full_res_on_exit_setting)
        cache_layout.add_setting(self._clear_full_res_on_exit_setting)

        cache_group.setLayout(cache_layout)
        layout.addWidget(cache_group)

        # ---------------------------------------------------------------------
        # Debug Section
        # ---------------------------------------------------------------------
        debug_group = QGroupBox("Debug")
        debug_group.setToolTip("Developer debugging options.")
        debug_layout = _SettingsFormLayout()
        debug_layout.setSpacing(SPACING_S)

        self._debug_mode_setting = _SettingDebugMode(settings_service)
        self._settings.append(self._debug_mode_setting)
        debug_layout.add_setting(self._debug_mode_setting)

        debug_group.setLayout(debug_layout)
        layout.addWidget(debug_group)

        # ---------------------------------------------------------------------
        # Dialog Buttons
        # ---------------------------------------------------------------------
        layout.addStretch()

        button_row = QHBoxLayout()
        button_row.setSpacing(SPACING_S)
        button_row.addStretch()

        self._ok_btn = QPushButton("OK")
        self._ok_btn.clicked.connect(self._on_accept)
        button_row.addWidget(self._ok_btn)

        self._cancel_btn = QPushButton("Cancel")
        self._cancel_btn.clicked.connect(self.reject)
        button_row.addWidget(self._cancel_btn)

        layout.addLayout(button_row)

        # Connect callbacks
        self._color_tag_enabled_setting.connect_toggled(self._on_color_tag_toggled)
        self._on_color_tag_toggled(self._color_tag_enabled_setting.is_checked())

    def _on_color_tag_toggled(self, enabled: bool) -> None:
        """Enable/disable color tagging sub-widgets based on the checkbox."""
        self._color_tag_palette_size_setting.set_enabled(enabled)
        self._color_tag_min_share_setting.set_enabled(enabled)
        self._color_tag_neutral_s_threshold_setting.set_enabled(enabled)

    def _on_accept(self) -> None:
        """Persist all settings via SettingsService and accept the dialog."""
        for setting in self._settings:
            setting.store_setting()

        self.accept()

    def showEvent(self, event: QShowEvent) -> None:  # noqa: N802
        super().showEvent(event)
        self.setFocus()
