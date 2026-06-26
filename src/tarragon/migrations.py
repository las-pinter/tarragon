"""Migration runner for Tarragon's SQLite schema.

MVP note: no migrations needed yet — the initial schema is stable.
This module exists as a framework for future PRs that introduce version bumps.
"""

from __future__ import annotations

from tarragon.db import Database


class MigrationRunner:
    """Orchestrates database schema migrations.

    Currently a stub; ready to accept migration functions in future versions.
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
