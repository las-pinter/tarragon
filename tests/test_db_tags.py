"""Tests for Tag related Databsse operations"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest

from tarragon.db.common.tag import Tag, TagSource
from tarragon.db.database import Database


@pytest.fixture
def db() -> Generator[Database, None, None]:
    """Provide an in-memory database for each test (isolated)."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


TEST_FILE_1 = "/test_folder_1/test_file_1.png"
TEST_FILE_2 = "/test_folder_2/test_file_2.png"
TEST_FILE_3 = "/test_folder_3/test_file_3.png"

TEST_TAG_NAME_1 = "test tag 1"
TEST_TAG_NAME_2 = "test tag 2"
TEST_TAG_NAME_3 = "test tag 3"
TEST_TAG_NAME_4 = "test tag 4"


def _is_tag_associated_with_files(db: Database, paths: list[str], tag: Tag) -> bool:
    tag_dict = db.get_tags_for_files(paths)
    for tags in tag_dict.values():
        if tag not in tags:
            return False

    return True


def _is_tag_associated_with_file(db: Database, path: str, tag: Tag) -> bool:
    tags = db.get_tags_for_file(path)
    return tag in tags


class TestEnsureTag:
    """Creates tags and returns stable IDs."""

    def test_create_new_tag(self, db: Database) -> None:
        """Creates a new tag and returns a valid tag."""
        tag = db.ensure_tag(TEST_TAG_NAME_1)
        assert isinstance(tag.get_id(), int)
        assert tag.get_id() > 0

    def test_returns_same_tag_on_repeat(self, db: Database) -> None:
        """Returns the same ID for the same tag name."""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_1)
        assert tag_1 == tag_2

    def test_different_tags_get_different_ids(self, db: Database) -> None:
        """Different tag names receive different IDs."""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_2)
        tag_3 = db.ensure_tag(TEST_TAG_NAME_3)
        assert tag_1 != tag_2 != tag_3

    def test_ensure_tag_keeps_first_creator_source(self, db: Database) -> None:
        """ensure_tag never rewrites an existing row's source (first creator wins)."""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1, TagSource.AUTO_COLOR)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_1, TagSource.USER)
        assert tag_1 != tag_2  # in-memory sources differ

        stored = db.get_all_tags()
        assert len(stored) == 1
        assert next(iter(stored)).get_source() == TagSource.AUTO_COLOR

    def test_ensure_tag_does_not_flip_user_source_to_auto(self, db: Database) -> None:
        """Requesting AUTO_COLOR on an existing USER row leaves the stored source alone."""
        db.ensure_tag(TEST_TAG_NAME_1, TagSource.USER)
        db.ensure_tag(TEST_TAG_NAME_1, TagSource.AUTO_COLOR)

        stored = db.get_all_tags()
        assert len(stored) == 1
        assert next(iter(stored)).get_source() == TagSource.USER


class TestAddTagsToFile:
    """Associates tags with file paths."""

    def test_add_single_tag_to_path(self, db: Database) -> None:
        """Associates one tag with a path."""
        tag = db.ensure_tag(TEST_TAG_NAME_1)

        tag.set_source(TagSource.USER)
        db.add_tag_to_file(TEST_FILE_1, tag)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag)

    def test_adds_same_tag_to_multiple_paths(self, db: Database) -> None:
        """add_file_tags associates a tag with multiple paths."""
        tag = db.ensure_tag(TEST_TAG_NAME_1)
        paths = [TEST_FILE_1, TEST_FILE_2, TEST_FILE_3]

        tag.set_source(TagSource.USER)
        db.add_tag_to_files(paths, tag)

        for p in paths:
            assert _is_tag_associated_with_file(db, p, tag)

    def test_default_source_is_user(self, db: Database) -> None:
        """add_file_tags defaults the association source to 'user'."""
        tag = db.ensure_tag(TEST_TAG_NAME_1)

        tag.set_source(TagSource.USER)
        db.add_tag_to_file(TEST_FILE_1, tag)

        tags = db.get_tags_for_file(TEST_FILE_1)
        for tag in tags:
            assert tag.get_source() == TagSource.USER


