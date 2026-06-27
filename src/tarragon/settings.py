"""Settings persistence — typed key-value store backed by SQLite."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from tarragon.db import Database

DEFAULTS: dict[str, Any] = {
    "max_multi_preview": 9,
    "large_canvas_threshold_mp": 20.0,
    "tile_grid_size": "2x2",
    "max_psd_workers": 3,
    "color_tag_enabled": True,
    "color_tag_palette_size": 8,
    "color_tag_min_share": 0.10,
    "color_tag_neutral_s_threshold": 0.15,
    "cache_dir": None,
    "cache_format": "png",
    "debug_mode": False,
}


class Settings:
    """Typed settings repository that delegates storage to a Database instance.

    JSON serialization/deserialization is handled here; the underlying
    Database stores raw string values via ``get_setting`` / ``set_setting``.
    """

    def __init__(self, db: Database) -> None:
        self._db = db

    # ── Lifecycle ────────────────────────────────────────────────

    def init_defaults(self) -> None:
        """INSERT OR IGNORE every DEFAULTS entry — idempotent first-run setup."""
        for key, value in DEFAULTS.items():
            if self._db.get_setting(key) is None:
                self._db.set_setting(key, json.dumps(value))

    # ── Accessors ────────────────────────────────────────────────

    def get(self, key: str) -> Any:
        """Read a setting value; fall back to DEFAULTS if the key is absent."""
        raw = self._db.get_setting(key)
        if raw is None:
            if key not in DEFAULTS:
                raise KeyError(key)
            return DEFAULTS[key]
        return json.loads(raw)

    def set(self, key: str, value: Any) -> None:
        """Persist a setting value (JSON-serialized)."""
        self._db.set_setting(key, json.dumps(value))

    # ── Teardown ─────────────────────────────────────────────────

    def close(self) -> None:
        """No-op — connection lifecycle is owned by the Database instance."""

    def __enter__(self) -> Settings:
        return self

    def __exit__(self, *args: Any) -> None:
        self.close()
