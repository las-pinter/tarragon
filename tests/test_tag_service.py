"""Tests for TagService"""

from __future__ import annotations

from pathlib import Path

import pytest

from tarragon.db.common.tag import Tag
from tarragon.db.database import Database
from tarragon.services.tag_service import TagService


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


TEST_FILE_1 = "/test_folder_1/test_file_1.png"
TEST_FILE_2 = "/test_folder_2/test_file_2.png"
TEST_FILE_3 = "/test_folder_3/test_file_3.png"

TEST_TAG_NAME_1 = "test tag 1"
TEST_TAG_NAME_2 = "test tag 2"
TEST_TAG_NAME_3 = "test tag 3"
TEST_TAG_NAME_4 = "test tag 4"


class TestCreateTag:
    """Creating tags without associtations"""

    def test_create_single_tag(self, service: TagService) -> None:
        """Creating a single tag without association"""
        created_tag = service.create_tag(TEST_TAG_NAME_1)
        assert created_tag.get_name() == TEST_TAG_NAME_1
        assert created_tag.get_id() > 0


class TestAddTagsToFiles:
    """Adding tags to files"""

    def test_add_a_single_tag_to_a_file_by_name(self, service: TagService) -> None:
        """Adding tags to a file."""
        created_tags = service.add_tags_to_file_by_name(TEST_FILE_1, [TEST_TAG_NAME_1])

        tags = service.get_tags_for_file(TEST_FILE_1)

        assert len(tags) == 1
        for t in created_tags:
            assert t in tags

    def test_add_a_single_tag_to_a_file(self, service: TagService) -> None:
        """Adding tags to a file."""
        created_tag = service.create_tag(TEST_TAG_NAME_1)
        service.add_tags_to_file(TEST_FILE_1, {created_tag})

        tags = service.get_tags_for_file(TEST_FILE_1)

        assert len(tags) == 1
        assert created_tag in tags

    def test_add_multiple_tags_to_a_file_by_name(self, service: TagService) -> None:
        """Adding multiple tags to a file."""
        created_tags = service.add_tags_to_file_by_name(TEST_FILE_1, [TEST_TAG_NAME_1, TEST_TAG_NAME_2])

        tags = service.get_tags_for_file(TEST_FILE_1)

        assert len(tags) == 2
        for t in created_tags:
            assert t in tags

    def test_add_multiple_tags_to_a_file(self, service: TagService) -> None:
        """Adding multiple tags to a file."""
        created_tag_1 = service.create_tag(TEST_TAG_NAME_1)
        created_tag_2 = service.create_tag(TEST_TAG_NAME_2)
        created_tags = {created_tag_1, created_tag_2}
        service.add_tags_to_file(TEST_FILE_1, {created_tag_1, created_tag_2})

        tags = service.get_tags_for_file(TEST_FILE_1)

        assert len(tags) == 2
        for t in created_tags:
            assert t in tags

    def test_add_multiple_tags_to_multiple_files_by_name(self, service: TagService) -> None:
        """Adding tags to multiple files."""
        paths = [TEST_FILE_1, TEST_FILE_2]
        created_tags = service.add_tags_to_files_by_name(paths, [TEST_TAG_NAME_1, TEST_TAG_NAME_2])

        for path in paths:
            tags = service.get_tags_for_file(path)
            assert len(tags) == 2
            for t in created_tags:
                assert t in tags

    def test_add_multiple_tags_to_multiple_files(self, service: TagService) -> None:
        """Adding tags to multiple files."""
        paths = [TEST_FILE_1, TEST_FILE_2]
        created_tag_1 = service.create_tag(TEST_TAG_NAME_1)
        created_tag_2 = service.create_tag(TEST_TAG_NAME_2)
        created_tags = {created_tag_1, created_tag_2}
        service.add_tags_to_files(paths, {created_tag_1, created_tag_2})

        for path in paths:
            tags = service.get_tags_for_file(path)
            assert len(tags) == 2
            for t in created_tags:
                assert t in tags

    def test_emit_tags_changed(self, service: TagService) -> None:
        """Emits tags_changed."""
        emitted: list[bool] = []
        service.tags_changed.connect(lambda: emitted.append(True))

        service.add_tags_to_file_by_name(TEST_FILE_1, [TEST_TAG_NAME_1])

        assert len(emitted) == 1, "tags_changed should be emitted once"

    def test_emit_tags_changed_2(self, service: TagService) -> None:
        """Emits tags_changed."""
        emitted: list[bool] = []
        service.tags_changed.connect(lambda: emitted.append(True))

        service.add_tags_to_file_by_name(TEST_FILE_1, [TEST_TAG_NAME_1])
        service.add_tags_to_file_by_name(TEST_FILE_2, [TEST_TAG_NAME_2])

        assert len(emitted) == 2, "tags_changed should be emitted twice"

    def test_add_tags_to_files_idempotent(self, service: TagService) -> None:
        """Adding the same tag multiple times does not duplicate."""
        created_tag_1 = service.add_tags_to_files_by_name([TEST_FILE_1], [TEST_TAG_NAME_1])
        created_tag_2 = service.add_tags_to_files_by_name([TEST_FILE_1], [TEST_TAG_NAME_1])
        created_tag_3 = service.add_tags_to_file_by_name(TEST_FILE_1, [TEST_TAG_NAME_1])
        created_tag_4 = service.add_tags_to_file_by_name(TEST_FILE_1, [TEST_TAG_NAME_1])

        assert created_tag_1 == created_tag_2 == created_tag_3 == created_tag_4

        tags = service.get_tags_for_file(TEST_FILE_1)
        assert len(tags) == 1
        for t in created_tag_1:
            assert t in tags


