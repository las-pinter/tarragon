"""SettingsDialog — preferences dialog for cache, logging, and format settings.

Provides a modal dialog where users can configure the thumbnail cache directory,
toggle debug logging, and select the cache image format.  Changes are persisted
to the ``Settings`` store only when the user clicks OK.
"""

from __future__ import annotations

from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QFileDialog,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from tarragon.app_paths import cache_dir, data_dir
from tarragon.settings import Settings


class SettingsDialog(QDialog):
    """Modal preferences dialog for application settings.

    Exposes three configurable settings:

    - **Cache directory** — user-selectable path (falls back to the default
      ``app_paths.cache_dir()`` when unset).
    - **Debug logging** — boolean toggle for verbose log output.
    - **Cache format** — image format used for cached thumbnails (PNG or JPEG).

    Settings are read from the ``Settings`` store on construction and written
    back only when the user accepts the dialog via the OK button.
    """

    def __init__(self, settings: Settings, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._settings = settings

        self.setWindowTitle("Preferences")
        self.setMinimumWidth(400)

        # ── Main layout ─────────────────────────────────────────────────
        layout = QVBoxLayout(self)
        layout.setContentsMargins(12, 12, 12, 12)
        layout.setSpacing(10)

        # ── Cache directory section ─────────────────────────────────────
        cache_label = QLabel("Cache Directory:")
        layout.addWidget(cache_label)

        cache_row = QHBoxLayout()
        cache_row.setSpacing(6)

        current_cache = self._settings.get("cache_dir")
        display_path = str(current_cache) if current_cache else str(cache_dir())

        self._cache_dir_edit = QLineEdit(display_path)
        self._cache_dir_edit.setReadOnly(True)
        cache_row.addWidget(self._cache_dir_edit)

        self._browse_btn = QPushButton("Browse...")
        self._browse_btn.clicked.connect(self._browse_cache_dir)
        cache_row.addWidget(self._browse_btn)

        layout.addLayout(cache_row)

        # ── Debug logging section ───────────────────────────────────────
        self._debug_checkbox = QCheckBox("Enable debug logging")
        self._debug_checkbox.setChecked(bool(self._settings.get("debug_mode")))
        layout.addWidget(self._debug_checkbox)

        # ── Cache format section ────────────────────────────────────────
        format_row = QHBoxLayout()
        format_row.setSpacing(6)

        format_label = QLabel("Cache Format:")
        format_row.addWidget(format_label)

        self._format_combo = QComboBox()
        self._format_combo.addItems(["PNG", "JPEG"])

        current_format = str(self._settings.get("cache_format")).lower()
        self._format_combo.setCurrentIndex(0 if current_format == "png" else 1)
        format_row.addWidget(self._format_combo)

        format_row.addStretch()
        layout.addLayout(format_row)

        # ── Dialog buttons ──────────────────────────────────────────────
        layout.addStretch()

        button_row = QHBoxLayout()
        button_row.setSpacing(8)
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

    def _on_accept(self) -> None:
        """Persist all settings and accept the dialog."""
        # Compare against the platform default, not the currently active path
        # (which may include a custom override). This prevents wiping a custom
        # cache_dir when the user opens Preferences and clicks OK unchanged.
        platform_default = str(data_dir() / "cache" / "previews")
        cache_text = self._cache_dir_edit.text().strip()

        if cache_text == platform_default:
            self._settings.set("cache_dir", None)
        else:
            self._settings.set("cache_dir", cache_text)

        self._settings.set("debug_mode", self._debug_checkbox.isChecked())

        format_value = "png" if self._format_combo.currentIndex() == 0 else "jpeg"
        self._settings.set("cache_format", format_value)

        self.accept()
