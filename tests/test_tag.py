"""Unit tests for the Tag class and build_tag_from_dict helper."""

from __future__ import annotations

from typing import Any, cast

import pytest

from tarragon.db.common.tag import Tag, TagSource, build_tag_from_dict

TEST_TAG_ID_1 = 1
TEST_TAG_ID_2 = 2
TEST_TAG_ID_3 = 3
TEST_TAG_NAME_1 = "test tag 1"
TEST_TAG_NAME_2 = "test tag 2"
TEST_TAG_NAME_3 = "test tag 3"
TEST_FILE_PATH_1 = "/test_folder_1/test_file_1.png"
TEST_FILE_PATH_2 = "/test_folder_2/test_file_2.png"


@pytest.fixture
def tag() -> Tag:
    """Return a tag populated with every field."""
    return Tag(
        id=TEST_TAG_ID_1,
        name=TEST_TAG_NAME_1,
        source=TagSource.USER,
        usage_count=2,
        usage_paths={TEST_FILE_PATH_1, TEST_FILE_PATH_2},
    )


@pytest.fixture
def tag_with_defaults() -> Tag:
    """Return a tag constructed with only an id and a name."""
    return Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1)


class TestTagConstruction:
    """Constructing tags with various field combinations."""

    def test_construct_with_defaults(self, tag_with_defaults: Tag) -> None:
        """Defaults for optional fields are None."""
        assert tag_with_defaults.get_source() is None
        assert tag_with_defaults.get_usage_count() is None
        assert tag_with_defaults.get_usage_paths() is None

    def test_construct_with_all_fields(self, tag: Tag) -> None:
        """All provided fields are stored and returned."""
        assert tag.get_id() == TEST_TAG_ID_1
        assert tag.get_name() == TEST_TAG_NAME_1
        assert tag.get_source() == TagSource.USER
        assert tag.get_usage_count() == 2
        assert tag.get_usage_paths() == {TEST_FILE_PATH_1, TEST_FILE_PATH_2}

    def test_construct_accepts_zero_id(self) -> None:
        """A zero id is stored without validation."""
        assert Tag(id=0, name=TEST_TAG_NAME_1).get_id() == 0

    def test_construct_accepts_empty_name(self) -> None:
        """An empty string name is stored without validation."""
        assert Tag(id=TEST_TAG_ID_1, name="").get_name() == ""

    def test_construct_accepts_empty_usage_paths(self) -> None:
        """An empty usage paths set is stored as given."""
        empty_paths: set[str] = set()
        assert Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1, usage_paths=empty_paths).get_usage_paths() == empty_paths

    def test_construct_keeps_usage_paths_reference(self) -> None:
        """The provided usage paths set is stored without copying."""
        paths = {TEST_FILE_PATH_1}
        constructed = Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1, usage_paths=paths)
        assert constructed.get_usage_paths() is paths


class TestTagGetters:
    """Accessing tag fields through getters."""

    def test_get_id_returns_constructed_id(self, tag: Tag) -> None:
        """get_id returns the id passed to the constructor."""
        assert tag.get_id() == TEST_TAG_ID_1

    def test_get_name_returns_constructed_name(self, tag: Tag) -> None:
        """get_name returns the name passed to the constructor."""
        assert tag.get_name() == TEST_TAG_NAME_1

    def test_get_source_returns_constructed_source(self, tag: Tag) -> None:
        """get_source returns the source passed to the constructor."""
        assert tag.get_source() == TagSource.USER

    def test_get_source_returns_none_when_not_set(self, tag_with_defaults: Tag) -> None:
        """get_source returns None when no source was provided."""
        assert tag_with_defaults.get_source() is None

    def test_get_usage_count_returns_constructed_count(self, tag: Tag) -> None:
        """get_usage_count returns the count passed to the constructor."""
        assert tag.get_usage_count() == 2

    def test_get_usage_paths_returns_constructed_paths(self, tag: Tag) -> None:
        """get_usage_paths returns the paths passed to the constructor."""
        assert tag.get_usage_paths() == {TEST_FILE_PATH_1, TEST_FILE_PATH_2}


