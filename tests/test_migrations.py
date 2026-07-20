"""Tests for src/tarragon/migrations.py — schema migration runner."""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

import pytest
from tarragon.db import Database
from tarragon.migrations import MigrationRunner

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
    def test_bootstrap_sets_version_to_1(self, db: Database) -> None:
        """Fresh database (version 0) is bootstrapped to current version 2."""
        runner = MigrationRunner(db)
        version = runner.run()
        assert version == 1
