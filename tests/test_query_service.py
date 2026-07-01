"""Tests for tarragon.services.query_service — QueryService filter composition."""

from __future__ import annotations

from pathlib import Path

import pytest
from tarragon.db import Database
from tarragon.services.query_service import QueryService

# ── Helpers ──────────────────────────────────────────────────────────────


def _populate_test_data(db: Database) -> dict[str, int]:
    """Populate the in-memory DB with test thumbnails, tags, and file tags.

    Returns a mapping of tag name → tag id for use in tests.
    """
    # ── Thumbnails ────────────────────────────────────────────────────
    # Folder: /test/photos/
    db.upsert_thumbnail("/test/photos/sunset_beach.png", mtime=1, size=100, width=800, height=600, cache_uuid="u1")
    db.upsert_thumbnail("/test/photos/forest_path.jpg", mtime=2, size=200, width=1024, height=768, cache_uuid="u2")
    db.upsert_thumbnail("/test/photos/blue_ocean.jpg", mtime=3, size=300, width=1920, height=1080, cache_uuid="u3")
    db.upsert_thumbnail("/test/photos/green_valley.png", mtime=4, size=150, width=640, height=480, cache_uuid="u4")
    db.upsert_thumbnail("/test/photos/mountain_top.jpg", mtime=5, size=250, width=1280, height=720, cache_uuid="u5")
    db.upsert_thumbnail("/test/photos/doc.txt", mtime=6, size=10, width=100, height=100, cache_uuid="u6")

    # Folder: /test/other/
    db.upsert_thumbnail("/test/other/beach.png", mtime=7, size=120, width=800, height=600, cache_uuid="u7")

    # ── Tags (manual = 'user' source, colour = 'auto_color' source) ───
    tag_ids: dict[str, int] = {}

    # Manual tags
    tag_ids["beach"] = db.ensure_tag("beach")
    tag_ids["vacation"] = db.ensure_tag("vacation")
    tag_ids["nature"] = db.ensure_tag("nature")
    tag_ids["hiking"] = db.ensure_tag("hiking")

    # Color tags
    tag_ids["warm"] = db.ensure_tag("warm")
    tag_ids["green"] = db.ensure_tag("green")
    tag_ids["blue"] = db.ensure_tag("blue")

    # ── File-tag associations ─────────────────────────────────────────
    # sunset_beach.png  — beach (user), vacation (user), warm (auto_color)
    db.add_file_tags(["/test/photos/sunset_beach.png"], tag_ids["beach"], source="user")
    db.add_file_tags(["/test/photos/sunset_beach.png"], tag_ids["vacation"], source="user")
    db.add_file_tags(["/test/photos/sunset_beach.png"], tag_ids["warm"], source="auto_color")

    # forest_path.jpg   — nature (user), hiking (user), green (auto_color)
    db.add_file_tags(["/test/photos/forest_path.jpg"], tag_ids["nature"], source="user")
    db.add_file_tags(["/test/photos/forest_path.jpg"], tag_ids["hiking"], source="user")
    db.add_file_tags(["/test/photos/forest_path.jpg"], tag_ids["green"], source="auto_color")

    # blue_ocean.jpg    — beach (user), vacation (user), blue (auto_color)
    db.add_file_tags(["/test/photos/blue_ocean.jpg"], tag_ids["beach"], source="user")
    db.add_file_tags(["/test/photos/blue_ocean.jpg"], tag_ids["vacation"], source="user")
    db.add_file_tags(["/test/photos/blue_ocean.jpg"], tag_ids["blue"], source="auto_color")

    # green_valley.png  — nature (user), green (auto_color)
    db.add_file_tags(["/test/photos/green_valley.png"], tag_ids["nature"], source="user")
    db.add_file_tags(["/test/photos/green_valley.png"], tag_ids["green"], source="auto_color")

    # mountain_top.jpg  — hiking (user), blue (auto_color)
    db.add_file_tags(["/test/photos/mountain_top.jpg"], tag_ids["hiking"], source="user")
    db.add_file_tags(["/test/photos/mountain_top.jpg"], tag_ids["blue"], source="auto_color")

    # doc.txt — no tags at all

    # /test/other/beach.png  — beach (user), warm (auto_color)
    db.add_file_tags(["/test/other/beach.png"], tag_ids["beach"], source="user")
    db.add_file_tags(["/test/other/beach.png"], tag_ids["warm"], source="auto_color")

    return tag_ids