class TestTagSetters:
    """Updating mutable tag fields."""

    def test_set_source_updates_source(self, tag: Tag) -> None:
        """set_source replaces the stored source."""
        tag.set_source(TagSource.AUTO_COLOR)
        assert tag.get_source() == TagSource.AUTO_COLOR

    def test_set_usage_count_updates_usage_count(self, tag: Tag) -> None:
        """set_usage_count replaces the stored usage count."""
        tag.set_usage_count(5)
        assert tag.get_usage_count() == 5


class TestTagEquality:
    """Equality semantics between tags."""

    def test_tag_equals_itself(self, tag: Tag) -> None:
        """A tag compares equal to itself."""
        assert tag == tag

    def test_equal_tags_are_distinct_objects(self) -> None:
        """Equal tags can be distinct objects."""
        first = Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1)
        second = Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1)
        assert first is not second
        assert first == second

    def test_not_equal_when_id_differs(self, tag: Tag) -> None:
        """Tags with different ids are not equal."""
        other = Tag(
            id=TEST_TAG_ID_2,
            name=tag.get_name(),
            source=tag.get_source(),
            usage_count=tag.get_usage_count(),
            usage_paths=tag.get_usage_paths(),
        )
        assert tag != other

    def test_not_equal_when_name_differs(self, tag: Tag) -> None:
        """Tags with different names are not equal."""
        other = Tag(
            id=tag.get_id(),
            name=TEST_TAG_NAME_2,
            source=tag.get_source(),
            usage_count=tag.get_usage_count(),
            usage_paths=tag.get_usage_paths(),
        )
        assert tag != other

    def test_not_equal_when_source_differs(self, tag: Tag) -> None:
        """Tags with different sources are not equal."""
        other = Tag(
            id=tag.get_id(),
            name=tag.get_name(),
            source=TagSource.AUTO_COLOR,
            usage_count=tag.get_usage_count(),
            usage_paths=tag.get_usage_paths(),
        )
        assert tag != other

    def test_not_equal_when_usage_count_differs(self, tag: Tag) -> None:
        """Tags with different usage counts are not equal."""
        other = Tag(
            id=tag.get_id(),
            name=tag.get_name(),
            source=tag.get_source(),
            usage_count=3,
            usage_paths=tag.get_usage_paths(),
        )
        assert tag != other

    def test_not_equal_when_usage_paths_differs(self, tag: Tag) -> None:
        """Tags with different usage paths are not equal."""
        other = Tag(
            id=tag.get_id(),
            name=tag.get_name(),
            source=tag.get_source(),
            usage_count=tag.get_usage_count(),
            usage_paths={TEST_FILE_PATH_1},
        )
        assert tag != other

    def test_equal_ignores_usage_paths_order(self) -> None:
        """Tags are equal when usage paths differ only in order."""
        first = Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1, usage_paths={TEST_FILE_PATH_1, TEST_FILE_PATH_2})
        second = Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1, usage_paths={TEST_FILE_PATH_2, TEST_FILE_PATH_1})
        assert first == second


class TestTagHashing:
    """Hash behavior for use in sets and dicts."""

    def test_equal_tags_have_equal_hashes(self) -> None:
        """Equal tags produce the same hash."""
        first = Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1)
        second = Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1)
        assert hash(first) == hash(second)

    def test_hash_is_deterministic(self, tag: Tag) -> None:
        """Hashing the same tag twice gives the same result."""
        assert hash(tag) == hash(tag)

    def test_tag_usable_as_dict_key(self, tag: Tag) -> None:
        """A tag can be stored in a dict and retrieved by an equal tag."""
        tags: dict[Tag, str] = {tag: "value"}
        equal_tag = Tag(
            id=tag.get_id(),
            name=tag.get_name(),
            source=tag.get_source(),
            usage_count=tag.get_usage_count(),
            usage_paths=tag.get_usage_paths(),
        )
        assert tags[equal_tag] == "value"

    def test_tag_usable_as_set_member(self, tag: Tag) -> None:
        """A tag can be stored in a set and deduplicated by equality."""
        equal_tag = Tag(
            id=tag.get_id(),
            name=tag.get_name(),
            source=tag.get_source(),
            usage_count=tag.get_usage_count(),
            usage_paths=tag.get_usage_paths(),
        )
        assert len({tag, equal_tag}) == 1

    def test_hash_changes_after_mutation(self) -> None:
        """Mutating a tag changes its hash because hash is based on repr."""
        mutable_tag = Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1)
        original_hash = hash(mutable_tag)
        mutable_tag.set_usage_count(3)
        assert hash(mutable_tag) != original_hash


