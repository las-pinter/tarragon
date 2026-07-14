"""SettingsDialog — preferences dialog for all application settings.

Provides a modal dialog where users can configure performance, grid layout,
color tagging, cache, and debug settings. Changes are persisted through
SettingsService (which handles validation/clamping) only when the user
clicks OK.
"""

from __future__ import annotations

from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QDoubleSpinBox,
    QFileDialog,
    QFormLayout,
    QGroupBox,
    QHBoxLayout,
    QLineEdit,
    QPushButton,
    QSpinBox,
    QVBoxLayout,
    QWidget,
)

from tarragon.app_paths import cache_dir, data_dir
from tarragon.services.settings_service import SettingsService
from tarragon.theme.constants import MD, SM, XS


class SettingsDialog(QDialog):
    """Modal preferences dialog for application settings.

    Exposes all 11 configurable settings organized into sections:

    - **Performance** — PSD workers, multi-preview cap, large canvas threshold.
    - **Grid & Layout** — tile grid size preset.
    - **Color Tagging** — enable toggle, palette size, min share, neutral S threshold.
    - **Cache** — directory path and image format.
    - **Debug** — debug logging toggle.

    Settings are read from the SettingsService on construction and written
    back only when the user accepts the dialog via the OK button.
    """

    def __init__(self, settings_service: SettingsService, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._settings_service = settings_service

        self.setWindowTitle("Preferences")
        self.setMinimumWidth(450)

        # ── Main layout ─────────────────────────────────────────────────
        layout = QVBoxLayout(self)
        layout.setContentsMargins(MD, MD, MD, MD)
        layout.setSpacing(SM)

        # ── Performance Section ─────────────────────────────────────────
        perf_group = QGroupBox("Performance")
        perf_group.setToolTip("Settings that control rendering speed and memory usage.")
        perf_layout = QFormLayout()
        perf_layout.setSpacing(SM)

        self._psd_workers_spin = QSpinBox()
        self._psd_workers_spin.setRange(1, 8)
        self._psd_workers_spin.setValue(self._settings_service.get_max_psd_workers())
        self._psd_workers_spin.setToolTip(
            "Number of parallel processes for rendering PSD/PSB files. "
            "Higher values speed up batch processing but use more RAM. "
            "Changes require restart."
        )
        perf_layout.addRow("Max PSD Workers:", self._psd_workers_spin)

        self._multi_preview_spin = QSpinBox()
        self._multi_preview_spin.setRange(1, 100)
        self._multi_preview_spin.setValue(self._settings_service.get_max_multi_preview())
        self._multi_preview_spin.setToolTip(
            "Maximum number of images to show when multiple files are selected. "
            "Prevents memory issues with large selections."
        )
        perf_layout.addRow("Max Multi-Preview:", self._multi_preview_spin)

        self._canvas_threshold_spin = QDoubleSpinBox()
        self._canvas_threshold_spin.setRange(0.1, 1000.0)
        self._canvas_threshold_spin.setDecimals(1)
        self._canvas_threshold_spin.setValue(self._settings_service.get_large_canvas_threshold_mp())
        self._canvas_threshold_spin.setToolTip(
            "Canvas size threshold (in megapixels) for switching to tiled PSD rendering. "
            "Larger values use more memory but may be faster for big files."
        )
        perf_layout.addRow("Large Canvas Threshold (MP):", self._canvas_threshold_spin)

        perf_group.setLayout(perf_layout)
        layout.addWidget(perf_group)

        # ── Grid & Layout Section ───────────────────────────────────────
        grid_group = QGroupBox("Grid & Layout")
        grid_group.setToolTip("Settings for tiled rendering grid configuration.")
        grid_layout = QFormLayout()
        grid_layout.setSpacing(SM)

        self._grid_combo = QComboBox()
        self._grid_combo.addItems(["1x1", "2x2", "3x3", "4x4"])
        current_grid = self._settings_service.get_tile_grid_size()
        idx = self._grid_combo.findText(current_grid)
        if idx >= 0:
            self._grid_combo.setCurrentIndex(idx)
        self._grid_combo.setToolTip(
            "Grid size for tiled PSD rendering (e.g., '2x2' splits into 4 tiles). "
            "Higher values reduce memory usage but may be slower."
        )
        grid_layout.addRow("Tile Grid Size:", self._grid_combo)

        grid_group.setLayout(grid_layout)
        layout.addWidget(grid_group)

        # ── Color Tagging Section ───────────────────────────────────────
        color_group = QGroupBox("Color Tagging")
        color_group.setToolTip("Automatic color tag extraction from dominant image colors.")
        color_layout = QFormLayout()
        color_layout.setSpacing(SM)

        self._color_enabled_check = QCheckBox("Enable color tagging")
        self._color_enabled_check.setChecked(self._settings_service.get_color_tag_enabled())
        self._color_enabled_check.setToolTip(
            "Enable automatic color tag extraction from images. "
            "Tags like 'color:red', 'color:blue' are added based on dominant colors."
        )
        color_layout.addRow(self._color_enabled_check)

        self._palette_size_spin = QSpinBox()
        self._palette_size_spin.setRange(2, 32)
        self._palette_size_spin.setValue(self._settings_service.get_color_tag_palette_size())
        self._palette_size_spin.setToolTip(
            "Number of dominant colors to extract per image. "
            "Higher values detect more color variations but may be slower."
        )
        color_layout.addRow("Palette Size:", self._palette_size_spin)

        self._min_share_spin = QDoubleSpinBox()
        self._min_share_spin.setRange(0.0, 1.0)
        self._min_share_spin.setDecimals(2)
        self._min_share_spin.setSingleStep(0.05)
        self._min_share_spin.setValue(self._settings_service.get_color_tag_min_share())
        self._min_share_spin.setToolTip(
            "Minimum percentage of image area a color must cover to be tagged "
            "(e.g., 0.10 = 10%). Lower values detect more colors."
        )
        color_layout.addRow("Min Color Share:", self._min_share_spin)

        self._neutral_s_spin = QDoubleSpinBox()
        self._neutral_s_spin.setRange(0.0, 1.0)
        self._neutral_s_spin.setDecimals(2)
        self._neutral_s_spin.setSingleStep(0.05)
        self._neutral_s_spin.setValue(self._settings_service.get_color_tag_neutral_s_threshold())
        self._neutral_s_spin.setToolTip(
            "Saturation threshold for neutral colors. "
            "Colors with saturation below this are tagged as 'neutral' instead of a hue."
        )
        color_layout.addRow("Neutral Saturation Threshold:", self._neutral_s_spin)

        color_group.setLayout(color_layout)
        layout.addWidget(color_group)

        # ── Cache Section ───────────────────────────────────────────────
        cache_group = QGroupBox("Cache")
        cache_group.setToolTip("Thumbnail cache storage location and image format.")
        cache_layout = QVBoxLayout()
        cache_layout.setSpacing(SM)

        # Cache directory row
        cache_dir_row = QHBoxLayout()
        cache_dir_row.setSpacing(XS)

        current_cache = self._settings_service.get_cache_dir()
        display_path = str(current_cache) if current_cache else str(cache_dir())

        self._cache_dir_edit = QLineEdit(display_path)
        self._cache_dir_edit.setReadOnly(True)
        self._cache_dir_edit.setToolTip(
            "Custom directory for thumbnail cache. Leave empty to use default location. Changes require cache rebuild."
        )
        cache_dir_row.addWidget(self._cache_dir_edit)

        self._browse_btn = QPushButton("Browse...")
        self._browse_btn.clicked.connect(self._browse_cache_dir)
        self._browse_btn.setToolTip(
            "Custom directory for thumbnail cache. Leave empty to use default location. Changes require cache rebuild."
        )
        cache_dir_row.addWidget(self._browse_btn)

        cache_layout.addLayout(cache_dir_row)

        # Cache format row
        format_row = QHBoxLayout()
        format_row.setSpacing(XS)

        self._format_combo = QComboBox()
        self._format_combo.addItems(["PNG", "JPEG"])
        current_format = self._settings_service.get_cache_format()
        self._format_combo.setCurrentIndex(0 if current_format == "png" else 1)
        self._format_combo.setToolTip(
            "Cache file format: PNG (lossless, larger files) or JPEG (lossy, smaller files). "
            "Changes require cache rebuild."
        )
        format_row.addWidget(self._format_combo)
        format_row.addStretch()

        cache_layout.addLayout(format_row)

        cache_group.setLayout(cache_layout)
        layout.addWidget(cache_group)

        # ── Debug Section ───────────────────────────────────────────────
        debug_group = QGroupBox("Debug")
        debug_group.setToolTip("Developer debugging options.")
        debug_layout = QVBoxLayout()
        debug_layout.setSpacing(SM)

        self._debug_checkbox = QCheckBox("Enable debug logging")
        self._debug_checkbox.setChecked(self._settings_service.get_debug_mode())
        self._debug_checkbox.setToolTip(
            "Enable verbose debug logging. Useful for troubleshooting but may slow down the application."
        )
        debug_layout.addWidget(self._debug_checkbox)

        debug_group.setLayout(debug_layout)
        layout.addWidget(debug_group)

        # ── Wire color_tag_enabled dependency ───────────────────────────
        self._color_enabled_check.toggled.connect(self._on_color_tag_toggled)
        self._on_color_tag_toggled(self._color_enabled_check.isChecked())

        # ── Dialog buttons ──────────────────────────────────────────────
        layout.addStretch()

        button_row = QHBoxLayout()
        button_row.setSpacing(SM)
        button_row.addStretch()

        self._ok_btn = QPushButton("OK")
        self._ok_btn.clicked.connect(self._on_accept)
        button_row.addWidget(self._ok_btn)

        self._cancel_btn = QPushButton("Cancel")
        self._cancel_btn.clicked.connect(self.reject)
        button_row.addWidget(self._cancel_btn)

        layout.addLayout(button_row)

    # ── Slots ─────────────────────────────────────────────────────────────

    def _browse_cache_dir(self) -> None:
        """Open a directory chooser and update the cache path display."""
        current_path = self._cache_dir_edit.text()
        chosen = QFileDialog.getExistingDirectory(
            self,
            "Select Cache Directory",
            current_path,
        )
        if chosen:
            self._cache_dir_edit.setText(chosen)

    def _on_color_tag_toggled(self, enabled: bool) -> None:  # noqa: FBT001
        """Enable/disable color tagging sub-widgets based on the checkbox."""
        self._palette_size_spin.setEnabled(enabled)
        self._min_share_spin.setEnabled(enabled)
        self._neutral_s_spin.setEnabled(enabled)

    def _on_accept(self) -> None:
        """Persist all settings via SettingsService and accept the dialog."""
        # Performance
        self._settings_service.set_max_psd_workers(self._psd_workers_spin.value())
        self._settings_service.set_max_multi_preview(self._multi_preview_spin.value())
        self._settings_service.set_large_canvas_threshold_mp(self._canvas_threshold_spin.value())

        # Grid & Layout
        self._settings_service.set_tile_grid_size(self._grid_combo.currentText())

        # Color Tagging
        self._settings_service.set_color_tag_enabled(self._color_enabled_check.isChecked())
        self._settings_service.set_color_tag_palette_size(self._palette_size_spin.value())
        self._settings_service.set_color_tag_min_share(self._min_share_spin.value())
        self._settings_service.set_color_tag_neutral_s_threshold(self._neutral_s_spin.value())

        # Cache — compare against platform default to avoid wiping custom path
        platform_default = str(data_dir() / "cache")
        cache_text = self._cache_dir_edit.text().strip()

        if cache_text == platform_default:
            self._settings_service.set_cache_dir(None)
        else:
            self._settings_service.set_cache_dir(cache_text)

        format_value = "png" if self._format_combo.currentIndex() == 0 else "jpeg"
        self._settings_service.set_cache_format(format_value)

        # Debug
        self._settings_service.set_debug_mode(self._debug_checkbox.isChecked())

        self.accept()