class TestGetTagsForFiles:
    """Querying tags attached to files."""

    def test_get_tags_for_single_file_no_result(self, service: TagService) -> None:
        """Returns an empty result for a file which doesn't have tags"""
        tags = service.get_tags_for_file(TEST_FILE_1)
        assert len(tags) == 0

    def test_get_tags_for_multiple_files_no_result(self, service: TagService) -> None:
        """Returns an empty result for multiple files which don't have tags"""
        tags_dict = service.get_tags_for_files([TEST_FILE_1, TEST_FILE_2])
        assert len(tags_dict[TEST_FILE_1]) == 0
        assert len(tags_dict[TEST_FILE_2]) == 0

    def test_get_tags_for_single_file(self, service: TagService) -> None:
        """Returns a set of tags for a file"""
        tags_created = service.add_tags_to_file_by_name(TEST_FILE_1, [TEST_TAG_NAME_1])

        tags = service.get_tags_for_file(TEST_FILE_1)

        assert len(tags) == 1
        for t in tags_created:
            assert t in tags

    def test_get_tags_for_multiple_files(self, service: TagService) -> None:
        """Returns a set of tags for a file"""
        tags_created = service.add_tags_to_files_by_name([TEST_FILE_1, TEST_FILE_2], [TEST_TAG_NAME_1])

        tags_dict = service.get_tags_for_files([TEST_FILE_1, TEST_FILE_2])

        assert len(tags_dict[TEST_FILE_1]) == 1
        assert len(tags_dict[TEST_FILE_2]) == 1
        for t in tags_created:
            assert t in tags_dict[TEST_FILE_1]
            assert t in tags_dict[TEST_FILE_2]


class TestGetAllTags:
    """Listing all tags with usage counts."""

    def test_empty(self, service: TagService) -> None:
        """Returns empty list if there is no tag in the db"""
        assert service.get_all_tags() == []

    def test_returns_all_tags(self, service: TagService) -> None:
        """Returns multiple tags with usage count"""
        created_tag_na_1 = service.create_tag(TEST_TAG_NAME_1)
        created_tag_na_2 = service.create_tag(TEST_TAG_NAME_2)
        created_tags_na = {created_tag_na_1, created_tag_na_2}
        created_tags_a = service.add_tags_to_files_by_name(
            [TEST_FILE_1, TEST_FILE_2], [TEST_TAG_NAME_3, TEST_TAG_NAME_4]
        )

        for t_na in created_tags_na:
            t_na.set_usage_count(0)

        for t_a in created_tags_a:
            t_a.set_usage_count(2)

        all_tags = service.get_all_tags()

        for t_na in created_tags_na:
            assert t_na in all_tags

        for t_a in created_tags_a:
            assert t_a in all_tags

    def test_ordered_by_name(self, service: TagService) -> None:
        """Tags are returned in alphabetical order."""
        tag_1 = service.create_tag(TEST_TAG_NAME_1)
        tag_2 = service.create_tag(TEST_TAG_NAME_2)
        tag_3 = service.create_tag(TEST_TAG_NAME_3)
        tag_4 = service.create_tag(TEST_TAG_NAME_4)

        ordered_tag_names = sorted([t.get_name() for t in [tag_1, tag_2, tag_3, tag_4]])
        all_tags = service.get_all_tags()
        names = [t.get_name() for t in all_tags]
        assert names == ordered_tag_names