class TestGetTagsForFiles:
    """Returns the tags for file paths."""

    def test_empty_for_missing_path(self, db: Database) -> None:
        """Returns an empty set for a missing path."""

        assert db.get_tags_for_file(TEST_FILE_1) == set()

    def test_return_one_tag(self, db: Database) -> None:
        """Returns a tag for a file"""
        tag = db.ensure_tag(TEST_TAG_NAME_1)

        db.add_tag_to_file(TEST_FILE_1, tag)

        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag)

    def test_return_one_tag_for_multiple_files(self, db: Database) -> None:
        """Returns a tag for multiple files"""
        tag = db.ensure_tag(TEST_TAG_NAME_1)

        db.add_tag_to_file(TEST_FILE_1, tag)
        db.add_tag_to_file(TEST_FILE_2, tag)

        assert _is_tag_associated_with_files(db, [TEST_FILE_1, TEST_FILE_2], tag)

    def test_return_mutliple_tags(self, db: Database) -> None:
        """Returns multiple tags for a file."""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_2)
        tag_3 = db.ensure_tag(TEST_TAG_NAME_3)

        db.add_tag_to_file(TEST_FILE_1, tag_1)
        db.add_tag_to_file(TEST_FILE_1, tag_2)
        db.add_tag_to_file(TEST_FILE_1, tag_3)

        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_1)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_2)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_3)

    def test_return_mutliple_tags_for_multiple_files(self, db: Database) -> None:
        """Returns multiple tags for multiple files."""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_2)
        tag_3 = db.ensure_tag(TEST_TAG_NAME_3)

        db.add_tag_to_file(TEST_FILE_1, tag_1)
        db.add_tag_to_file(TEST_FILE_1, tag_2)
        db.add_tag_to_file(TEST_FILE_1, tag_3)
        db.add_tag_to_file(TEST_FILE_2, tag_1)
        db.add_tag_to_file(TEST_FILE_2, tag_2)
        db.add_tag_to_file(TEST_FILE_3, tag_2)
        db.add_tag_to_file(TEST_FILE_3, tag_3)

        assert _is_tag_associated_with_files(db, [TEST_FILE_1, TEST_FILE_2], tag_1)
        assert not _is_tag_associated_with_file(db, TEST_FILE_3, tag_1)
        assert _is_tag_associated_with_files(db, [TEST_FILE_1, TEST_FILE_2, TEST_FILE_3], tag_2)
        assert _is_tag_associated_with_files(db, [TEST_FILE_1, TEST_FILE_3], tag_2)
        assert not _is_tag_associated_with_file(db, TEST_FILE_2, tag_3)

    def test_return_tag_to_unnormal_file_path(self, db: Database) -> None:
        tag = db.ensure_tag(TEST_TAG_NAME_1)

        unnorm_file = "\\\\test_folder_1\\\\test_file_1.png"
        db.add_tag_to_file(unnorm_file, tag)

        assert _is_tag_associated_with_file(db, unnorm_file, tag)


class TestGetAllTags:
    """Retrieving all the tags stored in the database"""

    def test_empty(self, db: Database) -> None:
        """Returns empty set if there are no tags in the database"""
        assert db.get_all_tags() == set()

    def test_get_all_tags_without_associations(self, db: Database) -> None:
        """Returns all the tags without file associations"""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_2)
        tag_3 = db.ensure_tag(TEST_TAG_NAME_3)
        tag_1.set_usage_count(0)
        tag_2.set_usage_count(0)
        tag_3.set_usage_count(0)

        tags = db.get_all_tags()

        assert tag_1 in tags
        assert tag_2 in tags
        assert tag_3 in tags

    def test_get_all_tags_while_associated_to_files(self, db: Database) -> None:
        """Returns all the tags with file associations"""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_2)
        tag_3 = db.ensure_tag(TEST_TAG_NAME_3)

        db.add_tag_to_file(TEST_FILE_1, tag_1)
        db.add_tag_to_file(TEST_FILE_1, tag_2)
        db.add_tag_to_file(TEST_FILE_1, tag_3)
        db.add_tag_to_file(TEST_FILE_2, tag_2)
        db.add_tag_to_file(TEST_FILE_2, tag_3)
        db.add_tag_to_file(TEST_FILE_3, tag_3)

        tag_1.set_usage_count(1)
        tag_2.set_usage_count(2)
        tag_3.set_usage_count(3)

        tags = db.get_all_tags()

        assert tag_1 in tags
        assert tag_2 in tags
        assert tag_3 in tags

    def test_get_all_tags_mixed(self, db: Database) -> None:
        """Returns all the tags with or without file associations"""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_2)
        tag_3 = db.ensure_tag(TEST_TAG_NAME_3)

        db.add_tag_to_file(TEST_FILE_3, tag_3)

        tag_1.set_usage_count(0)
        tag_2.set_usage_count(0)
        tag_3.set_usage_count(1)

        tags = db.get_all_tags()

        assert tag_1 in tags
        assert tag_2 in tags
        assert tag_3 in tags


