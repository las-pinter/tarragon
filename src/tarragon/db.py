"""Database schema initialization and CRUD repository for Tarragon's SQLite store."""

from __future__ import annotations

import logging
import sqlite3
import threading
import time
from collections.abc import Sequence
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

#: Acceptable SQLite parameter types (any sequence thereof).
SqlParams = Sequence[str | int | float | bytes | None]

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


class Database:
    """SQLite-backed repository for Tarragon's catalog data.

    Manages schema initialization and provides CRUD operations for thumbnails,
    tags, favorites, settings, and editor associations.

    Usage as context manager is supported; the caller owns connection lifecycle.
    """

    def __init__(self, db_path: Path) -> None:
        self._db_path = db_path
        self._lock = threading.Lock()
        self._conn = sqlite3.connect(str(db_path), check_same_thread=False)
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._conn.row_factory = sqlite3.Row

    # ── Thread-safe helpers ─────────────────────────────────────

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
            logger.error("SQL failed after %.3fs: %s | params: %s | error: %s", elapsed, sql, params, e)
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
            logger.error("SQL (many) failed after %.3fs: %s | rows: %d | error: %s", elapsed, sql, len(seq), e)
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

    # ── Lifecycle ────────────────────────────────────────────────

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

    # ── Schema version ───────────────────────────────────────────

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

    # ── Thumbnail CRUD ───────────────────────────────────────────

    def upsert_thumbnail(
        self,
        path: str,
        mtime: int,
        size: int,
        width: int,
        height: int,
        cache_uuid: str,
        thumbnail_cache_path: str | None = None,
        preview_cache_path: str | None = None,
        full_cache_path: str | None = None,
    ) -> None:
        """Insert or update a thumbnail record."""
        logger.debug("upsert_thumbnail: path=%s, mtime=%d, size=%d", path, mtime, size)
        self._execute(
            """
            INSERT INTO thumbnails (
                path, mtime, size, width, height,
                cache_uuid, thumbnail_cache_path, preview_cache_path, full_cache_path
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(path) DO UPDATE SET
                mtime=excluded.mtime,
                size=excluded.size,
                width=excluded.width,
                height=excluded.height,
                cache_uuid=excluded.cache_uuid,
                thumbnail_cache_path=excluded.thumbnail_cache_path,
                preview_cache_path=excluded.preview_cache_path,
                full_cache_path=excluded.full_cache_path
            """,
            (
                path, mtime, size, width, height,
                cache_uuid, thumbnail_cache_path, preview_cache_path, full_cache_path,
            ),
        )
        self._commit()

    def delete_thumbnail(self, path: str) -> None:
        """Remove a thumbnail record by path."""
        logger.debug("delete_thumbnail: path=%s", path)
        self._execute("DELETE FROM thumbnails WHERE path = ?", (path,))
        self._commit()

    def get_thumbnail(self, path: str) -> dict[str, Any] | None:
        """Fetch a single thumbnail record as a dict, or None if absent."""
        logger.debug("get_thumbnail: path=%s", path)
        row = self._execute("SELECT * FROM thumbnails WHERE path = ?", (path,)).fetchone()
        return _row_to_dict(row) if row else None

    def list_thumbnails_for_folder(self, folder_path: str) -> list[dict[str, Any]]:
        """List all thumbnail records whose path starts with folder_path."""
        logger.debug("list_thumbnails_for_folder: folder_path=%s", folder_path)
        cursor = self._execute("SELECT * FROM thumbnails WHERE path LIKE ?", (f"{folder_path}%",))
        return [_row_to_dict(row) for row in cursor.fetchall()]

    # ── Tag CRUD ─────────────────────────────────────────────────

    def ensure_tag(self, name: str) -> int:
        """Insert a tag if it doesn't exist; always returns the tag id."""
        logger.debug("ensure_tag: name=%s", name)
        cursor = self._execute(
            "INSERT INTO tags (name) VALUES (?) ON CONFLICT(name) DO UPDATE SET name=name RETURNING id",
            (name,),
        )
        return int(cursor.fetchone()["id"])

    def add_file_tags(self, paths: list[str], tag_id: int, source: str = "user") -> None:
        """Associate one or more file paths with a given tag."""
        logger.debug("add_file_tags: %d paths, tag_id=%d, source=%s", len(paths), tag_id, source)
        self._executemany(
            "INSERT OR IGNORE INTO file_tags (path, tag_id, source) VALUES (?, ?, ?)",
            [(p, tag_id, source) for p in paths],
        )
        self._commit()

    def remove_file_tags(self, paths: list[str], tag_id: int) -> None:
        """Remove file-tag associations for the given paths and tag."""
        logger.debug("remove_file_tags: %d paths, tag_id=%d", len(paths), tag_id)
        placeholders = ",".join("?" * len(paths))
        self._execute(
            f"DELETE FROM file_tags WHERE path IN ({placeholders}) AND tag_id = ?",
            (*paths, tag_id),
        )
        self._commit()

    def get_file_tag_ids(self, path: str) -> set[int]:
        """Return the set of tag ids associated with a given path."""
        logger.debug("get_file_tag_ids: path=%s", path)
        cursor = self._execute("SELECT tag_id FROM file_tags WHERE path = ?", (path,))
        return {row["tag_id"] for row in cursor.fetchall()}

    def replace_auto_color_tags(self, path: str, tags: list[str]) -> None:
        """Delete old auto_color tags for a path and insert new ones."""
        logger.debug("replace_auto_color_tags: path=%s, tags=%d", path, len(tags))
        self._execute(
            "DELETE FROM file_tags WHERE path = ? AND source = 'auto_color'",
            (path,),
        )
        if tags:
            tag_ids: list[int] = []
            for name in tags:
                cursor = self._execute(
                    "INSERT INTO tags (name) VALUES (?) ON CONFLICT(name) DO UPDATE SET name=name RETURNING id",
                    (name,),
                )
                tag_ids.append(cursor.fetchone()["id"])

            self._executemany(
                "INSERT OR IGNORE INTO file_tags (path, tag_id, source) VALUES (?, ?, ?)",
                [(path, tid, "auto_color") for tid in tag_ids],
            )
        self._commit()

    # ── Favorites CRUD ───────────────────────────────────────────

    def add_favorite(
        self,
        path: str,
        label: str | None = None,
        sort_order: int = 0,
    ) -> None:
        """Add a file to favorites."""
        logger.debug("add_favorite: path=%s, label=%s", path, label)
        self._execute(
            "INSERT OR IGNORE INTO favorites (path, label, sort_order) VALUES (?, ?, ?)",
            (path, label, sort_order),
        )
        self._commit()

    def remove_favorite(self, path: str) -> None:
        """Remove a file from favorites."""
        logger.debug("remove_favorite: path=%s", path)
        self._execute("DELETE FROM favorites WHERE path = ?", (path,))
        self._commit()

    def list_favorites(self) -> list[dict[str, Any]]:
        """Return all favorite records ordered by sort_order then path."""
        logger.debug("list_favorites")
        cursor = self._execute("SELECT * FROM favorites ORDER BY sort_order, path")
        return [_row_to_dict(row) for row in cursor.fetchall()]

    # ── Settings CRUD (thin wrapper over settings.py dual access) ─

    def get_setting(self, key: str) -> str | None:
        """Read a raw string setting value; None if absent."""
        logger.debug("get_setting: key=%s", key)
        row = self._execute("SELECT value FROM settings WHERE key = ?", (key,)).fetchone()
        return row["value"] if row else None

    def set_setting(self, key: str, value: str) -> None:
        """Persist a raw string setting."""
        logger.debug("set_setting: key=%s", key)
        self._execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)",
            (key, value),
        )
        self._commit()

    # ── Editor associations CRUD ─────────────────────────────────

    def get_editor_command(self, extension: str) -> str | None:
        """Return the editor command template for an extension; None if absent."""
        logger.debug("get_editor_command: extension=%s", extension)
        row = self._execute(
            "SELECT command_template FROM editor_associations WHERE extension = ?",
            (extension,),
        ).fetchone()
        return row["command_template"] if row else None

    def upsert_editor_association(self, extension: str, command_template: str) -> None:
        """Insert or update an editor association."""
        logger.debug("upsert_editor_association: extension=%s", extension)
        self._execute(
            "INSERT OR REPLACE INTO editor_associations (extension, command_template) VALUES (?, ?)",
            (extension, command_template),
        )
        self._commit()

    def remove_editor_association(self, extension: str) -> None:
        """Remove an editor association by extension."""
        logger.debug("remove_editor_association: extension=%s", extension)
        self._execute("DELETE FROM editor_associations WHERE extension = ?", (extension,))
        self._commit()

    # ── Folder cache UUID CRUD ──────────────────────────────────

    def get_folder_uuid(self, folder_path: str) -> str | None:
        """Return the cache UUID for a source folder, or None if not mapped."""
        logger.debug("get_folder_uuid: folder_path=%s", folder_path)
        row = self._execute(
            "SELECT cache_uuid FROM folder_cache_uuids WHERE folder_path = ?",
            (folder_path,),
        ).fetchone()
        return row["cache_uuid"] if row else None

    def upsert_folder_uuid(self, folder_path: str, cache_uuid: str) -> None:
        """Insert or update the cache UUID for a source folder."""
        logger.debug("upsert_folder_uuid: folder_path=%s", folder_path)
        self._execute(
            "INSERT INTO folder_cache_uuids (folder_path, cache_uuid) VALUES (?, ?) "
            "ON CONFLICT(folder_path) DO UPDATE SET cache_uuid=excluded.cache_uuid",
            (folder_path, cache_uuid),
        )
        self._commit()

    def get_or_create_folder_uuid(self, folder_path: str, candidate_uuid: str) -> str:
        """Atomically insert a candidate UUID and return the winning UUID.

        Uses INSERT ... ON CONFLICT DO NOTHING so that concurrent callers
        for the same folder all converge on a single UUID without a
        read-then-write race.  The actual stored UUID is always read back
        to guarantee consistency.
        """
        logger.debug("get_or_create_folder_uuid: folder_path=%s", folder_path)
        with self._lock:
            self._conn.execute(
                "INSERT INTO folder_cache_uuids (folder_path, cache_uuid) "
                "VALUES (?, ?) ON CONFLICT(folder_path) DO NOTHING",
                (folder_path, candidate_uuid),
            )
            row = self._conn.execute(
                "SELECT cache_uuid FROM folder_cache_uuids WHERE folder_path = ?",
                (folder_path,),
            ).fetchone()
            self._conn.commit()
        return str(row["cache_uuid"])

    def cleanup_stale_folder_uuids(self) -> int:
        """Remove folder_cache_uuids entries whose source folder no longer exists.

        Returns the number of stale entries removed.
        """
        logger.debug("cleanup_stale_folder_uuids: checking for stale entries")
        cursor = self._execute("SELECT folder_path FROM folder_cache_uuids")
        folder_paths = [row["folder_path"] for row in cursor.fetchall()]

        stale_paths = [fp for fp in folder_paths if not Path(fp).is_dir()]

        if stale_paths:
            placeholders = ",".join("?" * len(stale_paths))
            self._execute(
                f"DELETE FROM folder_cache_uuids WHERE folder_path IN ({placeholders})",
                tuple(stale_paths),
            )
            self._commit()

        return len(stale_paths)

    # ── Context manager protocol ─────────────────────────────────

    def __enter__(self) -> Database:
        return self

    def __exit__(self, *args: Any) -> None:
        self.close()