class TestTagOrdering:
    """Ordering comparisons by tag name."""

    def test_less_than_compares_by_name(self) -> None:
        """A tag is less than another when its name sorts earlier."""
        assert Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1) < Tag(id=TEST_TAG_ID_2, name=TEST_TAG_NAME_2)

    def test_greater_than_compares_by_name(self) -> None:
        """A tag is greater than another when its name sorts later."""
        assert Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_2) > Tag(id=TEST_TAG_ID_2, name=TEST_TAG_NAME_1)

    def test_less_than_false_when_names_equal(self) -> None:
        """A tag is not less than another with the same name."""
        assert not (Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1) < Tag(id=TEST_TAG_ID_2, name=TEST_TAG_NAME_1))

    def test_greater_than_false_when_names_equal(self) -> None:
        """A tag is not greater than another with the same name."""
        assert not (Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1) > Tag(id=TEST_TAG_ID_2, name=TEST_TAG_NAME_1))

    def test_ordering_ignores_id(self) -> None:
        """Ordering depends only on the name, not the id."""
        assert not (Tag(id=TEST_TAG_ID_2, name=TEST_TAG_NAME_1) < Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1))

    def test_sorting_orders_by_name(self) -> None:
        """Sorting tags orders them by name."""
        tag_1 = Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1)
        tag_2 = Tag(id=TEST_TAG_ID_2, name=TEST_TAG_NAME_2)
        tag_3 = Tag(id=TEST_TAG_ID_3, name=TEST_TAG_NAME_3)
        assert sorted([tag_2, tag_1, tag_3]) == [tag_1, tag_2, tag_3]

    def test_less_than_non_tag_raises_attribute_error(self) -> None:
        """Comparing with a non-tag raises AttributeError."""
        tag = Tag(id=TEST_TAG_ID_1, name=TEST_TAG_NAME_1)
        with pytest.raises(AttributeError):
            _ = tag < cast(Any, TEST_TAG_NAME_2)


class TestTagRepr:
    """String representation of tags."""

    def test_repr_contains_all_fields(self, tag: Tag) -> None:
        """repr includes every field value."""
        text = repr(tag)
        assert str(tag.get_id()) in text
        assert tag.get_name() in text
        assert str(tag.get_source()) in text
        assert str(tag.get_usage_count()) in text
        assert str(tag.get_usage_paths()) in text

    def test_repr_has_exact_format(self) -> None:
        """repr matches the expected positional format."""
        single = Tag(
            id=TEST_TAG_ID_1,
            name="test",
            source=TagSource.USER,
            usage_count=3,
            usage_paths={TEST_FILE_PATH_1},
        )
        assert repr(single) == "Tag(1, test, user, 3, {'/test_folder_1/test_file_1.png'})"

    def test_str_matches_repr(self, tag: Tag) -> None:
        """str falls back to repr because no __str__ is defined."""
        assert str(tag) == repr(tag)


