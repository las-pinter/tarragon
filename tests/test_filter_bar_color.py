"""Tests for FilterBarColor"""

from __future__ import annotations

from collections.abc import Generator

import pytest
from PySide6.QtWidgets import QPushButton

from tarragon.theme.color_buckets import ColorBucket
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
        bucket_names = {btn.property("color_bucket") for btn in bar.findChildren(QPushButton)}
        assert bucket_names == set(FilterBarColor.BUCKET_HUES.keys())


class TestToggleColor:
    """Toggling colours updates the active colour set."""

    def test_toggle_color_off(self, bar: FilterBarColor) -> None:
        """Toggling an already-active colour removes it."""
        bar.toggle_color(ColorBucket.BLUE)
        assert ColorBucket.BLUE in bar.get_active_colors()

        bar.toggle_color(ColorBucket.BLUE)

        assert ColorBucket.BLUE not in bar.get_active_colors()

    def test_toggle_via_button_click(self, bar: FilterBarColor) -> None:
        """Clicking a swatch button toggles the colour on."""
        btn = bar._swatch_buttons[ColorBucket.GREEN]

        btn.click()

        assert ColorBucket.GREEN in bar.get_active_colors()


class TestSetActiveColors:
    """set_active_colors updates the active colour set."""

    def test_set_active_colors_bare_names(self, bar: FilterBarColor) -> None:
        """set_active_colors accepts bare bucket names."""
        bar.set_active_colors({ColorBucket.RED, ColorBucket.BLUE})

        assert bar.get_active_colors() == {ColorBucket.RED, ColorBucket.BLUE}

    def test_set_active_colors_empty(self, bar: FilterBarColor) -> None:
        """Setting an empty set clears all active colours."""
        bar.set_active_colors({ColorBucket.RED, ColorBucket.BLUE})

        bar.set_active_colors(set())

        assert bar.get_active_colors() == set()


class TestColorFilterChangedSignal:
    """The color_filter_changed signal is emitted with the active set."""

    def test_color_filter_changed_signal_on_toggle(self, bar: FilterBarColor) -> None:
        """Toggling a swatch emits color_filter_changed with the active set."""
        captured: list[set[str]] = []
        bar.filter_changed.connect(lambda s: captured.append(s))

        bar.toggle_color(ColorBucket.RED)

        assert len(captured) == 1
        assert captured[0] == {ColorBucket.RED}

    def test_signal_emitted_on_set_active(self, bar: FilterBarColor) -> None:
        """set_active_colors also emits the signal."""
        captured: list[set[str]] = []
        bar.filter_changed.connect(lambda s: captured.append(s))

        bar.set_active_colors({ColorBucket.BLUE, ColorBucket.GREEN})

        assert len(captured) == 1
        assert captured[0] == {ColorBucket.BLUE, ColorBucket.GREEN}

    def test_signal_emitted_on_click(self, bar: FilterBarColor) -> None:
        """Clicking a swatch button emits the signal."""
        captured: list[set[str]] = []
        bar.filter_changed.connect(lambda s: captured.append(s))
        btn = bar._swatch_buttons[ColorBucket.YELLOW]

        btn.click()

        assert len(captured) == 1
        assert captured[0] == {ColorBucket.YELLOW}


class TestMultipleColorsActive:
    """Multiple colours can be active simultaneously."""

    def test_multiple_colors_active(self, bar: FilterBarColor) -> None:
        """Multiple colours can be active simultaneously."""
        bar.toggle_color(ColorBucket.RED)
        bar.toggle_color(ColorBucket.BLUE)
        bar.toggle_color(ColorBucket.GREEN)

        assert bar.get_active_colors() == {ColorBucket.RED, ColorBucket.BLUE, ColorBucket.GREEN}

    def test_deactivating_one_keeps_others(self, bar: FilterBarColor) -> None:
        """Deactivating one colour leaves the rest active."""
        bar.set_active_colors({ColorBucket.RED, ColorBucket.BLUE, ColorBucket.GREEN})

        bar.toggle_color(ColorBucket.BLUE)

        assert bar.get_active_colors() == {ColorBucket.RED, ColorBucket.GREEN}


class TestSwatchesHaveCorrectColors:
    """Swatch buttons expose the correct visual state."""

    def test_swatches_have_correct_colors(self, bar: FilterBarColor) -> None:
        """Each swatch button has the correct background colour in its stylesheet."""
        for bucket_name, hex_color in FilterBarColor.BUCKET_HUES.items():
            btn = bar._swatch_buttons[bucket_name]
            stylesheet = btn.styleSheet()
            assert hex_color in stylesheet, f"Swatch '{bucket_name}' stylesheet missing colour {hex_color}"

    def test_tooltips_show_bucket_names(self, bar: FilterBarColor) -> None:
        """Each swatch tooltip displays '<bucket_name>'."""
        for bucket_name in FilterBarColor.BUCKET_HUES:
            btn = bar._swatch_buttons[bucket_name]
            assert btn.toolTip() == f"{bucket_name}", f"Swatch '{bucket_name}' tooltip mismatch"

    def test_active_swatch_has_amber_border(self, bar: FilterBarColor) -> None:
        """An active swatch shows the amber border colour in its stylesheet."""
        bar.toggle_color(ColorBucket.RED)

        btn = bar._swatch_buttons[ColorBucket.RED]
        assert "#fac775" in btn.styleSheet()

    def test_inactive_swatch_lacks_amber_border(self, bar: FilterBarColor) -> None:
        """An inactive swatch does not show the amber border colour."""
        btn = bar._swatch_buttons[ColorBucket.RED]
        assert "#fac775" not in btn.styleSheet()
