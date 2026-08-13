"""Tests for FilterBarColor"""

from __future__ import annotations

from collections.abc import Generator

import pytest
from PySide6.QtWidgets import QPushButton
from tarragon.widgets.filter_bar_color import FilterBarColor


@pytest.fixture
def bar() -> Generator[FilterBarColor, None, None]:
    """Provide a FilterBarColor that is closed after the test."""
    w = FilterBarColor()
    yield w
    w.close()


class TestFilterBarColorCreation:
    """FilterBarColor construction and basic swatch structure."""

    def test_color_filter_bar_creation(self, bar: FilterBarColor) -> None:
        """Widget is created without error and contains swatch buttons."""
        assert isinstance(bar, FilterBarColor)
        swatches = bar.findChildren(QPushButton)
        assert len(swatches) == len(FilterBarColor.BUCKET_HUES)

    def test_all_buckets_present(self, bar: FilterBarColor) -> None:
        """All 10 colour buckets have corresponding swatch buttons."""
        bucket_names = {btn.property("bucket_name") for btn in bar.findChildren(QPushButton)}
        assert bucket_names == set(FilterBarColor.BUCKET_HUES.keys())


class TestToggleColor:
    """Toggling colours updates the active colour set."""

    def test_toggle_color_off(self, bar: FilterBarColor) -> None:
        """Toggling an already-active colour removes it."""
        bar.toggle_color("blue")
        assert "color:blue" in bar.get_active_colors()

        bar.toggle_color("blue")

        assert "color:blue" not in bar.get_active_colors()

    def test_toggle_via_button_click(self, bar: FilterBarColor) -> None:
        """Clicking a swatch button toggles the colour on."""
        btn = bar._swatch_buttons["green"]

        btn.click()

        assert "color:green" in bar.get_active_colors()

    def test_toggle_unknown_bucket_ignored(self, bar: FilterBarColor) -> None:
        """Toggling an unknown bucket name does nothing."""
        initial = bar.get_active_colors()

        bar.toggle_color("ultraviolet")

        assert bar.get_active_colors() == initial


class TestSetActiveColors:
    """set_active_colors updates the active colour set."""

    def test_set_active_colors_bare_names(self, bar: FilterBarColor) -> None:
        """set_active_colors accepts bare bucket names."""
        bar.set_active_colors({"red", "blue"})

        assert bar.get_active_colors() == {"color:red", "color:blue"}

    def test_set_active_colors_prefixed_names(self, bar: FilterBarColor) -> None:
        """set_active_colors accepts 'color:' prefixed names."""
        bar.set_active_colors({"color:green", "color:teal"})

        assert bar.get_active_colors() == {"color:green", "color:teal"}

    def test_set_active_colors_empty(self, bar: FilterBarColor) -> None:
        """Setting an empty set clears all active colours."""
        bar.set_active_colors({"red", "blue"})

        bar.set_active_colors(set())

        assert bar.get_active_colors() == set()

    def test_set_active_colors_ignores_unknown(self, bar: FilterBarColor) -> None:
        """Unknown bucket names in the set are silently ignored."""
        bar.set_active_colors({"red", "ultraviolet"})

        assert bar.get_active_colors() == {"color:red"}


class TestGetActiveColors:
    """get_active_colors returns active colours with the 'color:' prefix."""

    def test_get_active_colors_initially_empty(self, bar: FilterBarColor) -> None:
        """No colours are active on a fresh widget."""
        assert bar.get_active_colors() == set()

    def test_get_active_colors_returns_prefixed(self, bar: FilterBarColor) -> None:
        """Returned names use the 'color:' prefix."""
        bar.toggle_color("cyan")
        result = bar.get_active_colors()
        assert result == {"color:cyan"}
        for name in result:
            assert name.startswith("color:")


class TestColorFilterChangedSignal:
    """The color_filter_changed signal is emitted with the active set."""

    def test_color_filter_changed_signal_on_toggle(self, bar: FilterBarColor) -> None:
        """Toggling a swatch emits color_filter_changed with the active set."""
        captured: list[set[str]] = []
        bar.filter_changed.connect(lambda s: captured.append(s))

        bar.toggle_color("red")

        assert len(captured) == 1
        assert captured[0] == {"color:red"}

    def test_signal_emitted_on_set_active(self, bar: FilterBarColor) -> None:
        """set_active_colors also emits the signal."""
        captured: list[set[str]] = []
        bar.filter_changed.connect(lambda s: captured.append(s))

        bar.set_active_colors({"blue", "green"})

        assert len(captured) == 1
        assert captured[0] == {"color:blue", "color:green"}

    def test_signal_emitted_on_click(self, bar: FilterBarColor) -> None:
        """Clicking a swatch button emits the signal."""
        captured: list[set[str]] = []
        bar.filter_changed.connect(lambda s: captured.append(s))
        btn = bar._swatch_buttons["yellow"]

        btn.click()

        assert len(captured) == 1
        assert captured[0] == {"color:yellow"}


class TestMultipleColorsActive:
    """Multiple colours can be active simultaneously."""

    def test_multiple_colors_active(self, bar: FilterBarColor) -> None:
        """Multiple colours can be active simultaneously."""
        bar.toggle_color("red")
        bar.toggle_color("blue")
        bar.toggle_color("green")

        assert bar.get_active_colors() == {"color:red", "color:blue", "color:green"}

    def test_deactivating_one_keeps_others(self, bar: FilterBarColor) -> None:
        """Deactivating one colour leaves the rest active."""
        bar.set_active_colors({"red", "blue", "green"})

        bar.toggle_color("blue")

        assert bar.get_active_colors() == {"color:red", "color:green"}


class TestSwatchesHaveCorrectColors:
    """Swatch buttons expose the correct visual state."""

    def test_swatches_have_correct_colors(self, bar: FilterBarColor) -> None:
        """Each swatch button has the correct background colour in its stylesheet."""
        for bucket_name, hex_color in FilterBarColor.BUCKET_HUES.items():
            btn = bar._swatch_buttons[bucket_name]
            stylesheet = btn.styleSheet()
            assert hex_color in stylesheet, f"Swatch '{bucket_name}' stylesheet missing colour {hex_color}"

    def test_tooltips_show_bucket_names(self, bar: FilterBarColor) -> None:
        """Each swatch tooltip displays 'color:<bucket_name>'."""
        for bucket_name in FilterBarColor.BUCKET_HUES:
            btn = bar._swatch_buttons[bucket_name]
            assert btn.toolTip() == f"color:{bucket_name}", f"Swatch '{bucket_name}' tooltip mismatch"

    def test_active_swatch_has_amber_border(self, bar: FilterBarColor) -> None:
        """An active swatch shows the amber border colour in its stylesheet."""
        bar.toggle_color("red")

        btn = bar._swatch_buttons["red"]
        assert "#fac775" in btn.styleSheet()

    def test_inactive_swatch_lacks_amber_border(self, bar: FilterBarColor) -> None:
        """An inactive swatch does not show the amber border colour."""
        btn = bar._swatch_buttons["red"]
        assert "#fac775" not in btn.styleSheet()
