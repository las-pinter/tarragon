"""Shared inline styling for color square buttons (single source of truth).

Both the filter-bar swatches (:mod:`tarragon.widgets.filter_bar_color`) and
the preview panel color squares (:mod:`tarragon.widgets.preview_panel`)
render as colored squares whose background is unique per color bucket and
whose border/radius/hover change at runtime.  Static QSS cannot express a
per-bucket background, so this module owns the complete inline stylesheet
for a square button plus its hover behaviour.

:func:`color_square_stylesheet` is the one place that decides the normal
and hover look (all colors and the radius come from theme tokens).
:class:`ColorSquareButton` stores the button state and re-applies that
stylesheet on enter/leave and whenever the active state changes, replacing
the historical ``QPushButton[colorSquare="true"]`` QSS rule and its
``:hover`` variant.
"""

from __future__ import annotations

from PySide6.QtCore import QEvent
from PySide6.QtGui import QEnterEvent
from PySide6.QtWidgets import QPushButton, QWidget

from tarragon.theme.colors import AMBER_ACCENT, TEXT_PRIMARY
from tarragon.theme.constants import BORDER_INACTIVE, RADIUS_S

_ACTIVE_BORDER_WIDTH = 2
_INACTIVE_BORDER_WIDTH = 1


def color_square_stylesheet(
    hex_color: str,
    *,
    active: bool = False,
    hover: bool = False,
    show_inactive_border: bool = True,
) -> str:
    """Return the complete inline QSS for a color square button.

    Args:
        hex_color: The swatch background color (unique per bucket).
        active: Whether the swatch is selected (amber border) or not.
        hover: Whether the pointer is currently over the button.
        show_inactive_border: Whether an unselected, unhovered button shows
            the muted inactive border.  Filter-bar swatches pass ``True``;
            preview squares pass ``False`` to keep their borderless look.

    The border follows the historical visual contract:

    - active: 2px amber border
    - hover (inactive): 1px primary-text border
    - inactive: 1px muted border (or ``none`` when *show_inactive_border``
      is false)
    """
    if active:
        border = f"{_ACTIVE_BORDER_WIDTH}px solid {AMBER_ACCENT.name()}"
    elif hover:
        border = f"{_INACTIVE_BORDER_WIDTH}px solid {TEXT_PRIMARY.name()}"
    elif show_inactive_border:
        border = f"{_INACTIVE_BORDER_WIDTH}px solid {BORDER_INACTIVE}"
    else:
        border = "none"
    return f"QPushButton {{  background-color: {hex_color};  border: {border};  border-radius: {RADIUS_S}px;}}"


def apply_color_square_style(
    button: QPushButton,
    hex_color: str,
    *,
    active: bool = False,
    hover: bool = False,
    show_inactive_border: bool = True,
) -> None:
    """Apply the shared color-square stylesheet to *button* (one-shot).

    Prefer :class:`ColorSquareButton` when the button must react to hover;
    this function only applies a single static state.
    """
    button.setStyleSheet(
        color_square_stylesheet(
            hex_color,
            active=active,
            hover=hover,
            show_inactive_border=show_inactive_border,
        )
    )


class ColorSquareButton(QPushButton):
    """A color-square button that owns its hover state.

    The button stores its current hex color and active state, and re-applies
    :func:`color_square_stylesheet` whenever either changes or the mouse
    enters/leaves the button, so the hover border stays in sync without any
    static ``:hover`` QSS rule.
    """

    def __init__(
        self,
        hex_color: str,
        *,
        active: bool = False,
        show_inactive_border: bool = True,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._swatch_hex = hex_color
        self._swatch_active = active
        self._swatch_show_inactive_border = show_inactive_border
        self._swatch_hover = False
        self._refresh_style()

    def set_color_square_state(
        self,
        hex_color: str | None = None,
        *,
        active: bool | None = None,
    ) -> None:
        """Update the swatch color/active state and re-apply the stylesheet."""
        if hex_color is not None:
            self._swatch_hex = hex_color
        if active is not None:
            self._swatch_active = active
        self._refresh_style()

    def set_active(self, active: bool) -> None:
        """Mark the swatch active/inactive and re-apply the stylesheet."""
        self._swatch_active = active
        self._refresh_style()

    def _refresh_style(self) -> None:
        self.setStyleSheet(
            color_square_stylesheet(
                self._swatch_hex,
                active=self._swatch_active,
                hover=self._swatch_hover,
                show_inactive_border=self._swatch_show_inactive_border,
            )
        )

    def enterEvent(self, event: QEnterEvent) -> None:  # noqa: N802
        """Re-apply the stylesheet with the hover border on mouse enter."""
        self._swatch_hover = True
        self._refresh_style()
        super().enterEvent(event)

    def leaveEvent(self, event: QEvent) -> None:  # noqa: N802
        """Re-apply the normal stylesheet when the mouse leaves."""
        self._swatch_hover = False
        self._refresh_style()
        super().leaveEvent(event)
