"""Tests for src/tarragon/migrations.py — schema migration runner."""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest
from tarragon.db import Database
from tarragon.migrations import MigrationRunner, _migrate_v1_to_v2


# ── Fixtures ────────────────────────────────────────────────────


@pytest.fixture()
def db() -> Generator[Database, None, None]:
    """Provide an in-memory database for each test (isolated)."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


# ── MigrationRunner ─────────────────────────────────────────────


class TestMigrationRunnerBootstrap:
    def test_bootstrap_sets_version_to_2(self, db: Database) -> None:
        """Fresh database (version 0) is bootstrapped to current version 2."""
        runner = MigrationRunner(db)
        version = runner.run()
        assert version == 2

    def test_already_at_v2_is_noop(self, db: Database) -> None:
        """Running migrations on an already-migrated database is a no-op."""
        db.set_schema_version(2)
        runner = MigrationRunner(db)
        version = runner.run()
        assert version == 2


# ── v1 → v2: Path normalization ────────────────────────────────


class TestMigrateV1ToV2:
    def _insert_backslash_records(self, db: Database) -> None:
        """Insert records with backslash paths directly via SQL (bypassing normalization)."""
        db._execute(
            "INSERT INTO thumbnails (path, mtime, size, width, height, cache_uuid) VALUES (?, ?, ?, ?, ?, ?)",
            ("D:\\Dropbox\\Art\\image.jpg", 1, 100, 800, 600, "u1"),
        )
        db._execute(
            "INSERT INTO thumbnails (path, mtime, size, width, height, cache_uuid) VALUES (?, ?, ?, ?, ?, ?)",
            ("D:\\Dropbox\\Art\\photo.png", 2, 200, 1024, 768, "u2"),
        )
        tag_id = db.ensure_tag("landscape")
        db._execute(
            "INSERT INTO file_tags (path, tag_id, source) VALUES (?, ?, ?)",
            ("D:\\Dropbox\\Art\\image.jpg", tag_id, "user"),
        )
        db._execute(
            "INSERT INTO favorites (path, label, sort_order) VALUES (?, ?, ?)",
            ("D:\\Dropbox\\Art\\image.jpg", "sunset", 1),
        )
        db._execute(
            "INSERT INTO folder_cache_uuids (folder_path, cache_uuid) VALUES (?, ?)",
            ("D:\\Dropbox\\Art", "cache-uuid-123"),
        )
        db._commit()

    def test_normalizes_thumbnail_paths(self, db: Database) -> None:
        """Backslash paths in thumbnails are converted to forward slashes."""
        self._insert_backslash_records(db)
        _migrate_v1_to_v2(db)

        rows = db.fetch_all("SELECT path FROM thumbnails ORDER BY path")
        paths = [r["path"] for r in rows]
        assert paths == ["D:/Dropbox/Art/image.jpg", "D:/Dropbox/Art/photo.png"]

    def test_normalizes_file_tag_paths(self, db: Database) -> None:
        """Backslash paths in file_tags are converted to forward slashes."""
        self._insert_backslash_records(db)
        _migrate_v1_to_v2(db)

        rows = db.fetch_all("SELECT path FROM file_tags")
        assert len(rows) == 1
        assert rows[0]["path"] == "D:/Dropbox/Art/image.jpg"

    def test_normalizes_favorite_paths(self, db: Database) -> None:
        """Backslash paths in favorites are converted to forward slashes."""
        self._insert_backslash_records(db)
        _migrate_v1_to_v2(db)

        rows = db.fetch_all("SELECT path FROM favorites")
        assert len(rows) == 1
        assert rows[0]["path"] == "D:/Dropbox/Art/image.jpg"

    def test_normalizes_folder_cache_paths(self, db: Database) -> None:
        """Backslash paths in folder_cache_uuids are converted to forward slashes."""
        self._insert_backslash_records(db)
        _migrate_v1_to_v2(db)

        rows = db.fetch_all("SELECT folder_path FROM folder_cache_uuids")
        assert len(rows) == 1
        assert rows[0]["folder_path"] == "D:/Dropbox/Art"

    def test_already_normalized_paths_unchanged(self, db: Database) -> None:
        """Paths already using forward slashes are not modified."""
        db._execute(
            "INSERT INTO thumbnails (path, mtime, size, width, height, cache_uuid) VALUES (?, ?, ?, ?, ?, ?)",
            ("/home/user/photos/img.jpg", 1, 100, 800, 600, "u1"),
        )
        db._commit()

        _migrate_v1_to_v2(db)

        row = db.fetch_all("SELECT path FROM thumbnails")
        assert row[0]["path"] == "/home/user/photos/img.jpg"

    def test_idempotent(self, db: Database) -> None:
        """Running the migration twice produces the same result."""
        self._insert_backslash_records(db)
        _migrate_v1_to_v2(db)
        _migrate_v1_to_v2(db)  # Second run should be safe

        rows = db.fetch_all("SELECT path FROM thumbnails ORDER BY path")
        paths = [r["path"] for r in rows]
        assert paths == ["D:/Dropbox/Art/image.jpg", "D:/Dropbox/Art/photo.png"]

    def test_empty_tables_safe(self, db: Database) -> None:
        """Migration runs without error on empty tables."""
        _migrate_v1_to_v2(db)  # Should not raise

    def test_migration_runner_bumps_version(self, db: Database) -> None:
        """MigrationRunner.run() bumps schema version from 1 to 2."""
        self._insert_backslash_records(db)
        db.set_schema_version(1)

        runner = MigrationRunner(db)
        version = runner.run()

        assert version == 2
        # Verify paths were actually migrated
        rows = db.fetch_all("SELECT path FROM thumbnails ORDER BY path")
        paths = [r["path"] for r in rows]
        assert paths == ["D:/Dropbox/Art/image.jpg", "D:/Dropbox/Art/photo.png"]

    def test_migration_runner_skips_when_already_v2(self, db: Database) -> None:
        """MigrationRunner does not re-run migration when already at v2."""
        self._insert_backslash_records(db)
        db.set_schema_version(2)

        runner = MigrationRunner(db)
        version = runner.run()

        assert version == 2
        # Paths should still have backslashes since migration was skipped
        rows = db.fetch_all("SELECT path FROM thumbnails ORDER BY path")
        paths = [r["path"] for r in rows]
        assert "D:\\Dropbox\\Art\\image.jpg" in paths