class TestRemoveTagsFromFiles:
    """Tag removal."""

    def test_remove_single_tag_from_a_file(self, service: TagService) -> None:
        """Remove a tag from a file"""
        created_tags = service.add_tags_to_file_by_name(TEST_FILE_1, [TEST_TAG_NAME_1])
        tags = service.get_tags_for_file(TEST_FILE_1)

        assert len(tags) == 1
        for t in created_tags:
            assert t in tags

        tag = created_tags.pop()
        service.remove_tags_from_file(TEST_FILE_1, {tag})

        tags = service.get_tags_for_file(TEST_FILE_1)
        assert len(tags) == 0

    def test_remove_multiple_tags_from_a_file(self, service: TagService) -> None:
        """Remove multiple tags from a file"""
        created_tags = service.add_tags_to_file_by_name(TEST_FILE_1, [TEST_TAG_NAME_1, TEST_TAG_NAME_2])
        tags = service.get_tags_for_file(TEST_FILE_1)

        assert len(tags) == 2
        for t in created_tags:
            assert t in tags

        service.remove_tags_from_file(TEST_FILE_1, tags)

        tags = service.get_tags_for_file(TEST_FILE_1)
        assert len(tags) == 0

    def test_remove_multiple_tags_from_a_file_not_all(self, service: TagService) -> None:
        """Remove multiple tags from a file while leaving some"""
        created_tags = service.add_tags_to_file_by_name(
            TEST_FILE_1, [TEST_TAG_NAME_1, TEST_TAG_NAME_2, TEST_TAG_NAME_3]
        )
        tags = service.get_tags_for_file(TEST_FILE_1)

        assert len(tags) == 3
        for t in created_tags:
            assert t in tags

        tag_1 = tags.pop()
        tag_2 = tags.pop()
        tag_3 = tags.pop()
        service.remove_tags_from_file(TEST_FILE_1, {tag_1, tag_2})

        tags = service.get_tags_for_file(TEST_FILE_1)
        assert len(tags) == 1
        assert tag_3 in tags

    def test_remove_tags_emits_tags_changed(self, service: TagService) -> None:
        """remove_tags_from_files emits tags_changed."""
        created_tags_1 = service.add_tags_to_file_by_name(TEST_FILE_1, [TEST_TAG_NAME_1])
        created_tags_2 = service.add_tags_to_files_by_name([TEST_FILE_2, TEST_FILE_3], [TEST_TAG_NAME_2])

        emitted: list[bool] = []
        service.tags_changed.connect(lambda: emitted.append(True))

        service.remove_tags_from_file(TEST_FILE_1, created_tags_1)

        assert len(emitted) == 1, "tags_changed should be emitted once"

        service.remove_tags_from_files([TEST_FILE_2, TEST_FILE_3], created_tags_2)

        assert len(emitted) == 2, "tags_changed should be emitted a second time"

    def test_remove_nonexistent_tag(self, service: TagService) -> None:
        """Removing a tag that doesn't exist does not raise."""
        created_tags = service.add_tags_to_file_by_name(TEST_FILE_1, [TEST_TAG_NAME_1])

        service.remove_tags_from_file(TEST_FILE_1, {Tag(9999, "nonexistent")})
        tags = service.get_tags_for_file(TEST_FILE_1)
        assert len(tags) == 1
        for t in created_tags:
            assert t in tags
