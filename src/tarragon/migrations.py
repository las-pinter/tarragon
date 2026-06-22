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
        # No migrations to run yet — just return the existing version.
        return self._db.get_schema_version()
