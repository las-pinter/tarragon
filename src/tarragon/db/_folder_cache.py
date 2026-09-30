"""Folder cache UUID CRUD operations mixed into the Database class."""

from __future__ import annotations

import logging
from pathlib import Path

from tarragon.db._base import MixinBase, in_clause, normalize_path

logger = logging.getLogger(__name__)


class FolderCacheMixin(MixinBase):
    """Map source folders to cache UUIDs and clean up stale entries."""

    def get_folder_uuid(self, folder_path: str) -> str | None:
        """Return the cache UUID for a source folder, or None if not mapped."""
        folder_path = normalize_path(folder_path)
        logger.debug("Called - folder_path: %s", folder_path)
        row = self._fetch_one_locked(
            "SELECT cache_uuid FROM folder_cache_uuids WHERE folder_path = ?",
            (folder_path,),
        )
        return str(row["cache_uuid"]) if row is not None else None

    def upsert_folder_uuid(self, folder_path: str, cache_uuid: str) -> None:
        """Insert or update the cache UUID for a source folder."""
        folder_path = normalize_path(folder_path)
        logger.debug("Called - folder_path: %s", folder_path)
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
        logger.debug("Called - folder_path: %s", folder_path)
        insert_sql = (
            "INSERT INTO folder_cache_uuids (folder_path, cache_uuid) VALUES (?, ?) ON CONFLICT(folder_path) DO NOTHING"
        )
        # Convergent design: each statement is separately locked inside
        # _execute/_commit, so racing threads never hold the lock at the
        # same time (no nesting, no deadlock). Whichever INSERT wins the
        # race, the read-back returns that single stored UUID, so every
        # caller converges on the same value for this folder.
        #
        # The read-back runs fully under the connection lock via
        # _fetch_one_locked (SELECT + fetchone + dict copy atomic): a shared
        # connection's Row/statement state must not be consumed unlocked,
        # or an interleaving thread's conn.execute corrupts it (IndexError /
        # TypeError / wrong string returns under concurrent load).
        self._execute(insert_sql, (folder_path, candidate_uuid))
        row = self._fetch_one_locked(
            "SELECT cache_uuid FROM folder_cache_uuids WHERE folder_path = ?",
            (folder_path,),
        )
        # With the outer lock gone, a concurrent cleanup_stale_folder_uuids
        # DELETE can commit between our INSERT and read-back (folder removed
        # from disk in the window), so a None read-back is legitimate and
        # must not crash on a subscript. Retry once to re-establish the row;
        # if it still vanishes, the mapping was deleted concurrently and we
        # fail loudly instead of returning a wrong value.
        if row is None:
            self._execute(insert_sql, (folder_path, candidate_uuid))
            row = self._fetch_one_locked(
                "SELECT cache_uuid FROM folder_cache_uuids WHERE folder_path = ?",
                (folder_path,),
            )
        self._commit()
        if row is None:
            raise RuntimeError(
                f"cache UUID for {folder_path!r} vanished between INSERT and "
                "read-back even after one retry (concurrent cleanup?)"
            )
        return str(row["cache_uuid"])

    def cleanup_stale_folder_uuids(self) -> int:
        """Remove folder_cache_uuids entries whose source folder no longer exists.

        Returns the number of stale entries removed.
        """
        logger.debug("Called")
        # With a connection shared across threads, the SELECT + fetchall +
        # extraction must happen under the lock: consuming the cursor after
        # _execute releases it lets another thread's conn.execute corrupt
        # the statement state (same pattern issue as get_or_create_folder_uuid).
        with self._lock:
            rows = self._conn.execute("SELECT folder_path FROM folder_cache_uuids").fetchall()
            folder_paths = [row["folder_path"] for row in rows]

        stale_paths = [fp for fp in folder_paths if not Path(fp).is_dir()]

        if stale_paths:
            self._execute(
                f"DELETE FROM folder_cache_uuids WHERE folder_path IN {in_clause(len(stale_paths))}",
                tuple(stale_paths),
            )
            self._commit()

        return len(stale_paths)
