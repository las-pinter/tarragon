"""Migration runner for Tarragon's SQLite schema.

Migrations are applied sequentially based on the stored schema version.
Each migration function receives the Database instance and is responsible
for its own SQL execution and committing.
"""

from __future__ import annotations

import logging

from tarragon.db import Database

logger = logging.getLogger(__name__)


def _migrate_v1_to_v2(db: Database) -> None:
    """Normalize all stored paths from backslashes to forward slashes.

    Earlier versions of Tarragon on Windows stored paths with backslash
    separators (e.g. ``D:\\Dropbox\\Art\\image.jpg``).  The application now
    normalizes all paths to forward slashes at the database boundary.  This
    migration ensures existing records are consistent so that ``ON CONFLICT``
    clauses, ``LIKE`` patterns, and tag/favorite lookups work correctly.
    """
    tables_columns = [
        ("thumbnails", "path"),
        ("file_tags", "path"),
        ("favorites", "path"),
        ("folder_cache_uuids", "folder_path"),
    ]
    for table, column in tables_columns:
        db._execute(
            f"UPDATE {table} SET {column} = REPLACE({column}, '\\', '/')"
            f" WHERE {column} LIKE '%\\%'"
        )
    db._commit()
    logger.info("Migration v1→v2: normalized backslash paths to forward slashes")


class MigrationRunner:
    """Orchestrates database schema migrations.

    Applies pending migrations in order based on the stored schema version.
    """

    def __init__(self, db: Database) -> None:
        self._db = db

    def run(self) -> int:
        """Execute pending migrations and return the new schema version.

        Returns:
            The current schema version after all migrations have been applied.
        """
        self._db.init_schema()

        # After init_schema, bootstrap version if needed
        if self._db.get_schema_version() == 0:
            self._db.set_schema_version(1)

        # v1 → v2: normalize backslash paths to forward slashes
        if self._db.get_schema_version() < 2:
            logger.info("Running migration v1→v2 (path normalization)")
            _migrate_v1_to_v2(self._db)
            self._db.set_schema_version(2)

        return self._db.get_schema_version()
