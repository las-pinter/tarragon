"""Migration runner for Tarragon's SQLite schema.

The schema version is a single generation marker, not a chain of per-version
migrations: ``init_schema()`` brings the schema up to date (including the
legacy ``file_tags.source`` guard) and the runner re-stamps the stored version
to the current generation.
"""

from __future__ import annotations

import logging

from tarragon.db.database import Database

logger = logging.getLogger(__name__)


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

        # After init_schema, bootstrap version if needed. Re-stamp legacy
        # databases too: version 1 predates file_tags.source and must be
        # recorded as migrated once init_schema() has added the column.
        if self._db.get_schema_version() < 2:
            self._db.set_schema_version(2)

        return self._db.get_schema_version()
