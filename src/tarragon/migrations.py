"""Migration runner for Tarragon's SQLite schema.

Migrations are applied sequentially based on the stored schema version.
Each migration function receives the Database instance and is responsible
for its own SQL execution and committing.
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

        # After init_schema, bootstrap version if needed
        if self._db.get_schema_version() == 0:
            self._db.set_schema_version(1)

        return self._db.get_schema_version()
