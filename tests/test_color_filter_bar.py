"""Tests for ColorFilterBar widget (Task 5.3).

Covers:
    - Widget creation with all 10 swatches
    - Toggle behaviour (click and programmatic)
    - Signal emission with correct payload
    - Visual state (background colours, tooltips, active border)
"""

from __future__ import annotations

from collections.abc import Generator

import pytest
from PySide6.QtWidgets import QApplication, QPushButton
from tarragon.widgets.color_filter_bar import ColorFilterBar

# ── Fixtures ────────────────────────────────────────────────────────────────


@pytest.fixture()
def bar() -> Generator[ColorFilterBar, None, None]:
    """Provide a ColorFilterBar that is closed after the test."""
    w = ColorFilterBar()
    yield w
    w.close()


# ── Creation & structure ────────────────────────────────────────────────────


class TestColorFilterBarCreation:
    def test_color_filter_bar_creation(self, bar: ColorFilterBar) -> None:
        """Widget is created without error and contains swatch buttons."""
        assert isinstance(bar, ColorFilterBar)
        swatches = bar.findChildren(QPushButton)
        assert len(swatches) == len(ColorFilterBar.BUCKET_HUES)

    def test_all_buckets_present(self, bar: ColorFilterBar) -> None:
        """All 10 colour buckets have corresponding swatch buttons."""
        bucket_names = {btn.property("bucket_name") for btn in bar.findChildren(QPushButton)}
        assert bucket_names == set(ColorFilterBar.BUCKET_HUES.keys())


# ── Toggle behaviour ────────────────────────────────────────────────────────


class TestToggleColor:
    def test_toggle_color_on(self, bar: ColorFilterBar) -> None:
        """Toggling a colour adds it to the active set."""
        # Arrange & Act
        bar.toggle_color("red")

        # Assert
        assert "color:red" in bar.get_active_colors()

    def test_toggle_color_off(self, bar: ColorFilterBar) -> None:
        """Toggling an already-active colour removes it."""
        # Arrange
        bar.toggle_color("blue")
        assert "color:blue" in bar.get_active_colors()

        # Act
        bar.toggle_color("blue")

        # Assert
        assert "color:blue" not in bar.get_active_colors()

    def test_toggle_via_button_click(self, bar: ColorFilterBar) -> None:
        """Clicking a swatch button toggles the colour on."""
        # Arrange
        btn = bar._swatch_buttons["green"]

        # Act
        btn.click()

        # Assert
        assert "color:green" in bar.get_active_colors()

    def test_toggle_unknown_bucket_ignored(self, bar: ColorFilterBar) -> None:
        """Toggling an unknown bucket name does nothing."""
        # Arrange
        initial = bar.get_active_colors()

        # Act
        bar.toggle_color("ultraviolet")

        # Assert
        assert bar.get_active_colors() == initial


# ── Programmatic control ────────────────────────────────────────────────────


class TestSetActiveColors:
    def test_set_active_colors_bare_names(self, bar: ColorFilterBar) -> None:
        """set_active_colors accepts bare bucket names."""
        # Arrange & Act
        bar.set_active_colors({"red", "blue"})

        # Assert
        assert bar.get_active_colors() == {"color:red", "color:blue"}

    def test_set_active_colors_prefixed_names(self, bar: ColorFilterBar) -> None:
        """set_active_colors accepts 'color:' prefixed names."""
        # Arrange & Act
        bar.set_active_colors({"color:green", "color:teal"})

        # Assert
        assert bar.get_active_colors() == {"color:green", "color:teal"}

    def test_set_active_colors_empty(self, bar: ColorFilterBar) -> None:
        """Setting an empty set clears all active colours."""
        # Arrange
        bar.set_active_colors({"red", "blue"})

        # Act
        bar.set_active_colors(set())

        # Assert
        assert bar.get_active_colors() == set()

    def test_set_active_colors_ignores_unknown(self, bar: ColorFilterBar) -> None:
        """Unknown bucket names in the set are silently ignored."""
        # Arrange & Act
        bar.set_active_colors({"red", "ultraviolet"})

        # Assert
        assert bar.get_active_colors() == {"color:red"}


# ── get_active_colors ───────────────────────────────────────────────────────


