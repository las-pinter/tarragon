"""Settings CRUD operations mixed into the Database class."""

from __future__ import annotations

import logging

from tarragon.db._base import _MixinBase

logger = logging.getLogger(__name__)


class SettingsMixin(_MixinBase):
    """Read and write key-value settings."""

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