class TestRemoveFileTags:
    """Removes tag associations."""

    def test_remove_tag_from_single_file(self, db: Database) -> None:
        """Removes the given tag from a single file"""
        tag = db.ensure_tag(TEST_TAG_NAME_1)
        db.add_tag_to_file(TEST_FILE_1, tag)

        db.remove_tag_from_file(TEST_FILE_1, tag)
        assert not _is_tag_associated_with_file(db, TEST_FILE_1, tag)

    def test_remove_specific_tag(self, db: Database) -> None:
        """Removes the given tag association only."""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_2)
        tag_3 = db.ensure_tag(TEST_TAG_NAME_3)

        db.add_tag_to_file(TEST_FILE_1, tag_1)
        db.add_tag_to_file(TEST_FILE_1, tag_2)
        db.add_tag_to_file(TEST_FILE_1, tag_3)

        db.remove_tag_from_file(TEST_FILE_1, tag_1)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_2)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_3)
        assert not _is_tag_associated_with_file(db, TEST_FILE_1, tag_1)

    def test_remove_not_affecting_other_files(self, db: Database) -> None:
        """Leaves other paths and tags untouched."""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_2)
        tag_3 = db.ensure_tag(TEST_TAG_NAME_3)

        db.add_tag_to_file(TEST_FILE_1, tag_1)
        db.add_tag_to_file(TEST_FILE_2, tag_2)
        db.add_tag_to_file(TEST_FILE_2, tag_3)

        db.remove_tag_from_file(TEST_FILE_1, tag_1)
        assert not _is_tag_associated_with_file(db, TEST_FILE_1, tag_1)
        assert _is_tag_associated_with_file(db, TEST_FILE_2, tag_2)
        assert _is_tag_associated_with_file(db, TEST_FILE_2, tag_3)


class TestDeleteTag:
    """Removes tags and cascades to file tags."""

    def test_delete_removes_tag(self, db: Database) -> None:
        """Removes the tag from the tags table."""
        tag = db.ensure_tag(TEST_TAG_NAME_1)

        db.delete_tag(tag)

        tags = db.get_all_tags()
        assert tag not in tags
        assert tags == set()

    def test_delete_cascades_to_file_tags(self, db: Database) -> None:
        """Deleting a tag CASCADE-deletes all file-tag associations."""
        tag = db.ensure_tag(TEST_TAG_NAME_1)
        test_files = [TEST_FILE_1, TEST_FILE_2, TEST_FILE_3]

        tag.set_source(TagSource.USER)
        db.add_tag_to_files(test_files, tag)

        # Verify associations exist
        for f in test_files:
            assert _is_tag_associated_with_file(db, f, tag)

        # Delete the tag
        db.delete_tag(tag)

        # All associations should be gone
        for f in test_files:
            assert not _is_tag_associated_with_file(db, f, tag)

    def test_delete_nonexistent_tag_does_not_error(self, db: Database) -> None:
        """Deleting a non-existent tag silently succeeds."""
        tag = Tag(id=99999, name="Non Existing", usage_paths=None)
        db.delete_tag(tag)  # Should not raise

    def test_delete_tag_preserves_other_tags(self, db: Database) -> None:
        """Deleting one tag does not affect other tags."""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_2)

        db.add_tag_to_file(TEST_FILE_1, tag_1)
        db.add_tag_to_file(TEST_FILE_2, tag_2)

        db.delete_tag(tag_1)

        assert not _is_tag_associated_with_file(db, TEST_FILE_1, tag_1)
        assert _is_tag_associated_with_file(db, TEST_FILE_2, tag_2)

        # Now the tag will get the usage count
        tag_2.set_usage_count(1)
        tags = db.get_all_tags()
        assert tag_1 not in tags
        assert tag_2 in tags