class TestGetActiveColors:
    def test_get_active_colors_initially_empty(self, bar: ColorFilterBar) -> None:
        """No colours are active on a fresh widget."""
        assert bar.get_active_colors() == set()

    def test_get_active_colors_returns_prefixed(self, bar: ColorFilterBar) -> None:
        """Returned names use the 'color:' prefix."""
        bar.toggle_color("cyan")
        result = bar.get_active_colors()
        assert result == {"color:cyan"}
        for name in result:
            assert name.startswith("color:")


# ── Signal emission ─────────────────────────────────────────────────────────


class TestColorFilterChangedSignal:
    def test_color_filter_changed_signal_on_toggle(self, bar: ColorFilterBar) -> None:
        """Toggling a swatch emits color_filter_changed with the active set."""
        # Arrange
        captured: list[set[str]] = []
        bar.color_filter_changed.connect(lambda s: captured.append(s))

        # Act
        bar.toggle_color("red")

        # Assert
        assert len(captured) == 1
        assert captured[0] == {"color:red"}

    def test_signal_emitted_on_set_active(self, bar: ColorFilterBar) -> None:
        """set_active_colors also emits the signal."""
        # Arrange
        captured: list[set[str]] = []
        bar.color_filter_changed.connect(lambda s: captured.append(s))

        # Act
        bar.set_active_colors({"blue", "green"})

        # Assert
        assert len(captured) == 1
        assert captured[0] == {"color:blue", "color:green"}

    def test_signal_emitted_on_click(self, bar: ColorFilterBar) -> None:
        """Clicking a swatch button emits the signal."""
        # Arrange
        captured: list[set[str]] = []
        bar.color_filter_changed.connect(lambda s: captured.append(s))
        btn = bar._swatch_buttons["yellow"]

        # Act
        btn.click()

        # Assert
        assert len(captured) == 1
        assert captured[0] == {"color:yellow"}


# ── Multiple colours ────────────────────────────────────────────────────────


class TestMultipleColorsActive:
    def test_multiple_colors_active(self, bar: ColorFilterBar) -> None:
        """Multiple colours can be active simultaneously."""
        # Arrange & Act
        bar.toggle_color("red")
        bar.toggle_color("blue")
        bar.toggle_color("green")

        # Assert
        assert bar.get_active_colors() == {"color:red", "color:blue", "color:green"}

    def test_deactivating_one_keeps_others(self, bar: ColorFilterBar) -> None:
        """Deactivating one colour leaves the rest active."""
        # Arrange
        bar.set_active_colors({"red", "blue", "green"})

        # Act
        bar.toggle_color("blue")

        # Assert
        assert bar.get_active_colors() == {"color:red", "color:green"}


# ── Visual state ────────────────────────────────────────────────────────────


class TestSwatchesHaveCorrectColors:
    def test_swatches_have_correct_colors(self, bar: ColorFilterBar) -> None:
        """Each swatch button has the correct background colour in its stylesheet."""
        for bucket_name, hex_color in ColorFilterBar.BUCKET_HUES.items():
            btn = bar._swatch_buttons[bucket_name]
            stylesheet = btn.styleSheet()
            assert hex_color in stylesheet, f"Swatch '{bucket_name}' stylesheet missing colour {hex_color}"

    def test_tooltips_show_bucket_names(self, bar: ColorFilterBar) -> None:
        """Each swatch tooltip displays 'color:<bucket_name>'."""
        for bucket_name in ColorFilterBar.BUCKET_HUES:
            btn = bar._swatch_buttons[bucket_name]
            assert btn.toolTip() == f"color:{bucket_name}", f"Swatch '{bucket_name}' tooltip mismatch"

    def test_active_swatch_has_amber_border(self, bar: ColorFilterBar) -> None:
        """An active swatch shows the amber border colour in its stylesheet."""
        # Arrange & Act
        bar.toggle_color("red")

        # Assert
        btn = bar._swatch_buttons["red"]
        assert "#fac775" in btn.styleSheet()

    def test_inactive_swatch_lacks_amber_border(self, bar: ColorFilterBar) -> None:
        """An inactive swatch does not show the amber border colour."""
        # Assert (no toggles — all inactive)
        btn = bar._swatch_buttons["red"]
        assert "#fac775" not in btn.styleSheet()