# ── Fixtures ─────────────────────────────────────────────────────────────


@pytest.fixture(scope="module")
def db() -> Database:
    """Provide a module-scoped in-memory SQLite database."""
    database = Database(Path(":memory:"))
    database.init_schema()
    return database


@pytest.fixture()
def service(db: Database) -> QueryService:
    """Provide a QueryService wired to the in-memory database."""
    return QueryService(db)


@pytest.fixture(scope="module")
def tag_ids(db: Database) -> dict[str, int]:
    """Populate test data once per module and return tag name → id mapping."""
    return _populate_test_data(db)


# ── Tests ───────────────────────────────────────────────────────────────


class TestQueryService:
    """Group all QueryService tests to match project convention."""

    def test_empty_filters_returns_all(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """No filters at all — returns every path under the folder."""
        result = service.query("/test/photos/")

        assert len(result) == 6
        paths = {str(p) for p in result}
        assert "/test/photos/sunset_beach.png" in paths
        assert "/test/photos/forest_path.jpg" in paths
        assert "/test/photos/blue_ocean.jpg" in paths
        assert "/test/photos/green_valley.png" in paths
        assert "/test/photos/mountain_top.jpg" in paths
        assert "/test/photos/doc.txt" in paths

    def test_empty_filters_returns_ordered(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """Results are ordered alphabetically by path."""
        result = service.query("/test/photos/")
        path_strs = [str(p) for p in result]
        assert path_strs == sorted(path_strs)

    def test_filename_filter_matches(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """Filename filter returns only paths whose basename contains the string."""
        result = service.query("/test/photos/", filename_filter="beach")

        paths = {str(p) for p in result}
        assert paths == {"/test/photos/sunset_beach.png"}

    def test_filename_filter_case_insensitive(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """Filename matching is case-insensitive (ASCII)."""
        result = service.query("/test/photos/", filename_filter="BEACH")

        paths = {str(p) for p in result}
        assert paths == {"/test/photos/sunset_beach.png"}

    def test_filename_filter_special_chars(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """Percent and underscore in the filter string are safely escaped."""
        # These special chars should be treated as literals, not LIKE wildcards
        result = service.query("/test/photos/", filename_filter="%_")

        # No path contains the literal string "%_" so result should be empty
        assert result == []

        # Also test that a path containing these chars won't match accidentally
        result2 = service.query("/test/photos/", filename_filter="do_")
        # "doc.txt" — the '_' should be literal, so "do_" won't match "doc.txt"
        # 'c' != '_', so no match
        assert result2 == []

    def test_tag_filter_requires_all_tags(self, service: QueryService, tag_ids: dict[str, int]) -> None:
        """AND semantics: file must have ALL specified tags."""
        result = service.query(
            "/test/photos/",
            tag_ids={tag_ids["beach"], tag_ids["vacation"]},
        )

        paths = {str(p) for p in result}
        # sunset_beach.png and blue_ocean.jpg have BOTH beach AND vacation
        assert paths == {
            "/test/photos/sunset_beach.png",
            "/test/photos/blue_ocean.jpg",
        }

    def test_tag_filter_no_matches(self, service: QueryService, tag_ids: dict[str, int]) -> None:
        """Tag filter that matches nothing returns an empty list."""
        # Combine two tags that never appear together
        result = service.query(
            "/test/photos/",
            tag_ids={tag_ids["beach"], tag_ids["nature"]},
        )

        assert result == []

    def test_color_tag_or_semantics(self, service: QueryService, tag_ids: dict[str, int]) -> None:
        """OR semantics: file with ANY of the specified colour tags matches."""
        result = service.query(
            "/test/photos/",
            color_tags={"green"},
        )

        paths = {str(p) for p in result}
        # forest_path.jpg and green_valley.png have "green" tag
        assert paths == {
            "/test/photos/forest_path.jpg",
            "/test/photos/green_valley.png",
        }

    def test_color_tag_or_semantics_multiple(self, service: QueryService, tag_ids: dict[str, int]) -> None:
        """Multiple colour tags: any match suffices."""
        result = service.query(
            "/test/photos/",
            color_tags={"green", "blue"},
        )

        paths = {str(p) for p in result}
        # green → forest_path.jpg, green_valley.png
        # blue  → blue_ocean.jpg, mountain_top.jpg
        assert paths == {
            "/test/photos/forest_path.jpg",
            "/test/photos/green_valley.png",
            "/test/photos/blue_ocean.jpg",
            "/test/photos/mountain_top.jpg",
        }

    def test_tag_and_color_combined(self, service: QueryService, tag_ids: dict[str, int]) -> None:
        """Both tag AND colour filters applied together (AND between groups).

        File must have ALL manual tags AND ANY of the colour tags.
        """
        result = service.query(
            "/test/photos/",
            tag_ids={tag_ids["nature"]},
            color_tags={"green"},
        )

        paths = {str(p) for p in result}
        # forest_path.jpg → nature (user) + green (auto_color)  ✓
        # green_valley.png → nature (user) + green (auto_color) ✓
        assert paths == {
            "/test/photos/forest_path.jpg",
            "/test/photos/green_valley.png",
        }

    def test_empty_folder_path_returns_all(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """Empty folder_path queries the entire database (global mode)."""
        result = service.query("")
        # Should return all 7 thumbnails across both folders
        assert len(result) == 7

    def test_no_matching_folder_returns_empty(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """Non-existent folder returns an empty list."""
        result = service.query("/nonexistent/")
        assert result == []

    def test_empty_tag_ids_set_ignored(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """Empty tag_ids set is treated as no tag filter (returns all)."""
        result = service.query("/test/photos/", tag_ids=set())
        assert len(result) == 6

    def test_empty_color_tags_set_ignored(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """Empty color_tags set is treated as no color filter (returns all)."""
        result = service.query("/test/photos/", color_tags=set())
        assert len(result) == 6

    # ── Bug 1: Global query (empty folder_path) ────────────────────────

    def test_global_query_returns_all_thumbnails(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """Empty folder_path queries the entire database (global mode)."""
        result = service.query("")
        # 6 in /test/photos/ + 1 in /test/other/ = 7 total
        assert len(result) == 7

    def test_global_query_with_tag_filter(
        self,
        service: QueryService,
        tag_ids: dict[str, int],
    ) -> None:
        """Global query with tag filter returns matches from all folders."""
        result = service.query("", tag_ids={tag_ids["beach"]})
        paths = {str(p) for p in result}
        # beach tag: sunset_beach.png, blue_ocean.jpg (photos) + beach.png (other)
        assert paths == {
            "/test/photos/sunset_beach.png",
            "/test/photos/blue_ocean.jpg",
            "/test/other/beach.png",
        }

    def test_global_query_with_color_filter(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """Global query with color filter returns matches from all folders."""
        result = service.query("", color_tags={"warm"})
        paths = {str(p) for p in result}
        # warm: sunset_beach.png (photos) + beach.png (other)
        assert paths == {
            "/test/photos/sunset_beach.png",
            "/test/other/beach.png",
        }

    def test_global_query_with_filename_filter(
        self,
        service: QueryService,
        tag_ids: dict[str, int],  # noqa: ARG002
    ) -> None:
        """Global query with filename filter works across all folders."""
        result = service.query("", filename_filter="beach")
        paths = {str(p) for p in result}
        assert paths == {
            "/test/photos/sunset_beach.png",
            "/test/other/beach.png",
        }
