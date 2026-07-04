"""Tests for TagService — service-layer tag CRUD operations.

WAAAGH! Wrenchbasha's torture chamber for da TagService!
"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication
from tarragon.db import Database
from tarragon.services.tag_service import TagService

# =========================================================================
# Fixtures
# =========================================================================


@pytest.fixture
def db() -> Database:
    """Create an in-memory Database with initialised schema."""
    database = Database(Path(":memory:"))
    database.init_schema()
    return database


@pytest.fixture
def service(db: Database) -> TagService:
    """Create a TagService backed by an in-memory database."""
    return TagService(db=db)


# =========================================================================
# get_or_create_tag
# =========================================================================


class TestGetOrCreateTag:
    """get_or_create_tag — create vs. retrieve idempotency."""

    def test_get_or_create_tag_creates_and_returns_id(self, service: TagService) -> None:
        """First call creates a tag, second call returns the *same* id."""
        tag_id_1 = service.get_or_create_tag("character")
        assert isinstance(tag_id_1, int)
        assert tag_id_1 > 0

        tag_id_2 = service.get_or_create_tag("character")
        assert tag_id_2 == tag_id_1, "Same name should return same id"

    def test_get_or_create_tag_multiple_tags(self, service: TagService) -> None:
        """Distinct names get distinct ids."""
        id_a = service.get_or_create_tag("landscape")
        id_b = service.get_or_create_tag("portrait")
        assert id_a != id_b


# =========================================================================
# add_tags_to_files
# =========================================================================


class TestAddTagsToFiles:
    """add_tags_to_files — batch tagging."""

    def test_add_tags_to_files_adds_tags_to_multiple_paths(self, service: TagService) -> None:
        """Adding tags to multiple files — verify via get_tags_for_file."""
        paths = ["/img/a.png", "/img/b.png"]
        service.add_tags_to_files(paths, ["character", "landscape"])

        for path in paths:
            tags = service.get_tags_for_file(path)
            names = {t["name"] for t in tags}
            assert names == {"character", "landscape"}, f"{path} should have both tags"

    def test_add_tags_to_files_emits_tags_changed(self, service: TagService) -> None:
        """add_tags_to_files emits tagsChanged."""
        emitted = []
        service.tagsChanged.connect(lambda: emitted.append(True))

        service.add_tags_to_files(["/img/a.png"], ["test"])

        assert len(emitted) == 1, "tagsChanged should be emitted once"

    def test_add_tags_to_files_idempotent(self, service: TagService) -> None:
        """Adding the same tag twice does not duplicate."""
        service.add_tags_to_files(["/img/a.png"], ["dupe"])
        service.add_tags_to_files(["/img/a.png"], ["dupe"])

        tags = service.get_tags_for_file("/img/a.png")
        assert len(tags) == 1
        assert tags[0]["name"] == "dupe"


# =========================================================================
# remove_tags_from_files
# =========================================================================


class TestRemoveTagsFromFiles:
    """remove_tags_from_files — batch tag removal."""

    def test_remove_tags_from_files_removes_specified_tags(self, service: TagService) -> None:
        """Add tags then remove them — verify they're gone."""
        paths = ["/img/a.png", "/img/b.png"]
        service.add_tags_to_files(paths, ["character", "landscape"])

        # Get the tag ids
        tag_id = service.get_or_create_tag("landscape")

        service.remove_tags_from_files(paths, {tag_id})

        for path in paths:
            tags = service.get_tags_for_file(path)
            names = {t["name"] for t in tags}
            assert names == {"character"}, f"{path} should only have 'character' after removal"

    def test_remove_tags_from_files_emits_tags_changed(self, service: TagService) -> None:
        """remove_tags_from_files emits tagsChanged."""
        service.add_tags_to_files(["/img/a.png"], ["test"])
        tag_id = service.get_or_create_tag("test")

        emitted = []
        service.tagsChanged.connect(lambda: emitted.append(True))

        service.remove_tags_from_files(["/img/a.png"], {tag_id})

        assert len(emitted) == 1, "tagsChanged should be emitted once"

    def test_remove_tags_from_files_nonexistent_tag(self, service: TagService) -> None:
        """Removing a tag that doesn't exist does not raise."""
        service.add_tags_to_files(["/img/a.png"], ["keep"])
        # Should not raise
        service.remove_tags_from_files(["/img/a.png"], {9999})
        tags = service.get_tags_for_file("/img/a.png")
        assert len(tags) == 1
        assert tags[0]["name"] == "keep"


# =========================================================================
# get_tags_for_file
# =========================================================================


