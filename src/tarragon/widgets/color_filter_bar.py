"""ColorFilterBar — horizontal scrollable row of clickable hue swatches.

Represents the 10 color buckets used by the color tagger.  Each swatch is a
coloured square button; clicking it toggles an active state (amber border) and
emits ``color_filter_changed`` with the set of currently active bucket names.
"""

from __future__ import annotations

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QHBoxLayout, QPushButton, QScrollArea, QWidget

# -- Style constants ----------------------------------------------------------

_SWATCH_SIZE = 36
_ACTIVE_BORDER_COLOR = "#F4A261"
_ACTIVE_BORDER_WIDTH = 2
_INACTIVE_BORDER_COLOR = "#555555"
_INACTIVE_BORDER_WIDTH = 1


class ColorFilterBar(QWidget):
    """A horizontal scrollable row of clickable colour-bucket swatches.

    Emits ``color_filter_changed(set)`` whenever a swatch is toggled.  The
    payload is a ``set[str]`` of active bucket names in ``"color:<name>"``
    format (e.g. ``{"color:red", "color:blue"}``).
    """

    color_filter_changed = Signal(set)  # set of "color:<bucket>" strings

    # Representative hue colours for each bucket
    BUCKET_HUES: dict[str, str] = {
        "red": "#E74C3C",
        "orange": "#F39C12",
        "yellow": "#F1C40F",
        "green": "#27AE60",
        "teal": "#1ABC9C",
        "cyan": "#00BCD4",
        "blue": "#3498DB",
        "purple": "#9B59B6",
        "magenta": "#E91E63",
        "neutral": "#7F8C8D",
    }

    def __init__(self, parent: QWidget | None = None) -> None:
        """Create the filter bar with one swatch per colour bucket."""
        super().__init__(parent)
        self._active_colors: set[str] = set()
        self._swatch_buttons: dict[str, QPushButton] = {}

        # -- Layout -----------------------------------------------------------
        outer_layout = QHBoxLayout(self)
        outer_layout.setContentsMargins(0, 0, 0, 0)

        scroll_area = QScrollArea()
        scroll_area.setWidgetResizable(True)
        scroll_area.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        scroll_area.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        outer_layout.addWidget(scroll_area)

        container = QWidget()
        self._layout = QHBoxLayout(container)
        self._layout.setContentsMargins(4, 4, 4, 4)
        self._layout.setSpacing(4)

        for bucket_name, hex_color in self.BUCKET_HUES.items():
            btn = self._create_swatch_button(bucket_name, hex_color)
            self._swatch_buttons[bucket_name] = btn
            self._layout.addWidget(btn)

        self._layout.addStretch()
        scroll_area.setWidget(container)

    # -- Public API -----------------------------------------------------------

    def set_active_colors(self, color_names: set[str]) -> None:
        """Set which colours are active (for programmatic control).

        *color_names* should contain bare bucket names (e.g. ``"red"``) or
        prefixed names (``"color:red"``).  Both forms are accepted.
        """
        bare_names: set[str] = set()
        for name in color_names:
            bare = name.removeprefix("color:") if name.startswith("color:") else name
            if bare in self.BUCKET_HUES:
                bare_names.add(bare)

        self._active_colors = bare_names
        self._refresh_all_swatches()
        self._emit_signal()

    def toggle_color(self, bucket_name: str) -> None:
        """Toggle a specific colour bucket on or off."""
        bare = bucket_name.removeprefix("color:") if bucket_name.startswith("color:") else bucket_name
        if bare not in self.BUCKET_HUES:
            return

        if bare in self._active_colors:
            self._active_colors.discard(bare)
        else:
            self._active_colors.add(bare)

        self._update_swatch_style(bare)
        self._emit_signal()

    def get_active_colors(self) -> set[str]:
        """Return the set of currently active colour bucket names.

        Names are returned in ``"color:<bucket>"`` format.
        """
        return {f"color:{name}" for name in self._active_colors}

    # -- Internal helpers -----------------------------------------------------

    def _create_swatch_button(self, bucket_name: str, hex_color: str) -> QPushButton:
        """Build a single swatch button for *bucket_name*."""
        btn = QPushButton()
        btn.setFixedSize(_SWATCH_SIZE, _SWATCH_SIZE)
        btn.setToolTip(f"color:{bucket_name}")
        btn.setProperty("bucket_name", bucket_name)
        btn.setCursor(Qt.CursorShape.PointingHandCursor)
        btn.clicked.connect(lambda _checked=False, name=bucket_name: self.toggle_color(name))
        self._apply_swatch_style(btn, hex_color, active=False)
        return btn

    @staticmethod
    def _apply_swatch_style(btn: QPushButton, hex_color: str, *, active: bool) -> None:
        """Apply the appropriate stylesheet to a swatch button."""
        border_color = _ACTIVE_BORDER_COLOR if active else _INACTIVE_BORDER_COLOR
        border_width = _ACTIVE_BORDER_WIDTH if active else _INACTIVE_BORDER_WIDTH
        btn.setStyleSheet(
            f"QPushButton {{"
            f"  background-color: {hex_color};"
            f"  border: {border_width}px solid {border_color};"
            f"  border-radius: 4px;"
            f"}}"
        )

    def _update_swatch_style(self, bucket_name: str) -> None:
        """Refresh the visual style of a single swatch to match its state."""
        btn = self._swatch_buttons[bucket_name]
        hex_color = self.BUCKET_HUES[bucket_name]
        active = bucket_name in self._active_colors
        self._apply_swatch_style(btn, hex_color, active=active)

    def _refresh_all_swatches(self) -> None:
        """Refresh visual styles for every swatch."""
        for bucket_name in self.BUCKET_HUES:
            self._update_swatch_style(bucket_name)

    def _emit_signal(self) -> None:
        """Emit ``color_filter_changed`` with the current active set."""
        self.color_filter_changed.emit(self.get_active_colors())
