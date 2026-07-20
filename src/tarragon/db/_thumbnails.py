"""Thumbnail CRUD operations mixed into the Database class."""

from __future__ import annotations

import logging
from typing import Any

from tarragon.db._base import _MixinBase, _row_to_dict, normalize_path

logger = logging.getLogger(__name__)


class ThumbnailsMixin(_MixinBase):
    """Insert, query, and delete thumbnail records."""

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
        path = normalize_path(path)
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
                path,
                mtime,
                size,
                width,
                height,
                cache_uuid,
                thumbnail_cache_path,
                preview_cache_path,
                full_cache_path,
            ),
        )
        self._commit()

    def bulk_upsert_stubs(self, files: list[tuple[str, int, int]]) -> None:
        """Batch-insert minimal thumbnail records for files discovered during a scan.

        Inserts path, mtime, and size with placeholder values for width/height
        (0) and cache fields (None/empty).  This ensures the database has records
        for the current folder *before* async thumbnail rendering completes, so
        that filtered queries return results immediately.

        When rendering later calls `upsert_thumbnail()` with real dimensions
        and cache paths, the ``ON CONFLICT`` clause updates the stub in place.

        Parameters
        ----------
        files:
            List of `(path, mtime, size)` tuples.
        """
        if not files:
            return
        logger.debug("bulk_upsert_stubs: %d files", len(files))
        # Normalize path separators to forward slashes for cross-platform consistency
        normalized = [(normalize_path(path), mtime, size) for path, mtime, size in files]
        self._executemany(
            """
            INSERT INTO thumbnails (
                path, mtime, size, width, height,
                cache_uuid, thumbnail_cache_path, preview_cache_path, full_cache_path
            )
            VALUES (?, ?, ?, 0, 0, '', NULL, NULL, NULL)
            ON CONFLICT(path) DO UPDATE SET
                mtime=excluded.mtime,
                size=excluded.size
            """,
            normalized,
        )
        self._commit()

    def delete_thumbnail(self, path: str) -> None:
        """Remove a thumbnail record by path."""
        path = normalize_path(path)
        logger.debug("delete_thumbnail: path=%s", path)
        self._execute("DELETE FROM thumbnails WHERE path = ?", (path,))
        self._commit()

    def get_thumbnail(self, path: str) -> dict[str, Any] | None:
        """Fetch a single thumbnail record as a dict, or None if absent."""
        path = normalize_path(path)
        logger.debug("get_thumbnail: path=%s", path)
        row = self._execute("SELECT * FROM thumbnails WHERE path = ?", (path,)).fetchone()
        return _row_to_dict(row) if row else None

    def list_thumbnails_for_folder(self, folder_path: str) -> list[dict[str, Any]]:
        """List all thumbnail records whose path starts with folder_path."""
        folder_path = normalize_path(folder_path)
        logger.debug("list_thumbnails_for_folder: folder_path=%s", folder_path)
        cursor = self._execute(
            "SELECT * FROM thumbnails WHERE path LIKE ?",
            (f"{folder_path.rstrip('/')}/%",),
        )
        return [_row_to_dict(row) for row in cursor.fetchall()]

    def delete_thumbnails_by_folder(self, folder_path: str) -> None:
        """Remove all thumbnail records whose path starts with *folder_path*.

        Parameters
        ----------
        folder_path:
            Folder path prefix.  All thumbnails whose path begins with
            this value (followed by ``/``) are deleted.
        """
        logger.debug("delete_thumbnails_by_folder: folder_path=%s", folder_path)
        folder_path = normalize_path(folder_path)
        self._execute(
            "DELETE FROM thumbnails WHERE path LIKE ?",
            (f"{folder_path.rstrip('/')}/%",),
        )
        self._commit()

    def list_distinct_folders(self) -> list[str]:
        """Return a sorted list of distinct folder paths from the thumbnails table.

        Extracts the parent directory of each thumbnail path and returns
        the unique set, sorted alphabetically.  Paths with empty or
        missing parent directories are excluded.

        Returns
        -------
        list[str]
            Sorted list of distinct folder path strings.
        """
        logger.debug("list_distinct_folders")
        cursor = self._execute("SELECT path FROM thumbnails WHERE path != ''")
        folders: set[str] = set()
        for row in cursor.fetchall():
            # Paths are stored with forward slashes; use PurePosixPath to
            # extract the parent without converting separators on Windows.
            raw_path: str = row["path"]
            last_sep = raw_path.rfind("/")
            parent = raw_path[:last_sep] if last_sep > 0 else ""
            if parent and parent != ".":
                folders.add(parent)
        return sorted(folders)