class TestGetTagsForFile:
    """get_tags_for_file — querying tags attached to a file."""

    def test_get_tags_for_file_returns_tag_list(self, service: TagService) -> None:
        """Returned list entries have id, name, source keys."""
        service.add_tags_to_files(["/img/a.png"], ["character"])

        tags = service.get_tags_for_file("/img/a.png")
        assert len(tags) == 1

        tag = tags[0]
        assert isinstance(tag["id"], int)
        assert tag["name"] == "character"
        assert tag["source"] == "user"

    def test_get_tags_for_file_no_tags(self, service: TagService) -> None:
        """File with no tags returns empty list."""
        tags = service.get_tags_for_file("/img/untagged.png")
        assert tags == []

    def test_get_tags_for_file_multiple_sources(self, service: TagService) -> None:
        """Different sources are reflected in the result."""
        service.add_tags_to_files(["/img/a.png"], ["user_tag"])

        # Manually add an auto_color tag to check source preservation
        tag_id = service.get_or_create_tag("auto_tag")
        service._db._execute(
            "INSERT INTO file_tags (path, tag_id, source) VALUES (?, ?, ?)",
            ("/img/a.png", tag_id, "auto_color"),
        )
        service._db._commit()

        tags = service.get_tags_for_file("/img/a.png")
        sources = {t["source"] for t in tags}
        assert "user" in sources
        assert "auto_color" in sources


# =========================================================================
# resolve_tri_state
# =========================================================================


class TestResolveTriState:
    """resolve_tri_state — checked / partially / unchecked."""

    def test_resolve_tri_state_all_checked(self, service: TagService) -> None:
        """All files have the tag → Checked."""
        paths = ["/img/a.png", "/img/b.png", "/img/c.png"]
        service.add_tags_to_files(paths, ["favourite"])

        tag_id = service.get_or_create_tag("favourite")
        state = service.resolve_tri_state(paths, tag_id)

        assert state == Qt.CheckState.Checked

    def test_resolve_tri_state_some_checked(self, service: TagService) -> None:
        """Some files have the tag → PartiallyChecked."""
        paths = ["/img/a.png", "/img/b.png", "/img/c.png"]
        service.add_tags_to_files(["/img/a.png", "/img/b.png"], ["favourite"])

        tag_id = service.get_or_create_tag("favourite")
        state = service.resolve_tri_state(paths, tag_id)

        assert state == Qt.CheckState.PartiallyChecked

    def test_resolve_tri_state_none_checked(self, service: TagService) -> None:
        """No files have the tag → Unchecked."""
        paths = ["/img/a.png", "/img/b.png", "/img/c.png"]
        # Don't add any tags

        tag_id = service.get_or_create_tag("favourite")
        state = service.resolve_tri_state(paths, tag_id)

        assert state == Qt.CheckState.Unchecked

    def test_resolve_tri_state_empty_paths(self, service: TagService) -> None:
        """Empty paths list → Unchecked (no files have the tag)."""
        tag_id = service.get_or_create_tag("lonely")
        state = service.resolve_tri_state([], tag_id)
        assert state == Qt.CheckState.Unchecked

    def test_resolve_tri_state_single_file(self, service: TagService) -> None:
        """Single file with tag → Checked."""
        service.add_tags_to_files(["/img/single.png"], ["tag"])
        tag_id = service.get_or_create_tag("tag")
        state = service.resolve_tri_state(["/img/single.png"], tag_id)
        assert state == Qt.CheckState.Checked


# =========================================================================
# get_all_tags
# =========================================================================


class TestGetAllTags:
    """get_all_tags — listing all tags with usage counts."""

    def test_get_all_tags_returns_all_tags(self, service: TagService) -> None:
        """Multiple tags created; all returned with correct counts."""
        service.get_or_create_tag("character")
        service.get_or_create_tag("landscape")
        service.get_or_create_tag("portrait")

        # Use the 'character' tag on two files
        char_id = service.get_or_create_tag("character")
        service._db.add_file_tags(["/img/a.png", "/img/b.png"], char_id)

        all_tags = service.get_all_tags()

        names = {t["name"] for t in all_tags}
        assert names == {"character", "landscape", "portrait"}

        for tag in all_tags:
            if tag["name"] == "character":
                assert tag["usage_count"] == 2
            elif tag["name"] == "landscape":
                assert tag["usage_count"] == 0
            elif tag["name"] == "portrait":
                assert tag["usage_count"] == 0

    def test_get_all_tags_empty(self, service: TagService) -> None:
        """No tags exist → empty list."""
        assert service.get_all_tags() == []

    def test_get_all_tags_ordered_by_name(self, service: TagService) -> None:
        """Tags are returned in alphabetical order."""
        service.get_or_create_tag("zebra")
        service.get_or_create_tag("alpha")
        service.get_or_create_tag("beta")

        all_tags = service.get_all_tags()
        names = [t["name"] for t in all_tags]
        assert names == ["alpha", "beta", "zebra"]