class TestBuildTagFromDict:
    """Building tags from dictionary rows."""

    def test_build_returns_tag_with_all_fields(self) -> None:
        """All dictionary fields are carried into the tag."""
        tag = build_tag_from_dict(
            {
                "id": TEST_TAG_ID_1,
                "name": TEST_TAG_NAME_1,
                "source": TagSource.USER,
                "usage_count": 2,
                "usage_paths": f"{TEST_FILE_PATH_1},{TEST_FILE_PATH_2}",
            }
        )
        assert tag is not None
        assert tag == Tag(
            id=TEST_TAG_ID_1,
            name=TEST_TAG_NAME_1,
            source=TagSource.USER,
            usage_count=2,
            usage_paths={TEST_FILE_PATH_1, TEST_FILE_PATH_2},
        )

    def test_build_returns_none_when_id_missing(self) -> None:
        """A dict without an id produces None."""
        assert build_tag_from_dict({"name": TEST_TAG_NAME_1}) is None
        assert build_tag_from_dict({}) is None

    def test_build_returns_none_when_name_missing(self) -> None:
        """A dict without a name produces None."""
        assert build_tag_from_dict({"id": TEST_TAG_ID_1}) is None

    def test_build_returns_none_when_id_empty(self) -> None:
        """A falsy id produces None."""
        assert build_tag_from_dict({"id": 0, "name": TEST_TAG_NAME_1}) is None
        assert build_tag_from_dict({"id": "", "name": TEST_TAG_NAME_1}) is None

    def test_build_returns_none_when_name_empty(self) -> None:
        """An empty name produces None."""
        assert build_tag_from_dict({"id": TEST_TAG_ID_1, "name": ""}) is None

    def test_build_defaults_optional_fields(self) -> None:
        """Missing optional fields default to None."""
        tag = build_tag_from_dict({"id": TEST_TAG_ID_1, "name": TEST_TAG_NAME_1})
        assert tag is not None
        assert tag.get_source() is None
        assert tag.get_usage_count() is None
        assert tag.get_usage_paths() is None

    def test_build_passes_zero_usage_count(self) -> None:
        """A zero usage count is carried through."""
        tag = build_tag_from_dict({"id": TEST_TAG_ID_1, "name": TEST_TAG_NAME_1, "usage_count": 0})
        assert tag is not None
        assert tag.get_usage_count() == 0

    def test_build_splits_usage_paths_on_comma(self) -> None:
        """A comma separated usage paths string becomes a set of paths."""
        tag = build_tag_from_dict(
            {"id": TEST_TAG_ID_1, "name": TEST_TAG_NAME_1, "usage_paths": f"{TEST_FILE_PATH_1},{TEST_FILE_PATH_2}"}
        )
        assert tag is not None
        assert tag.get_usage_paths() == {TEST_FILE_PATH_1, TEST_FILE_PATH_2}

    def test_build_single_usage_path(self) -> None:
        """A single usage path becomes a one element set."""
        tag = build_tag_from_dict({"id": TEST_TAG_ID_1, "name": TEST_TAG_NAME_1, "usage_paths": TEST_FILE_PATH_1})
        assert tag is not None
        assert tag.get_usage_paths() == {TEST_FILE_PATH_1}

    def test_build_none_usage_paths(self) -> None:
        """A missing usage paths key leaves usage paths as None."""
        tag = build_tag_from_dict({"id": TEST_TAG_ID_1, "name": TEST_TAG_NAME_1})
        assert tag is not None
        assert tag.get_usage_paths() is None

    def test_build_empty_usage_paths_string(self) -> None:
        """An empty usage paths string leaves usage paths as None."""
        tag = build_tag_from_dict({"id": TEST_TAG_ID_1, "name": TEST_TAG_NAME_1, "usage_paths": ""})
        assert tag is not None
        assert tag.get_usage_paths() is None

    def test_build_keeps_empty_segments_in_usage_paths(self) -> None:
        """Empty comma separated segments are kept in the usage paths set."""
        tag = build_tag_from_dict(
            {"id": TEST_TAG_ID_1, "name": TEST_TAG_NAME_1, "usage_paths": f"{TEST_FILE_PATH_1},,{TEST_FILE_PATH_2}"}
        )
        assert tag is not None
        assert tag.get_usage_paths() == {"", TEST_FILE_PATH_1, TEST_FILE_PATH_2}

    def test_build_passes_string_source_through(self) -> None:
        """A plain string source is stored without converting to TagSource."""
        tag = build_tag_from_dict({"id": TEST_TAG_ID_1, "name": TEST_TAG_NAME_1, "source": "user"})
        assert tag is not None
        assert tag.get_source() == "user"
        assert not isinstance(tag.get_source(), TagSource)

    def test_build_passes_enum_source_through(self) -> None:
        """A TagSource enum source is stored as given."""
        tag = build_tag_from_dict({"id": TEST_TAG_ID_1, "name": TEST_TAG_NAME_1, "source": TagSource.AUTO_COLOR})
        assert tag is not None
        assert tag.get_source() == TagSource.AUTO_COLOR

    def test_build_passes_string_id_through(self) -> None:
        """A string id is stored without converting to int."""
        tag = build_tag_from_dict({"id": "7", "name": TEST_TAG_NAME_1})
        assert tag is not None
        assert tag.get_id() == cast(Any, "7")
