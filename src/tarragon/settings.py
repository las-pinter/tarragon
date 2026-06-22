"""Settings persistence — typed key-value store backed by SQLite."""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path
from typing import Any

DEFAULTS: dict[str, Any] = {
    "max_multi_preview": 9,
    "large_canvas_threshold_mp": 20.0,
    "tile_grid_size": "2x2",
    "max_psd_workers": 3,
    "color_tag_enabled": True,
    "color_tag_palette_size": 8,
    "color_tag_min_share": 0.10,
    "color_tag_neutral_s_threshold": 0.15,
    "cache_format": "png",
}


class Settings:
    """SQLite-backed settings repository with JSON-serialized values."""

    def __init__(self, db_path: Path) -> None:
        self._db_path = db_path
        self._conn = sqlite3.connect(str(db_path))
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._ensure_schema()

    # ── Lifecycle ────────────────────────────────────────────────

    def _ensure_schema(self) -> None:
        """Create the settings table if it does not yet exist."""
        self._conn.execute(
            """
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            """
        )
        self._conn.commit()

    def init_defaults(self) -> None:
        """INSERT OR IGNORE every DEFAULTS entry — idempotent first-run setup."""
        for key, value in DEFAULTS.items():
            serialized = json.dumps(value)
            self._conn.execute(
                "INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)",
                (key, serialized),
            )
        self._conn.commit()

    # ── Accessors ────────────────────────────────────────────────

    def get(self, key: str) -> Any:
        """Read a setting value; fall back to DEFAULTS if the key is absent."""
        row = self._conn.execute("SELECT value FROM settings WHERE key = ?", (key,)).fetchone()
        if row is None:
            if key not in DEFAULTS:
                raise KeyError(key)
            return DEFAULTS[key]
        return json.loads(row[0])

    def set(self, key: str, value: Any) -> None:
        """Persist a setting value (JSON-serialized)."""
        serialized = json.dumps(value)
        self._conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)",
            (key, serialized),
        )
        self._conn.commit()

    # ── Teardown ─────────────────────────────────────────────────

    def close(self) -> None:
        """Close the underlying SQLite connection."""
        self._conn.close()

    def __enter__(self) -> Settings:
        return self

    def __exit__(self, *args: Any) -> None:
        self.close()
