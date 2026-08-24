"""Tests for natural sorting of gallery paths."""

from __future__ import annotations

from pathlib import Path

from tarragon.sorting import SortMode, natural_key, sort_paths


class TestNaturalKey:
    """natural_key produces a total order over path strings."""

    def test_no_digits(self) -> None:
        """Paths without digits sort by their casefolded string."""
        a = natural_key(Path("alpha.png"))
        b = natural_key(Path("beta.png"))
        assert a < b

    def test_digits_at_start(self) -> None:
        """Digit runs at the start of the name sort numerically."""
        a = natural_key(Path("2.png"))
        b = natural_key(Path("10.png"))
        assert a < b

    def test_digits_in_middle(self) -> None:
        """Digit runs in the middle of the name sort numerically."""
        a = natural_key(Path("name 2.png"))
        b = natural_key(Path("name 10.png"))
        assert a < b

    def test_digits_at_end(self) -> None:
        """Digit runs at the end of the name sort numerically."""
        a = natural_key(Path("photo2"))
        b = natural_key(Path("photo10"))
        assert a < b

    def test_multiple_digit_runs(self) -> None:
        """Multiple digit runs in one name are each compared numerically."""
        a = natural_key(Path("img 2 page 3.png"))
        b = natural_key(Path("img 10 page 3.png"))
        assert a < b

    def test_leading_zeros_pinned(self) -> None:
        """Leading zeros pin ordering: 001 before 01 before 1."""
        a = natural_key(Path("img 001.png"))
        b = natural_key(Path("img 01.png"))
        c = natural_key(Path("img 1.png"))
        assert a < b < c

    def test_case_insensitive_tiebreak(self) -> None:
        """Case-insensitive primary key with raw string tiebreak."""
        a = natural_key(Path("IMG 1.png"))
        b = natural_key(Path("img 1.png"))
        assert a < b

    def test_unicode_letters(self) -> None:
        """Unicode letters are casefolded for comparison."""
        a = natural_key(Path("caf\u00e9 2.png"))
        b = natural_key(Path("caf\u00e9 10.png"))
        assert a < b

    def test_unicode_digits(self) -> None:
        """Unicode digit runs are padded and compared numerically."""
        a = natural_key(Path("img \u0662.png"))
        b = natural_key(Path("img \u0661\u0660.png"))
        assert a < b

    def test_directory_grouping(self) -> None:
        """The full path is keyed so folder grouping is preserved."""
        a = natural_key(Path("/a/name 2.png"))
        b = natural_key(Path("/b/name 1.png"))
        assert a < b


class TestSortPaths:
    """sort_paths returns a naturally ordered list of paths."""

    def test_default_natural_order(self) -> None:
        """Default mode sorts name 1 before name 10."""
        paths = [Path(f"name {i}.png") for i in (1, 10, 2, 3)]
        result = sort_paths(paths)
        assert result == [Path(f"name {i}.png") for i in (1, 2, 3, 10)]

    def test_input_iterable_not_mutated(self) -> None:
        """The input list is left unchanged by sort_paths."""
        paths = [Path("name 10.png"), Path("name 2.png")]
        original = list(paths)
        sort_paths(paths)
        assert paths == original

    def test_empty_list(self) -> None:
        """An empty iterable yields an empty list."""
        assert sort_paths([]) == []

    def test_single_item(self) -> None:
        """A single path is returned unchanged."""
        path = Path("name 1.png")
        assert sort_paths([path]) == [path]

    def test_deterministic_across_input_orders(self) -> None:
        """Different input orders produce the same sorted result."""
        paths = [Path(f"name {i}.png") for i in (1, 10, 2, 3)]
        shuffled = [Path(f"name {i}.png") for i in (10, 3, 1, 2)]
        assert sort_paths(paths) == sort_paths(shuffled)

    def test_explicit_name_mode(self) -> None:
        """Passing SortMode.NAME explicitly sorts naturally."""
        paths = [Path("name 10.png"), Path("name 2.png")]
        assert sort_paths(paths, mode=SortMode.NAME) == [Path("name 2.png"), Path("name 10.png")]