# =========================================================================
# get_all_tags with folder_path (Bug 1 — local scoped counts)
# =========================================================================


class TestGetAllTagsScoped:
    """get_all_tags with folder_path — local vs global usage counts."""

    def test_global_counts_all_files(self, service: TagService) -> None:
        """Without folder_path, usage_count spans the entire database."""
        tag_id = service.get_or_create_tag("beach")
        service._db.add_file_tags(["/folder_a/img1.png", "/folder_b/img2.png"], tag_id)

        tags = service.get_all_tags()
        assert tags[0]["usage_count"] == 2

    def test_local_counts_scope_to_folder(self, service: TagService) -> None:
        """With folder_path, usage_count only includes files in that folder."""
        tag_id = service.get_or_create_tag("beach")
        service._db.add_file_tags(["/folder_a/img1.png", "/folder_b/img2.png"], tag_id)

        tags = service.get_all_tags(folder_path="/folder_a/")
        assert tags[0]["usage_count"] == 1

    def test_local_counts_zero_for_other_folder(self, service: TagService) -> None:
        """Folder with no matching files returns usage_count=0."""
        tag_id = service.get_or_create_tag("beach")
        service._db.add_file_tags(["/folder_a/img1.png"], tag_id)

        tags = service.get_all_tags(folder_path="/folder_c/")
        assert tags[0]["usage_count"] == 0

    def test_none_folder_path_same_as_global(self, service: TagService) -> None:
        """folder_path=None returns global counts (same as no argument)."""
        tag_id = service.get_or_create_tag("test")
        service._db.add_file_tags(["/a/1.png", "/b/2.png"], tag_id)

        global_tags = service.get_all_tags()
        none_tags = service.get_all_tags(folder_path=None)
        assert global_tags[0]["usage_count"] == none_tags[0]["usage_count"] == 2

    def test_empty_folder_path_same_as_global(self, service: TagService) -> None:
        """folder_path='' returns global counts (same as no argument)."""
        tag_id = service.get_or_create_tag("test")
        service._db.add_file_tags(["/a/1.png"], tag_id)

        tags = service.get_all_tags(folder_path="")
        assert tags[0]["usage_count"] == 1


# =========================================================================
# delete_tag
# =========================================================================


class TestDeleteTag:
    """delete_tag — removes tag and emits tagsChanged."""

    def test_delete_tag_removes_tag(self, service: TagService) -> None:
        """delete_tag removes the tag from the database."""
        tag_id = service.get_or_create_tag("deleteme")
        service.delete_tag(tag_id)

        all_tags = service.get_all_tags()
        assert all(t["id"] != tag_id for t in all_tags)

    def test_delete_tag_emits_tags_changed(self, service: TagService) -> None:
        """delete_tag emits the tagsChanged signal."""
        tag_id = service.get_or_create_tag("signal-test")

        emitted: list[bool] = []
        service.tagsChanged.connect(lambda: emitted.append(True))

        service.delete_tag(tag_id)
        assert len(emitted) == 1

    def test_delete_tag_cascades_to_file_associations(
        self, service: TagService
    ) -> None:
        """Deleting a tag removes it from all files (CASCADE)."""
        tag_id = service.get_or_create_tag("cascade")
        service.add_tags_to_files(["/a.png", "/b.png"], ["cascade"])

        # Verify tags exist on files
        assert any(t["name"] == "cascade" for t in service.get_tags_for_file("/a.png"))

        service.delete_tag(tag_id)

        # Tag should be gone from all files
        assert not any(t["name"] == "cascade" for t in service.get_tags_for_file("/a.png"))
        assert not any(t["name"] == "cascade" for t in service.get_tags_for_file("/b.png"))


# =========================================================================
# get_tag_name
# =========================================================================


class TestGetTagName:
    """get_tag_name — retrieve tag name by ID."""

    def test_get_tag_name_returns_name(self, service: TagService) -> None:
        """get_tag_name returns the correct name for an existing tag."""
        tag_id = service.get_or_create_tag("my-tag")
        assert service.get_tag_name(tag_id) == "my-tag"

    def test_get_tag_name_returns_none_for_missing(self, service: TagService) -> None:
        """get_tag_name returns None for a non-existent tag id."""
        assert service.get_tag_name(99999) is None

    def test_get_tag_name_color_tag(self, service: TagService) -> None:
        """get_tag_name works for color tags too."""
        tag_id = service.get_or_create_tag("color:red")
        assert service.get_tag_name(tag_id) == "color:red"
