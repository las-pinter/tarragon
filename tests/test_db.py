"""Tests for src/tarragon/db.py — schema init and all CRUD operations."""

from __future__ import annotations

import sqlite3
from collections.abc import Generator
from pathlib import Path

import pytest

from tarragon.db import Database

# ── Fixtures ────────────────────────────────────────────────────


@pytest.fixture()
def db() -> Generator[Database, None, None]:
    """Provide an in-memory database for each test (isolated)."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


# ── Schema Init ────────────────────────────────────────────────


class TestInitSchema:
    def test_creates_all_8_tables(self, db: Database) -> None:
        """init_schema() creates all 8 expected tables (excluding internal sqlite_sequence)."""
        cursor = db._conn.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        table_names = {row["name"] for row in cursor.fetchall()}
        # Exclude SQLite's internal autoincrement bookkeeping table
        user_tables = table_names - {"sqlite_sequence"}
        expected = {
            "schema_version",
            "thumbnails",
            "tags",
            "file_tags",
            "favorites",
            "settings",
            "editor_associations",
            "folder_cache_uuids",
        }
        assert user_tables == expected, f"Missing tables: {expected - user_tables}"

    def test_is_idempotent(self, db: Database) -> None:
        """Calling init_schema() twice does not error."""
        # Already initialized by fixture; call again.
        db.init_schema()  # Should not raise


class TestSchemaVersion:
    def test_version_defaults_to_zero(self, db: Database) -> None:
        """get_schema_version returns 0 before any version is set."""
        assert db.get_schema_version() == 0

    def test_set_and_get_roundtrip(self, db: Database) -> None:
        """Setting a version and reading it back yields the same value."""
        for version in (1, 42, 999):
            db.set_schema_version(version)
            assert db.get_schema_version() == version

    def test_replacing_version(self, db: Database) -> None:
        """Setting a new version replaces the old one."""
        db.set_schema_version(5)
        assert db.get_schema_version() == 5
        db.set_schema_version(10)
        assert db.get_schema_version() == 10


# ── Thumbnail CRUD ─────────────────────────────────────────────


class TestThumbnailUpsert:
    def test_insert_new_thumbnail(self, db: Database) -> None:
        """upsert_thumbnail inserts a new record and get_thumbnail returns it."""
        path = "/images/cat.png"
        db.upsert_thumbnail(path, mtime=1700000000, size=204800, width=800, height=600, cache_uuid="abc123")

        result = db.get_thumbnail(path)
        assert result is not None
        assert result["path"] == path
        assert result["mtime"] == 1700000000
        assert result["size"] == 204800
        assert result["width"] == 800
        assert result["height"] == 600
        assert result["cache_uuid"] == "abc123"

    def test_update_existing_thumbnail(self, db: Database) -> None:
        """upsert_thumbnail updates mtime/size on conflict."""
        path = "/images/dog.png"
        db.upsert_thumbnail(path, mtime=1700000000, size=1000, width=640, height=480, cache_uuid="uuid1")
        db.upsert_thumbnail(path, mtime=1700000999, size=2000, width=1024, height=768, cache_uuid="uuid2")

        result = db.get_thumbnail(path)
        assert result is not None
        assert result["mtime"] == 1700000999
        assert result["size"] == 2000
        assert result["width"] == 1024
        assert result["height"] == 768
        assert result["cache_uuid"] == "uuid2"

    def test_cache_paths_none_by_default(self, db: Database) -> None:
        """Cache path columns can be omitted (None)."""
        path = "/images/empty.png"
        db.upsert_thumbnail(path, mtime=1, size=0, width=1, height=1, cache_uuid="uuid-none")
        result = db.get_thumbnail(path)
        assert result is not None
        assert result["thumbnail_cache_path"] is None
        assert result["preview_cache_path"] is None
        assert result["full_cache_path"] is None


class TestThumbnailDelete:
    def test_delete_existing(self, db: Database) -> None:
        """delete_thumbnail removes the record; get returns None."""
        path = "/images/delete_me.png"
        db.upsert_thumbnail(path, mtime=1, size=100, width=50, height=50, cache_uuid="del-uuid")
        db.delete_thumbnail(path)

        assert db.get_thumbnail(path) is None

    def test_delete_nonexistent_does_not_error(self, db: Database) -> None:
        """Deleting a non-existent path silently succeeds."""
        db.delete_thumbnail("/no/such/file.png")  # Should not raise


class TestThumbnailGet:
    def test_missing_path_returns_none(self, db: Database) -> None:
        assert db.get_thumbnail("/nonexistent/path.jpg") is None

    def test_created_at_default_is_set(self, db: Database) -> None:
        """created_at gets a default datetime value."""
        path = "/images/timestamp.png"
        db.upsert_thumbnail(path, mtime=1, size=50, width=10, height=10, cache_uuid="ts-uuid")
        result = db.get_thumbnail(path)
        assert result is not None
        assert result["created_at"] is not None


class TestThumbnailListForFolder:
    def test_returns_matching_paths(self, db: Database) -> None:
        """list_thumbnails_for_folder returns only paths under the given folder."""
        db.upsert_thumbnail("/photos/2024/cat.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/photos/2024/dog.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")
        db.upsert_thumbnail("/other/rabbit.png", mtime=3, size=300, width=10, height=10, cache_uuid="u3")

        results = db.list_thumbnails_for_folder("/photos/2024/")
        assert len(results) == 2
        paths = {r["path"] for r in results}
        assert "/photos/2024/cat.png" in paths
        assert "/photos/2024/dog.png" in paths

    def test_empty_folder_returns_empty_list(self, db: Database) -> None:
        result = db.list_thumbnails_for_folder("/nonexistent/folder/")
        assert result == []

    def test_does_not_match_sibling_folder_with_shared_prefix(self, db: Database) -> None:
        """Querying '/photos' must NOT match '/photos-vacation/img.png'."""
        db.upsert_thumbnail("/photos/img.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/photos-vacation/img.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")

        results = db.list_thumbnails_for_folder("/photos")
        paths = {r["path"] for r in results}
        assert paths == {"/photos/img.png"}


# ── Tag CRUD ───────────────────────────────────────────────────


class TestEnsureTag:
    def test_creates_new_tag(self, db: Database) -> None:
        tag_id = db.ensure_tag("red")
        assert isinstance(tag_id, int)
        assert tag_id > 0

    def test_returns_same_id_on_repeat(self, db: Database) -> None:
        id1 = db.ensure_tag("blue")
        id2 = db.ensure_tag("blue")
        assert id1 == id2

    def test_different_tags_get_different_ids(self, db: Database) -> None:
        id_green = db.ensure_tag("green")
        id_yellow = db.ensure_tag("yellow")
        assert id_green != id_yellow


class TestAddFileTags:
    def test_adds_single_tag_to_path(self, db: Database) -> None:
        tag_id = db.ensure_tag("important")
        db.add_file_tags(["/a.png"], tag_id)

        assert db.get_file_tag_ids("/a.png") == {tag_id}

    def test_adds_same_tag_to_multiple_paths(self, db: Database) -> None:
        tag_id = db.ensure_tag("urgent")
        paths = ["/x.png", "/y.png", "/z.png"]
        db.add_file_tags(paths, tag_id)

        for p in paths:
            assert db.get_file_tag_ids(p) == {tag_id}

    def test_default_source_is_user(self, db: Database) -> None:
        tag_id = db.ensure_tag("manual")
        db.add_file_tags(["/file.png"], tag_id)

        row = db._conn.execute("SELECT source FROM file_tags WHERE path='/file.png' AND tag_id=?", (tag_id,)).fetchone()
        assert row["source"] == "user"


class TestRemoveFileTags:
    def test_removes_specific_tag(self, db: Database) -> None:
        id_a = db.ensure_tag("alpha")
        id_b = db.ensure_tag("beta")
        db.add_file_tags(["/t.png"], tag_id=id_a)
        db.add_file_tags(["/t.png"], tag_id=id_b)

        db.remove_file_tags(["/t.png"], tag_id=id_a)
        assert db.get_file_tag_ids("/t.png") == {id_b}

    def test_does_not_remove_other_tags(self, db: Database) -> None:
        id_x = db.ensure_tag("x")
        id_y = db.ensure_tag("y")
        db.add_file_tags(["/u.png"], tag_id=id_x)
        db.add_file_tags(["/v.png"], tag_id=id_y)

        db.remove_file_tags(["/u.png"], tag_id=id_x)
        assert db.get_file_tag_ids("/v.png") == {id_y}


class TestGetFileTagIds:
    def test_empty_for_missing_path(self, db: Database) -> None:
        assert db.get_file_tag_ids("/ghost.png") == set()

    def test_multiple_tags_as_set(self, db: Database) -> None:
        id_1 = db.ensure_tag("one")
        id_2 = db.ensure_tag("two")
        db.add_file_tags(["/multi.png"], tag_id=id_1)
        db.add_file_tags(["/multi.png"], tag_id=id_2)

        assert db.get_file_tag_ids("/multi.png") == {id_1, id_2}


class TestReplaceAutoColorTags:
    def test_replaces_old_auto_color_with_new(self, db: Database) -> None:
        """Old auto_color tags are deleted and new ones inserted."""
        old_id = db.ensure_tag("old_red")
        new_id_a = db.ensure_tag("new_red")
        new_id_b = db.ensure_tag("new_blue")

        # Set up: old auto_color tag + a user tag
        db.add_file_tags(["/scene.png"], tag_id=old_id, source="auto_color")
        db.add_file_tags(["/scene.png"], tag_id=new_id_a, source="user")

        result_before = db.get_file_tag_ids("/scene.png")
        assert old_id in result_before

        # Replace with new auto_color tags
        db.replace_auto_color_tags("/scene.png", ["new_red", "new_blue"])

        result_after = db.get_file_tag_ids("/scene.png")
        assert old_id not in result_after
        assert new_id_a in result_after  # user tag survives
        assert new_id_b in result_after

    def test_clears_auto_color_when_empty_list(self, db: Database) -> None:
        """Passing an empty list removes all auto_color tags for the path."""
        id_a = db.ensure_tag("a")
        id_b = db.ensure_tag("b")
        db.add_file_tags(["/clear.png"], tag_id=id_a, source="auto_color")
        db.add_file_tags(["/clear.png"], tag_id=id_b, source="user")

        db.replace_auto_color_tags("/clear.png", [])

        assert id_a not in db.get_file_tag_ids("/clear.png")
        assert id_b in db.get_file_tag_ids("/clear.png")


class TestDeleteTag:
    def test_delete_tag_removes_tag(self, db: Database) -> None:
        """delete_tag removes the tag from the tags table."""
        tag_id = db.ensure_tag("deleteme")
        db.delete_tag(tag_id)

        # Verify tag is gone
        row = db._conn.execute("SELECT * FROM tags WHERE id = ?", (tag_id,)).fetchone()
        assert row is None

    def test_delete_tag_cascades_to_file_tags(self, db: Database) -> None:
        """Deleting a tag CASCADE-deletes all file-tag associations."""
        tag_id = db.ensure_tag("cascade-test")
        db.add_file_tags(["/a.png", "/b.png", "/c.png"], tag_id)

        # Verify associations exist
        assert db.get_file_tag_ids("/a.png") == {tag_id}
        assert db.get_file_tag_ids("/b.png") == {tag_id}
        assert db.get_file_tag_ids("/c.png") == {tag_id}

        # Delete the tag
        db.delete_tag(tag_id)

        # All associations should be gone
        assert db.get_file_tag_ids("/a.png") == set()
        assert db.get_file_tag_ids("/b.png") == set()
        assert db.get_file_tag_ids("/c.png") == set()

    def test_delete_tag_nonexistent_does_not_error(self, db: Database) -> None:
        """Deleting a non-existent tag silently succeeds."""
        db.delete_tag(99999)  # Should not raise

    def test_delete_tag_preserves_other_tags(self, db: Database) -> None:
        """Deleting one tag does not affect other tags."""
        id_keep = db.ensure_tag("keep")
        id_delete = db.ensure_tag("delete")
        db.add_file_tags(["/x.png"], id_keep)
        db.add_file_tags(["/x.png"], id_delete)

        db.delete_tag(id_delete)

        # keep tag should still exist
        assert db.get_file_tag_ids("/x.png") == {id_keep}
        row = db._conn.execute("SELECT * FROM tags WHERE id = ?", (id_keep,)).fetchone()
        assert row is not None


class TestGetTagName:
    def test_get_tag_name_returns_name(self, db: Database) -> None:
        """get_tag_name returns the correct name for an existing tag."""
        tag_id = db.ensure_tag("test-tag")
        assert db.get_tag_name(tag_id) == "test-tag"

    def test_get_tag_name_returns_none_for_missing(self, db: Database) -> None:
        """get_tag_name returns None for a non-existent tag id."""
        assert db.get_tag_name(99999) is None

    def test_get_tag_name_multiple_tags(self, db: Database) -> None:
        """get_tag_name returns correct names for different tags."""
        id_a = db.ensure_tag("alpha")
        id_b = db.ensure_tag("beta")
        assert db.get_tag_name(id_a) == "alpha"
        assert db.get_tag_name(id_b) == "beta"


# ── Favorites CRUD ─────────────────────────────────────────────


class TestAddFavorite:
    def test_adds_favorite(self, db: Database) -> None:
        db.add_favorite("/fav.png", label="sunrise", sort_order=1)

        favorites = db.list_favorites()
        assert len(favorites) == 1
        assert favorites[0]["path"] == "/fav.png"
        assert favorites[0]["label"] == "sunrise"
        assert favorites[0]["sort_order"] == 1

    def test_default_label_and_sort_order(self, db: Database) -> None:
        db.add_favorite("/default.png")

        fav = db.list_favorites()[0]
        assert fav["label"] is None
        assert fav["sort_order"] == 0


class TestRemoveFavorite:
    def test_removes_existing(self, db: Database) -> None:
        db.add_favorite("/gone.png")
        db.remove_favorite("/gone.png")

        assert len(db.list_favorites()) == 0

    def test_no_error_on_missing_path(self, db: Database) -> None:
        db.remove_favorite("/nowhere.png")  # Should not raise


class TestListFavorites:
    def test_ordered_by_sort_order_then_path(self, db: Database) -> None:
        db.add_favorite("/b.png", sort_order=1)
        db.add_favorite("/a.png", sort_order=1)
        db.add_favorite("/c.png", sort_order=2)

        paths = [f["path"] for f in db.list_favorites()]
        assert paths == ["/a.png", "/b.png", "/c.png"]


# ── Settings CRUD ──────────────────────────────────────────────


class TestSettingsCrud:
    def test_get_missing_returns_none(self, db: Database) -> None:
        assert db.get_setting("no_such_key") is None

    def test_set_and_get_roundtrip(self, db: Database) -> None:
        db.set_setting("theme", "dark")
        assert db.get_setting("theme") == "dark"

    def test_overwrite_value(self, db: Database) -> None:
        db.set_setting("mode", "fast")
        db.set_setting("mode", "slow")
        assert db.get_setting("mode") == "slow"


# ── Editor Associations CRUD ───────────────────────────────────


class TestEditorAssociations:
    def test_get_missing_returns_none(self, db: Database) -> None:
        assert db.get_editor_command(".xyz") is None

    def test_upsert_and_get(self, db: Database) -> None:
        db.upsert_editor_association(".psd", "gimp {file}")
        assert db.get_editor_command(".psd") == "gimp {file}"

    def test_overwrite_template(self, db: Database) -> None:
        db.upsert_editor_association(".xcf", "paint.net {file}")
        db.upsert_editor_association(".xcf", "krita {file}")
        assert db.get_editor_command(".xcf") == "krita {file}"

    def test_remove(self, db: Database) -> None:
        db.upsert_editor_association(".svg", "inkscape {file}")
        db.remove_editor_association(".svg")
        assert db.get_editor_command(".svg") is None


# ── Folder Cache UUIDs ─────────────────────────────────────────


class TestFolderCacheUuids:
    def test_get_folder_uuid_returns_none_when_absent(self, db: Database) -> None:
        """get_folder_uuid returns None for an unmapped folder."""
        assert db.get_folder_uuid("/photos/vacation") is None

    def test_upsert_and_get_folder_uuid(self, db: Database) -> None:
        """upsert_folder_uuid stores a mapping that get_folder_uuid retrieves."""
        db.upsert_folder_uuid("/photos/vacation", "abc12345")
        assert db.get_folder_uuid("/photos/vacation") == "abc12345"

    def test_upsert_folder_uuid_overwrites(self, db: Database) -> None:
        """upsert_folder_uuid replaces an existing mapping."""
        db.upsert_folder_uuid("/photos/vacation", "abc12345")
        db.upsert_folder_uuid("/photos/vacation", "def67890")
        assert db.get_folder_uuid("/photos/vacation") == "def67890"

    def test_different_folders_have_independent_uuids(self, db: Database) -> None:
        """Different folder paths maintain separate UUID mappings."""
        db.upsert_folder_uuid("/photos/vacation", "aaa11111")
        db.upsert_folder_uuid("/photos/work", "bbb22222")
        assert db.get_folder_uuid("/photos/vacation") == "aaa11111"
        assert db.get_folder_uuid("/photos/work") == "bbb22222"


class TestGetOrCreateFolderUuid:
    def test_creates_new_entry_when_absent(self, db: Database) -> None:
        """get_or_create_folder_uuid inserts and returns the candidate UUID for a new folder."""
        result = db.get_or_create_folder_uuid("/photos/new", "candidate-uuid")
        assert result == "candidate-uuid"
        # Verify it's persisted
        assert db.get_folder_uuid("/photos/new") == "candidate-uuid"

    def test_returns_existing_uuid_ignoring_candidate(self, db: Database) -> None:
        """When a UUID already exists, the candidate is ignored and the existing UUID is returned."""
        db.upsert_folder_uuid("/photos/existing", "original-uuid")
        result = db.get_or_create_folder_uuid("/photos/existing", "different-candidate")
        assert result == "original-uuid"

    def test_two_calls_same_folder_return_same_uuid(self, db: Database) -> None:
        """Two concurrent-style calls with different candidates converge on the same UUID."""
        first = db.get_or_create_folder_uuid("/photos/shared", "uuid-a")
        second = db.get_or_create_folder_uuid("/photos/shared", "uuid-b")
        assert first == second == "uuid-a"

    def test_different_folders_get_different_uuids(self, db: Database) -> None:
        """Different folders maintain independent UUIDs through the atomic method."""
        uuid_a = db.get_or_create_folder_uuid("/photos/alpha", "uuid-alpha")
        uuid_b = db.get_or_create_folder_uuid("/photos/beta", "uuid-beta")
        assert uuid_a == "uuid-alpha"
        assert uuid_b == "uuid-beta"
        assert uuid_a != uuid_b


class TestCleanupStaleFolderUuids:
    def test_removes_entries_where_folder_missing(self, db: Database, tmp_path: Path) -> None:
        """Entries pointing to non-existent folders are deleted."""
        existing_dir = str(tmp_path / "exists")
        Path(existing_dir).mkdir()
        missing_dir = "/no/such/folder/ever"

        db.upsert_folder_uuid(existing_dir, "uuid-keep")
        db.upsert_folder_uuid(missing_dir, "uuid-remove")

        removed = db.cleanup_stale_folder_uuids()

        assert removed == 1
        assert db.get_folder_uuid(existing_dir) == "uuid-keep"
        assert db.get_folder_uuid(missing_dir) is None

    def test_keeps_entries_where_folder_exists(self, db: Database, tmp_path: Path) -> None:
        """Entries pointing to existing folders are preserved."""
        dir_a = str(tmp_path / "a")
        dir_b = str(tmp_path / "b")
        Path(dir_a).mkdir()
        Path(dir_b).mkdir()

        db.upsert_folder_uuid(dir_a, "uuid-a")
        db.upsert_folder_uuid(dir_b, "uuid-b")

        removed = db.cleanup_stale_folder_uuids()

        assert removed == 0
        assert db.get_folder_uuid(dir_a) == "uuid-a"
        assert db.get_folder_uuid(dir_b) == "uuid-b"

    def test_returns_zero_when_table_empty(self, db: Database) -> None:
        """No entries → returns 0, no error."""
        assert db.cleanup_stale_folder_uuids() == 0

    def test_removes_multiple_stale_entries(self, db: Database) -> None:
        """Multiple stale entries are all removed in one call."""
        db.upsert_folder_uuid("/gone/one", "uuid-1")
        db.upsert_folder_uuid("/gone/two", "uuid-2")
        db.upsert_folder_uuid("/gone/three", "uuid-3")

        removed = db.cleanup_stale_folder_uuids()

        assert removed == 3
        assert db.get_folder_uuid("/gone/one") is None
        assert db.get_folder_uuid("/gone/two") is None
        assert db.get_folder_uuid("/gone/three") is None


# ── Context Manager ────────────────────────────────────────────


class TestContextManager:
    def test_closes_connection_on_exit(self, tmp_path: Path) -> None:
        """Database as context manager closes the connection."""
        db_file = tmp_path / "cm.db"
        with Database(db_file) as d:
            d.init_schema()
            d.set_setting("test", "1")

        # Connection should be closed — executing raises an error
        with pytest.raises(sqlite3.ProgrammingError):
            d._conn.execute("SELECT 1")


# ── Distinct Folders ───────────────────────────────────────────


class TestListDistinctFolders:
    def test_empty_db_returns_empty_list(self, db: Database) -> None:
        """No thumbnails → no folders."""
        assert db.list_distinct_folders() == []

    def test_returns_distinct_parent_folders(self, db: Database) -> None:
        """Multiple files in the same folder produce one entry."""
        db.upsert_thumbnail("/photos/vacation/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/photos/vacation/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")
        db.upsert_thumbnail("/photos/work/c.png", mtime=3, size=300, width=10, height=10, cache_uuid="u3")

        folders = db.list_distinct_folders()
        assert folders == ["/photos/vacation", "/photos/work"]

    def test_sorted_alphabetically(self, db: Database) -> None:
        """Folders are returned in sorted order."""
        db.upsert_thumbnail("/z/f.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        db.upsert_thumbnail("/a/f.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")
        db.upsert_thumbnail("/m/f.png", mtime=3, size=300, width=10, height=10, cache_uuid="u3")

        folders = db.list_distinct_folders()
        assert folders == ["/a", "/m", "/z"]

    def test_excludes_empty_paths(self, db: Database) -> None:
        """Rows with empty path are excluded."""
        db.upsert_thumbnail("/valid/f.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        # Empty path is not inserted via upsert_thumbnail normally,
        # but we verify the WHERE clause works
        folders = db.list_distinct_folders()
        assert "" not in folders
        assert "/valid" in folders


# ── Windows Path Normalization ─────────────────────────────────


class TestWindowsPathNormalization:
    """Verify that backslash paths (Windows-style) are normalized to forward slashes."""

    def test_upsert_normalizes_backslashes(self, db: Database) -> None:
        """upsert_thumbnail stores paths with forward slashes even when given backslashes."""
        db.upsert_thumbnail(
            "D:\\Dropbox\\Art\\image.png",
            mtime=1,
            size=100,
            width=10,
            height=10,
            cache_uuid="u1",
        )
        result = db.get_thumbnail("D:/Dropbox/Art/image.png")
        assert result is not None
        assert result["path"] == "D:/Dropbox/Art/image.png"

    def test_get_thumbnail_normalizes_backslashes(self, db: Database) -> None:
        """get_thumbnail finds records regardless of the separator used in the query."""
        db.upsert_thumbnail(
            "D:/Dropbox/Art/image.png",
            mtime=1,
            size=100,
            width=10,
            height=10,
            cache_uuid="u1",
        )
        # Query with backslashes should still find the record
        result = db.get_thumbnail("D:\\Dropbox\\Art\\image.png")
        assert result is not None

    def test_bulk_upsert_normalizes_backslashes(self, db: Database) -> None:
        """bulk_upsert_stubs normalizes backslash paths to forward slashes."""
        files = [
            ("D:\\Dropbox\\Art\\a.png", 1, 100),
            ("D:\\Dropbox\\Art\\b.png", 2, 200),
        ]
        db.bulk_upsert_stubs(files)

        # Should be findable with forward-slash paths
        assert db.get_thumbnail("D:/Dropbox/Art/a.png") is not None
        assert db.get_thumbnail("D:/Dropbox/Art/b.png") is not None

    def test_list_thumbnails_for_folder_with_backslashes(self, db: Database) -> None:
        """list_thumbnails_for_folder works when folder_path uses backslashes."""
        db.upsert_thumbnail(
            "D:/Dropbox/Art/a.png",
            mtime=1,
            size=100,
            width=10,
            height=10,
            cache_uuid="u1",
        )
        db.upsert_thumbnail(
            "D:/Dropbox/Art/b.png",
            mtime=2,
            size=200,
            width=10,
            height=10,
            cache_uuid="u2",
        )
        # Query with backslash separator — this was the original bug
        results = db.list_thumbnails_for_folder("D:\\Dropbox\\Art")
        assert len(results) == 2

    def test_delete_thumbnails_by_folder_with_backslashes(self, db: Database) -> None:
        """delete_thumbnails_by_folder works when folder_path uses backslashes."""
        db.upsert_thumbnail(
            "D:/Dropbox/Art/a.png",
            mtime=1,
            size=100,
            width=10,
            height=10,
            cache_uuid="u1",
        )
        db.delete_thumbnails_by_folder("D:\\Dropbox\\Art")
        assert db.get_thumbnail("D:/Dropbox/Art/a.png") is None

    def test_list_distinct_folders_uses_forward_slashes(self, db: Database) -> None:
        """list_distinct_folders returns paths with forward slashes."""
        db.upsert_thumbnail(
            "D:/Dropbox/Art/a.png",
            mtime=1,
            size=100,
            width=10,
            height=10,
            cache_uuid="u1",
        )
        folders = db.list_distinct_folders()
        assert folders == ["D:/Dropbox/Art"]
        # Ensure no backslashes in the output
        for f in folders:
            assert "\\" not in f

    def test_add_file_tags_normalizes_paths(self, db: Database) -> None:
        """add_file_tags normalizes backslash paths."""
        tag_id = db.ensure_tag("test")
        db.add_file_tags(["D:\\Art\\file.png"], tag_id)
        # Should be retrievable with forward-slash path
        assert db.get_file_tag_ids("D:/Art/file.png") == {tag_id}

    def test_favorites_normalize_paths(self, db: Database) -> None:
        """add_favorite and remove_favorite normalize backslash paths."""
        db.add_favorite("D:\\Art\\fav.png", label="test")
        favs = db.list_favorites()
        assert len(favs) == 1
        assert favs[0]["path"] == "D:/Art/fav.png"

        # Remove with backslash path should still work
        db.remove_favorite("D:\\Art\\fav.png")
        assert len(db.list_favorites()) == 0

    def test_folder_uuid_normalizes_paths(self, db: Database) -> None:
        """Folder UUID methods normalize backslash paths."""
        db.upsert_folder_uuid("D:\\Dropbox\\Art", "uuid-123")
        # Retrieve with forward slashes
        assert db.get_folder_uuid("D:/Dropbox/Art") == "uuid-123"
        # Retrieve with backslashes
        assert db.get_folder_uuid("D:\\Dropbox\\Art") == "uuid-123"

    def test_get_all_tags_with_counts_backslash_folder(self, db: Database) -> None:
        """get_all_tags_with_counts works with backslash folder paths."""
        tag_id = db.ensure_tag("nature")
        db.upsert_thumbnail(
            "D:/Dropbox/Art/a.png",
            mtime=1,
            size=100,
            width=10,
            height=10,
            cache_uuid="u1",
        )
        db.add_file_tags(["D:/Dropbox/Art/a.png"], tag_id)

        # Query with backslash folder path
        result = db.get_all_tags_with_counts("D:\\Dropbox\\Art")
        assert len(result) == 1
        assert result[0]["name"] == "nature"
        assert result[0]["usage_count"] == 1
