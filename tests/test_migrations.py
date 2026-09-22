"""Tests for migrations"""

from __future__ import annotations

import sqlite3
from collections.abc import Generator
from pathlib import Path

import pytest

from tarragon.db.database import Database
from tarragon.migrations import MigrationRunner


@pytest.fixture()
def db() -> Generator[Database, None, None]:
    """Provide an in-memory database for each test (isolated)."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


class TestMigrationRunnerBootstrap:
    """MigrationRunner bootstraps databases to the current schema version."""

    def test_bootstrap_sets_version_to_2(self, db: Database) -> None:
        """Fresh database (version 0) is bootstrapped to current version 2."""
        runner = MigrationRunner(db)
        version = runner.run()
        assert version == 2

    def test_legacy_version_1_database_is_re_stamped_to_2(self, tmp_path: Path) -> None:
        """Legacy databases stamped version 1 are re-stamped to current version 2.

        Databases from before file_tags.source existed are stamped version 1;
        init_schema() adds the column, and the runner must update the stored
        version so it no longer lies at 1.
        """
        legacy_path = tmp_path / "legacy.db"
        conn = sqlite3.connect(str(legacy_path))
        conn.executescript(
            """
            CREATE TABLE schema_version (version INTEGER NOT NULL);
            CREATE TABLE tags (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE NOT NULL,
                source TEXT NOT NULL DEFAULT 'user'
            );
            CREATE TABLE file_tags (
                path TEXT NOT NULL,
                tag_id INTEGER NOT NULL,
                PRIMARY KEY (path, tag_id),
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            );
            INSERT INTO schema_version (version) VALUES (1);
            """
        )
        conn.commit()
        conn.close()

        db = Database(legacy_path)
        try:
            version = MigrationRunner(db).run()
            assert version == 2
            assert db.get_schema_version() == 2
            columns = {str(row["name"]) for row in db._conn.execute("PRAGMA table_info(file_tags)").fetchall()}
            assert "source" in columns
        finally:
            db.close()