class TestReplaceAutoColorTags:
    """Swaps auto-color tags for a path."""

    def test_replace_old_auto_color_with_new(self, db: Database) -> None:
        """Old auto_color tags are deleted and new ones inserted."""
        tag_old = db.ensure_tag(TEST_TAG_NAME_1, TagSource.AUTO_COLOR)
        tag_new_1 = db.ensure_tag(TEST_TAG_NAME_2, TagSource.AUTO_COLOR)
        tag_new_2 = db.ensure_tag(TEST_TAG_NAME_3, TagSource.AUTO_COLOR)

        db.add_tag_to_file(TEST_FILE_1, tag_old)

        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_old)

        # Replace with new auto_color tags
        db.replace_auto_color_tags(TEST_FILE_1, {tag_new_1, tag_new_2})

        assert not _is_tag_associated_with_file(db, TEST_FILE_1, tag_old)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_new_1)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_new_2)

    def test_clears_auto_color_when_empty_list(self, db: Database) -> None:
        """Passing an empty list removes all auto_color tags for the path."""
        tag_1 = db.ensure_tag(TEST_TAG_NAME_1, TagSource.AUTO_COLOR)
        tag_2 = db.ensure_tag(TEST_TAG_NAME_2, TagSource.USER)

        db.add_tag_to_file(TEST_FILE_1, tag_1)
        db.add_tag_to_file(TEST_FILE_1, tag_2)

        db.replace_auto_color_tags(TEST_FILE_1, set())
        assert not _is_tag_associated_with_file(db, TEST_FILE_1, tag_1)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_2)

    def test_only_replaces_auto_color_tags(self, db: Database):
        """The replacer only changes the auto_color tags, not the user ones"""
        tag_old_1 = db.ensure_tag(TEST_TAG_NAME_1, TagSource.AUTO_COLOR)
        tag_old_2 = db.ensure_tag(TEST_TAG_NAME_2, TagSource.USER)
        tag_new_1 = db.ensure_tag(TEST_TAG_NAME_3, TagSource.AUTO_COLOR)
        tag_new_2 = db.ensure_tag(TEST_TAG_NAME_4, TagSource.AUTO_COLOR)

        db.add_tag_to_file(TEST_FILE_1, tag_old_1)
        db.add_tag_to_file(TEST_FILE_1, tag_old_2)

        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_old_1)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_old_2)

        # Replace with new auto_color tags
        db.replace_auto_color_tags(TEST_FILE_1, {tag_new_1, tag_new_2})

        assert not _is_tag_associated_with_file(db, TEST_FILE_1, tag_old_1)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_old_2)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_new_1)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag_new_2)

    def test_replace_auto_color_only_affects_target_path(self, db: Database) -> None:
        """Replacing auto-color tags on one path never touches other paths.

        Regression: the old cleanup DELETE matched tag ids via the target path
        but deleted rows globally, stripping the tag from every file.
        """
        tag = db.ensure_tag(TEST_TAG_NAME_1, TagSource.AUTO_COLOR)
        db.add_tag_to_file(TEST_FILE_1, tag)
        db.add_tag_to_file(TEST_FILE_2, tag)
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag)
        assert _is_tag_associated_with_file(db, TEST_FILE_2, tag)

        db.replace_auto_color_tags(TEST_FILE_1, set())

        assert not _is_tag_associated_with_file(db, TEST_FILE_1, tag)
        assert _is_tag_associated_with_file(db, TEST_FILE_2, tag)

    def test_user_tag_survives_auto_color_processing(self, db: Database) -> None:
        """A USER tag association survives auto-color replacement and cleanup.

        Regression: the per-association source must protect user tag rows from
        the auto pipeline's cleanup, and ensure_tag must not flip the tags
        table source.
        """
        user_tag = db.ensure_tag(TEST_TAG_NAME_1, TagSource.USER)
        db.add_tag_to_file(TEST_FILE_1, user_tag)

        # The auto pipeline detects the same name on the same file
        db.replace_auto_color_tags(TEST_FILE_1, {user_tag})
        assert _is_tag_associated_with_file(db, TEST_FILE_1, user_tag)

        # A later cleanup pass must not delete the user's association
        db.replace_auto_color_tags(TEST_FILE_1, set())
        assert _is_tag_associated_with_file(db, TEST_FILE_1, user_tag)

        # The tags table source is never flipped
        stored = db.get_all_tags()
        assert len(stored) == 1
        assert next(iter(stored)).get_source() == TagSource.USER

    def test_auto_color_association_cleaned_up_on_later_replace(self, db: Database) -> None:
        """A second replace removes the auto association from the first pass.

        Regression: stale auto-color associations accumulated because the
        association source was only set in memory; the cleanup pass could not
        find rows to delete.
        """
        tag = db.ensure_tag(TEST_TAG_NAME_1, TagSource.USER)
        db.replace_auto_color_tags(TEST_FILE_1, {tag})
        assert _is_tag_associated_with_file(db, TEST_FILE_1, tag)

        db.replace_auto_color_tags(TEST_FILE_1, set())
        assert not _is_tag_associated_with_file(db, TEST_FILE_1, tag)

    def test_association_source_reflects_insert_path(self, db: Database) -> None:
        """file_tags.source records who created each association."""
        manual = db.ensure_tag(TEST_TAG_NAME_1, TagSource.USER)
        auto = db.ensure_tag(TEST_TAG_NAME_2, TagSource.AUTO_COLOR)

        db.add_tag_to_file(TEST_FILE_1, manual)
        db.replace_auto_color_tags(TEST_FILE_2, {auto})

        manual_rows = db.fetch_all("SELECT source FROM file_tags WHERE path = ?", (TEST_FILE_1,))
        auto_rows = db.fetch_all("SELECT source FROM file_tags WHERE path = ?", (TEST_FILE_2,))
        assert [r["source"] for r in manual_rows] == ["user"]
        assert [r["source"] for r in auto_rows] == ["auto_color"]

    def test_user_add_promotes_existing_auto_association(self, db: Database) -> None:
        """A user's manual add upgrades an auto association so cleanup keeps it.

        Regression: INSERT OR IGNORE left an existing 'auto_color' row intact
        when the user later added the same tag on the same path; the next
        cleanup pass then deleted the user's association (silent data loss).
        """
        color = db.ensure_tag(TEST_TAG_NAME_1, TagSource.AUTO_COLOR)
        db.replace_auto_color_tags(TEST_FILE_1, {color})
        assert _is_tag_associated_with_file(db, TEST_FILE_1, color)

        # User later adds the same name on the same path
        user_tag = db.ensure_tag(TEST_TAG_NAME_1, TagSource.USER)
        db.add_tag_to_file(TEST_FILE_1, user_tag)

        # The association row is now user-owned (assert BEFORE cleanup)
        rows = db.fetch_all("SELECT source FROM file_tags WHERE path = ?", (TEST_FILE_1,))
        assert [r["source"] for r in rows] == ["user"]

        # Cleanup must not delete the (now user-owned) association
        db.replace_auto_color_tags(TEST_FILE_1, set())
        assert _is_tag_associated_with_file(db, TEST_FILE_1, color)

        # The auto pipeline must never downgrade the user-owned row either
        db.replace_auto_color_tags(TEST_FILE_1, {color})
        rows = db.fetch_all("SELECT source FROM file_tags WHERE path = ?", (TEST_FILE_1,))
        assert [r["source"] for r in rows] == ["user"]
