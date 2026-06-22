"""Database schema initialization and CRUD repository for Tarragon's SQLite store."""

from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import Any

INITIAL_SCHEMA = """
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS thumbnails (
    path TEXT PRIMARY KEY, mtime INTEGER NOT NULL, size INTEGER NOT NULL,
    thumb_hash TEXT, width INTEGER NOT NULL, height INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    master_cache_path TEXT
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
        self._conn = sqlite3.connect(str(db_path))
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._conn.row_factory = sqlite3.Row

    # ── Lifecycle ────────────────────────────────────────────────

    def init_schema(self) -> None:
        """Execute INITIAL_SCHEMA; creates all tables if absent. Idempotent."""
        self._conn.executescript(INITIAL_SCHEMA)
        self._conn.commit()

    def close(self) -> None:
        """Close the underlying SQLite connection."""
        self._conn.close()

    # ── Schema version ───────────────────────────────────────────

    def set_schema_version(self, version: int) -> None:
        """Set the current schema version. Replaces any existing rows."""
        self._conn.execute("DELETE FROM schema_version")
        self._conn.execute(
            "INSERT INTO schema_version (version) VALUES (?)",
            (version,),
        )
        self._conn.commit()

    def get_schema_version(self) -> int:
        """Return the stored schema version, or 0 if absent."""
        row = self._conn.execute("SELECT version FROM schema_version LIMIT 1").fetchone()
        return row["version"] if row else 0

    # ── Thumbnail CRUD ───────────────────────────────────────────

    def upsert_thumbnail(
        self,
        path: str,
        mtime: int,
        size: int,
        width: int,
        height: int,
        master_cache_path: str | None = None,
    ) -> None:
        """Insert or update a thumbnail record."""
        self._conn.execute(
            """
            INSERT INTO thumbnails (path, mtime, size, width, height, master_cache_path)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(path) DO UPDATE SET
                mtime=excluded.mtime,
                size=excluded.size,
                width=excluded.width,
                height=excluded.height,
                master_cache_path=excluded.master_cache_path
            """,
            (path, mtime, size, width, height, master_cache_path),
        )
        self._conn.commit()

    def delete_thumbnail(self, path: str) -> None:
        """Remove a thumbnail record by path."""
        self._conn.execute("DELETE FROM thumbnails WHERE path = ?", (path,))
        self._conn.commit()

    def get_thumbnail(self, path: str) -> dict | None:
        """Fetch a single thumbnail record as a dict, or None if absent."""
        row = self._conn.execute("SELECT * FROM thumbnails WHERE path = ?", (path,)).fetchone()
        return _row_to_dict(row) if row else None

    def list_thumbnails_for_folder(self, folder_path: str) -> list[dict]:
        """List all thumbnail records whose path starts with folder_path."""
        cursor = self._conn.execute("SELECT * FROM thumbnails WHERE path LIKE ?", (f"{folder_path}%",))
        return [_row_to_dict(row) for row in cursor.fetchall()]

    # ── Tag CRUD ─────────────────────────────────────────────────

    def ensure_tag(self, name: str) -> int:
        """Insert a tag if it doesn't exist; always returns the tag id."""
        cursor = self._conn.execute(
            "INSERT INTO tags (name) VALUES (?) ON CONFLICT(name) DO UPDATE SET name=name RETURNING id",
            (name,),
        )
        return cursor.fetchone()["id"]

    def add_file_tags(self, paths: list[str], tag_id: int, source: str = "user") -> None:
        """Associate one or more file paths with a given tag."""
        self._conn.executemany(
            "INSERT OR IGNORE INTO file_tags (path, tag_id, source) VALUES (?, ?, ?)",
            [(p, tag_id, source) for p in paths],
        )
        self._conn.commit()

    def remove_file_tags(self, paths: list[str], tag_id: int) -> None:
        """Remove file-tag associations for the given paths and tag."""
        placeholders = ",".join("?" * len(paths))
        self._conn.execute(
            f"DELETE FROM file_tags WHERE path IN ({placeholders}) AND tag_id = ?",
            (*paths, tag_id),
        )
        self._conn.commit()

    def get_file_tag_ids(self, path: str) -> set[int]:
        """Return the set of tag ids associated with a given path."""
        cursor = self._conn.execute("SELECT tag_id FROM file_tags WHERE path = ?", (path,))
        return {row["tag_id"] for row in cursor.fetchall()}

    def replace_auto_color_tags(self, path: str, tags: list[str]) -> None:
        """Delete old auto_color tags for a path and insert new ones."""
        self._conn.execute(
            "DELETE FROM file_tags WHERE path = ? AND source = 'auto_color'",
            (path,),
        )
        if tags:
            tag_ids: list[int] = []
            for name in tags:
                cursor = self._conn.execute(
                    "INSERT INTO tags (name) VALUES (?) ON CONFLICT(name) DO UPDATE SET name=name RETURNING id",
                    (name,),
                )
                tag_ids.append(cursor.fetchone()["id"])

            self._conn.executemany(
                "INSERT OR IGNORE INTO file_tags (path, tag_id, source) VALUES (?, ?, ?)",
                [(path, tid, "auto_color") for tid in tag_ids],
            )
        self._conn.commit()

    # ── Favorites CRUD ───────────────────────────────────────────

    def add_favorite(
        self,
        path: str,
        label: str | None = None,
        sort_order: int = 0,
    ) -> None:
        """Add a file to favorites."""
        self._conn.execute(
            "INSERT OR IGNORE INTO favorites (path, label, sort_order) VALUES (?, ?, ?)",
            (path, label, sort_order),
        )
        self._conn.commit()

    def remove_favorite(self, path: str) -> None:
        """Remove a file from favorites."""
        self._conn.execute("DELETE FROM favorites WHERE path = ?", (path,))
        self._conn.commit()

    def list_favorites(self) -> list[dict]:
        """Return all favorite records ordered by sort_order then path."""
        cursor = self._conn.execute("SELECT * FROM favorites ORDER BY sort_order, path")
        return [_row_to_dict(row) for row in cursor.fetchall()]

    # ── Settings CRUD (thin wrapper over settings.py dual access) ─

    def get_setting(self, key: str) -> str | None:
        """Read a raw string setting value; None if absent."""
        row = self._conn.execute("SELECT value FROM settings WHERE key = ?", (key,)).fetchone()
        return row["value"] if row else None

    def set_setting(self, key: str, value: str) -> None:
        """Persist a raw string setting."""
        self._conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)",
            (key, value),
        )
        self._conn.commit()

    # ── Editor associations CRUD ─────────────────────────────────

    def get_editor_command(self, extension: str) -> str | None:
        """Return the editor command template for an extension; None if absent."""
        row = self._conn.execute(
            "SELECT command_template FROM editor_associations WHERE extension = ?",
            (extension,),
        ).fetchone()
        return row["command_template"] if row else None

    def upsert_editor_association(self, extension: str, command_template: str) -> None:
        """Insert or update an editor association."""
        self._conn.execute(
            "INSERT OR REPLACE INTO editor_associations (extension, command_template) VALUES (?, ?)",
            (extension, command_template),
        )
        self._conn.commit()

    def remove_editor_association(self, extension: str) -> None:
        """Remove an editor association by extension."""
        self._conn.execute("DELETE FROM editor_associations WHERE extension = ?", (extension,))
        self._conn.commit()

    # ── Context manager protocol ─────────────────────────────────

    def __enter__(self) -> Database:
        return self

    def __exit__(self, *args: Any) -> None:
        self.close()
