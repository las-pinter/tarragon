"""Shared infrastructure for the Database class.

Provides connection management, thread-safe SQL helpers, schema
initialisation, and module-level utilities used across all mixins.
"""

from __future__ import annotations

import logging
import sqlite3
import threading
import time
from collections.abc import Sequence
from pathlib import Path
from typing import Any, Self

logger = logging.getLogger(__name__)

#: Acceptable SQLite parameter types (any sequence thereof).
SqlParams = Sequence[str | int | float | bytes | None]


def _normalize_path(path: str) -> str:
    """Normalize path separators to forward slashes for cross-platform consistency.

    SQLite LIKE patterns use ``/`` as the separator.  On Windows,
    ``str(Path(...))`` produces backslash-separated paths which do not
    match ``/%`` LIKE patterns.  Normalizing everything to forward slashes
    at the database boundary ensures consistent behaviour on all platforms.
    """
    if not path:
        return path
    return path.replace("\\", "/")


#: Public alias for use by other modules.
normalize_path = _normalize_path

INITIAL_SCHEMA = """
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS thumbnails (
    path TEXT PRIMARY KEY, mtime INTEGER NOT NULL, size INTEGER NOT NULL,
    thumb_hash TEXT, width INTEGER NOT NULL, height INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    cache_uuid TEXT,
    thumbnail_cache_path TEXT,
    preview_cache_path TEXT,
    full_cache_path TEXT
);
CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE NOT NULL
);
CREATE TABLE IF NOT EXISTS file_tags (
    path TEXT NOT NULL, tag_id INTEGER NOT NULL,
    source TEXT NOT NULL DEFAULT 'user',
    PRIMARY KEY (path, tag_id, source),
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS favorites (
    id INTEGER PRIMARY KEY AUTOINCREMENT, path TEXT UNIQUE NOT NULL,
    label TEXT, sort_order INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS editor_associations (
    extension TEXT PRIMARY KEY, command_template TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS folder_cache_uuids (
    folder_path TEXT PRIMARY KEY, cache_uuid TEXT NOT NULL
);
"""


def _row_to_dict(row: sqlite3.Row) -> dict[str, Any]:
    """Convert a sqlite3.Row to a plain dict."""
    return dict(row)


class _MixinBase:
    """Abstract base for database mixins — declares the shared interface.

    This class defines the attributes and methods that all mixin classes
    rely on (``_execute``, ``_commit``, ``_conn``, ``_lock``, etc.) without
    providing an ``__init__``.  This avoids MRO diamond conflicts when
    ``Database`` inherits from both ``_Base`` and the mixins.
    """

    _conn: sqlite3.Connection
    _lock: threading.Lock
    _db_path: Path

    def _execute(self, sql: str, params: SqlParams = ()) -> sqlite3.Cursor:
        raise NotImplementedError

    def _executemany(self, sql: str, seq: Sequence[SqlParams]) -> None:
        raise NotImplementedError

    def _executescript(self, sql: str) -> None:
        raise NotImplementedError

    def _commit(self) -> None:
        raise NotImplementedError

    def fetch_all(self, sql: str, params: SqlParams = ()) -> list[dict[str, Any]]:
        raise NotImplementedError


class _Base(_MixinBase):
    """Base database class providing connection management and SQL helpers.

    ``Database`` composes ``_Base`` with all mixins via multiple inheritance.
    """

    def __init__(self, db_path: Path) -> None:
        self._db_path = db_path
        self._lock = threading.Lock()
        self._conn = sqlite3.connect(str(db_path), check_same_thread=False)
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._conn.row_factory = sqlite3.Row

    # -- Thread-safe helpers ------------------------------------------------

    def _execute(self, sql: str, params: SqlParams = ()) -> sqlite3.Cursor:
        """Execute SQL with lock for thread safety."""
        start = time.perf_counter()
        logger.debug("SQL: %s | params: %s", sql, params)
        try:
            with self._lock:
                cursor = self._conn.execute(sql, params)
            elapsed = time.perf_counter() - start
            logger.debug("SQL completed in %.3fs", elapsed)
            return cursor
        except sqlite3.Error as e:
            elapsed = time.perf_counter() - start
            logger.error(
                "SQL failed after %.3fs: %s | params: %s | error: %s",
                elapsed,
                sql,
                params,
                e,
            )
            raise

    def _executemany(self, sql: str, seq: Sequence[SqlParams]) -> None:
        """Execute executemany with lock for thread safety."""
        start = time.perf_counter()
        logger.debug("SQL (many): %s | rows: %d", sql, len(seq))
        try:
            with self._lock:
                self._conn.executemany(sql, seq)
            elapsed = time.perf_counter() - start
            logger.debug("SQL (many) completed in %.3fs", elapsed)
        except sqlite3.Error as e:
            elapsed = time.perf_counter() - start
            logger.error(
                "SQL (many) failed after %.3fs: %s | rows: %d | error: %s",
                elapsed,
                sql,
                len(seq),
                e,
            )
            raise

    def _executescript(self, sql: str) -> None:
        """Execute executescript with lock for thread safety."""
        start = time.perf_counter()
        logger.debug("SQL (script): executing schema script")
        try:
            with self._lock:
                self._conn.executescript(sql)
            elapsed = time.perf_counter() - start
            logger.debug("SQL (script) completed in %.3fs", elapsed)
        except sqlite3.Error as e:
            elapsed = time.perf_counter() - start
            logger.error("SQL (script) failed after %.3fs: error: %s", elapsed, e)
            raise

    def _commit(self) -> None:
        """Commit transaction with lock for thread safety."""
        with self._lock:
            self._conn.commit()

    # -- Generic query executor ---------------------------------------------

    def fetch_all(self, sql: str, params: SqlParams = ()) -> list[dict[str, Any]]:
        """Execute a SQL query and return all rows as a list of dicts.

        Parameters
        ----------
        sql:
            Parameterized SQL query string.
        params:
            Query parameters.

        Returns
        -------
        list[dict[str, Any]]
            List of row dicts (column_name -> value).
        """
        logger.debug("fetch_all: %s | params: %s", sql, params)
        cursor = self._execute(sql, params)
        return [_row_to_dict(row) for row in cursor.fetchall()]

    # -- Lifecycle -----------------------------------------------------------

    def init_schema(self) -> None:
        """Execute INITIAL_SCHEMA; creates all tables if absent. Idempotent."""
        logger.info("Initializing database schema at %s", self._db_path)
        self._executescript(INITIAL_SCHEMA)
        self._commit()
        logger.info("Database schema initialized successfully")

    def close(self) -> None:
        """Close the underlying SQLite connection."""
        logger.debug("Closing database connection: %s", self._db_path)
        self._conn.close()

    # -- Schema version ------------------------------------------------------

    def set_schema_version(self, version: int) -> None:
        """Set the current schema version. Replaces any existing rows."""
        logger.debug("set_schema_version: version=%d", version)
        self._execute("DELETE FROM schema_version")
        self._execute(
            "INSERT INTO schema_version (version) VALUES (?)",
            (version,),
        )
        self._commit()

    def get_schema_version(self) -> int:
        """Return the stored schema version, or 0 if absent."""
        logger.debug("get_schema_version")
        row = self._execute("SELECT version FROM schema_version LIMIT 1").fetchone()
        return row["version"] if row else 0

    # -- Context manager protocol --------------------------------------------

    def __enter__(self) -> Self:
        return self

    def __exit__(self, *args: Any) -> None:
        self.close()
