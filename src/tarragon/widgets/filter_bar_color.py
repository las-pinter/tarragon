"""Horizontal scrollable row of clickable hue swatches."""

from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QPushButton, QScrollArea, QWidget

from tarragon.theme.color_buckets import BUCKET_HEX_COLORS, ColorBucket
from tarragon.theme.colors import AMBER_ACCENT
from tarragon.theme.constants import SPACING_S, SPACING_XS
from tarragon.widgets.filter_bar_filter import FilterBarFilter

_SWATCH_SIZE = 36
_ACTIVE_BORDER_COLOR: str = AMBER_ACCENT.name()
_ACTIVE_BORDER_WIDTH = 2
_INACTIVE_BORDER_COLOR = "#555555"
_INACTIVE_BORDER_WIDTH = 1


class FilterBarColor(FilterBarFilter):
    """A horizontal scrollable row of clickable color-bucket swatches.

    Emits whenever a swatch is toggled.  The payload is a ``set[str]``
    of active bucket names.
    """

    BUCKET_HUES: dict[ColorBucket, str] = dict(BUCKET_HEX_COLORS)

    def __init__(self, parent: QWidget | None = None) -> None:
        """Create the filter bar with one swatch per color bucket."""
        super().__init__(parent)
        self._active_colors: set[ColorBucket] = set()
        self._swatch_buttons: dict[str, QPushButton] = {}

        outer_layout = QHBoxLayout(self)
        outer_layout.setContentsMargins(SPACING_S, SPACING_XS, SPACING_S, SPACING_XS)

        scroll_area = QScrollArea()
        scroll_area.setWidgetResizable(True)
        scroll_area.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        scroll_area.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        outer_layout.addWidget(scroll_area)

        container = QWidget()
        self._layout = QHBoxLayout(container)
        self._layout.setContentsMargins(SPACING_XS, SPACING_XS, SPACING_XS, SPACING_XS)
        self._layout.setSpacing(SPACING_XS)

        for color_bucket, hex_color in self.BUCKET_HUES.items():
            btn = self._create_swatch_button(color_bucket, hex_color)
            self._swatch_buttons[color_bucket] = btn
            self._layout.addWidget(btn)

        self._layout.addStretch()
        scroll_area.setWidget(container)

    def _create_swatch_button(self, color_bucket: ColorBucket, hex_color: str) -> QPushButton:
        """Build a single swatch button for *color_bucket*."""
        btn = QPushButton()
        btn.setFixedSize(_SWATCH_SIZE, _SWATCH_SIZE)
        btn.setToolTip(color_bucket)
        btn.setProperty("color_bucket", color_bucket)
        btn.setCursor(Qt.CursorShape.PointingHandCursor)
        btn.clicked.connect(lambda _checked=False, name=color_bucket: self.toggle_color(name))
        self._apply_swatch_style(btn, hex_color, active=False)
        return btn

    @staticmethod
    def _apply_swatch_style(btn: QPushButton, hex_color: str, *, active: bool) -> None:
        """Apply the appropriate stylesheet to a swatch button.

        This MUST remain inline because:
        - ``background-color`` is unique per bucket (dynamic hex_color).
        - ``border`` changes between active/inactive states at runtime.
        Neither property can be expressed as a static QSS rule.
        """
        border_color = _ACTIVE_BORDER_COLOR if active else _INACTIVE_BORDER_COLOR
        border_width = _ACTIVE_BORDER_WIDTH if active else _INACTIVE_BORDER_WIDTH
        btn.setStyleSheet(
            f"QPushButton {{"
            f"  background-color: {hex_color};"
            f"  border: {border_width}px solid {border_color};"
            f"  border-radius: 4px;"
            f"}}"
        )

    def _update_swatch_style(self, color_bucket: ColorBucket) -> None:
        """Refresh the visual style of a single swatch to match its state."""
        btn = self._swatch_buttons[color_bucket]
        hex_color = self.BUCKET_HUES[color_bucket]
        active = color_bucket in self._active_colors
        self._apply_swatch_style(btn, hex_color, active=active)

    def _refresh_all_swatches(self) -> None:
        """Refresh visual styles for every swatch."""
        for color_bucket in self.BUCKET_HUES:
            self._update_swatch_style(color_bucket)

    def set_active_colors(self, color_buckets: set[ColorBucket]) -> None:
        """Set which colors are active (for programmatic control).

        *color_names* should contain bare bucket names (e.g. ``"red"``) or
        prefixed names (``"color:red"``).  Both forms are accepted.
        """
        self._active_colors = color_buckets
        self._refresh_all_swatches()
        self._emit_signal(self.get_active_colors())

    def toggle_color(self, color_bucket: ColorBucket) -> None:
        """Toggle a specific color bucket on or off."""
        if color_bucket in self._active_colors:
            self._active_colors.discard(color_bucket)
        else:
            self._active_colors.add(color_bucket)

        self._update_swatch_style(color_bucket)
        self._emit_signal(self.get_active_colors())

    def get_active_colors(self) -> set[ColorBucket]:
        """Return the set of currently active color bucket names.

        Names are returned in ``"<bucket>"`` format.
        """
        return {bucket for bucket in self._active_colors}
