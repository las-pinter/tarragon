"""Tests for the shared color-square helper (single source of truth)."""

from __future__ import annotations

from PySide6.QtCore import QEvent, QPointF
from PySide6.QtGui import QEnterEvent

from tarragon.theme.colors import AMBER_ACCENT, TEXT_PRIMARY
from tarragon.theme.constants import BORDER_INACTIVE, RADIUS_S
from tarragon.widgets.color_square import ColorSquareButton, color_square_stylesheet

# Sample bucket hex (red)
_RED = "#E74C3C"


def _enter_event() -> QEnterEvent:
    """Build a synthetic mouse-enter event for direct event dispatch."""
    return QEnterEvent(QPointF(0, 0), QPointF(0, 0), QPointF(0, 0))


class TestColorSquareStylesheet:
    """color_square_stylesheet produces the historical visual contract."""

    def test_inactive_swatch_has_background_radius_and_muted_border(self) -> None:
        """An idle swatch keeps its bucket color, theme radius, and muted border."""
        qss = color_square_stylesheet(_RED)
        assert _RED in qss
        assert f"border-radius: {RADIUS_S}px" in qss
        assert f"border: 1px solid {BORDER_INACTIVE}" in qss
        assert AMBER_ACCENT.name() not in qss

    def test_active_swatch_has_amber_border(self) -> None:
        """An active swatch shows the amber border."""
        qss = color_square_stylesheet(_RED, active=True)
        assert f"border: 2px solid {AMBER_ACCENT.name()}" in qss

    def test_hover_swatch_uses_primary_text_border(self) -> None:
        """A hovered (inactive) swatch shows the primary-text border."""
        qss = color_square_stylesheet(_RED, hover=True, show_inactive_border=False)
        assert f"border: 1px solid {TEXT_PRIMARY.name()}" in qss

    def test_preview_mode_is_borderless_when_idle(self) -> None:
        """Preview squares (show_inactive_border=False) have no idle border."""
        qss = color_square_stylesheet(_RED, show_inactive_border=False)
        assert "border: none" in qss


class TestColorSquareButton:
    """ColorSquareButton owns hover and state-driven styling."""

    def test_initial_style_applied(self) -> None:
        """The constructor applies the shared stylesheet immediately."""
        btn = ColorSquareButton(_RED, active=False, show_inactive_border=False)
        try:
            assert "border: none" in btn.styleSheet()
        finally:
            btn.close()

    def test_enter_event_applies_hover_border(self) -> None:
        """Entering the button switches to the hover border."""
        btn = ColorSquareButton(_RED, show_inactive_border=False)
        try:
            btn.enterEvent(_enter_event())
            assert f"border: 1px solid {TEXT_PRIMARY.name()}" in btn.styleSheet()
        finally:
            btn.close()

    def test_leave_event_restores_normal_style(self) -> None:
        """Leaving the button restores the normal border state."""
        btn = ColorSquareButton(_RED, show_inactive_border=False)
        try:
            btn.enterEvent(_enter_event())
            btn.leaveEvent(QEvent(QEvent.Type.Leave))
            assert "border: none" in btn.styleSheet()
        finally:
            btn.close()

    def test_set_color_square_state_updates_border(self) -> None:
        """Updating the active state re-applies the stylesheet."""
        btn = ColorSquareButton(_RED)
        try:
            btn.set_color_square_state(active=True)
            assert f"border: 2px solid {AMBER_ACCENT.name()}" in btn.styleSheet()
        finally:
            btn.close()
