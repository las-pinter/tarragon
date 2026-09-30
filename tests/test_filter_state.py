"""Tests for FilterState composite filter counting."""

from __future__ import annotations

from tarragon.db.common.tag import Tag
from tarragon.models.filter_state import FilterState

_ALPHA = Tag(id=1, name="alpha")
_BETA = Tag(id=2, name="beta")


class TestActiveCount:
    """Active filter counting across all filter dimensions."""

    def test_active_count_zero_when_empty(self) -> None:
        """A freshly cleared FilterState counts zero active filters."""
        assert FilterState().active_count() == 0

    def test_active_count_sums_all_dimensions(self) -> None:
        """active_count() sums tags, color tags, folder filters, and the filename filter."""
        state = FilterState(
            filename_filter="photo",
            tags={_ALPHA, _BETA},
            color_tags={_ALPHA},
            folder_filters={"/a", "/b"},
        )
        assert state.active_count() == 6

    def test_active_count_counts_filename_filter_once(self) -> None:
        """The filename filter contributes exactly one regardless of its text length."""
        state = FilterState(filename_filter="")
        assert state.active_count() == 0
        state.filename_filter = "x"
        assert state.active_count() == 1
        state.filename_filter = "a very long search phrase"
        assert state.active_count() == 1

    def test_active_count_after_clear_is_zero(self) -> None:
        """clear() resets active_count() to zero."""
        state = FilterState(
            filename_filter="photo",
            tags={_ALPHA},
            color_tags={_BETA},
            folder_filters={"/a"},
        )
        state.clear()
        assert state.active_count() == 0
