"""Folder cache UUID CRUD operations mixed into the Database class."""

from __future__ import annotations

import logging
from pathlib import Path

from tarragon.db._base import MixinBase, normalize_path

logger = logging.getLogger(__name__)


class FolderCacheMixin(MixinBase):
    """Map source folders to cache UUIDs and clean up stale entries."""

    def get_folder_uuid(self, folder_path: str) -> str | None:
        """Return the cache UUID for a source folder, or None if not mapped."""
        folder_path = normalize_path(folder_path)
        logger.debug("get_folder_uuid: folder_path=%s", folder_path)
        row = self._execute(
            "SELECT cache_uuid FROM folder_cache_uuids WHERE folder_path = ?",
            (folder_path,),
        ).fetchone()
        return row["cache_uuid"] if row else None

    def upsert_folder_uuid(self, folder_path: str, cache_uuid: str) -> None:
        """Insert or update the cache UUID for a source folder."""
        folder_path = normalize_path(folder_path)
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
        read-then-write race. The actual stored UUID is always read back
        to guarantee consistency.
        """
        folder_path = normalize_path(folder_path)
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
