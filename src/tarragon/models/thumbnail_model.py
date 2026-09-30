"""QAbstractListModel providing path strings to a thumbnail grid view."""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass
from pathlib import Path
from typing import override

from PySide6.QtCore import QAbstractListModel, QModelIndex, QObject, QPersistentModelIndex, Qt

from tarragon.renderers.cache import RESOLUTION_FULL, RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL, tier_key

logger = logging.getLogger(__name__)


@dataclass(frozen=True)
class ThumbnailUpdate:
    """A single cached-path assignment for :meth:`ThumbnailModel.set_thumbnails`.

    Attributes:
        source_path: The original file path (as string).
        cache_path: The cached file path on disk.
        resolution: Pixel resolution (256, 1024) or None for full.
    """

    source_path: str
    cache_path: Path
    resolution: int | None = None


class ThumbnailModel(QAbstractListModel):
    """A list model that holds file paths for a QListView-based thumbnail grid.

    Provides multiple roles:
        - DisplayRole:        file basename (``path.name``)
        - PathRole:           full path as a string
        - ThumbnailRole256:   256px cached thumbnail path
        - ThumbnailRole1024:  1024px cached preview path
        - ThumbnailRoleFull:  full-resolution cached path

    Use :meth:`set_paths` to replace the entire path list.
    Use :meth:`set_thumbnails` (or the single-item :meth:`set_thumbnail`) to
    register cached paths for a resolution.
    """

    PathRole: int = Qt.ItemDataRole.UserRole + 1
    ThumbnailRole256: int = Qt.ItemDataRole.UserRole + 2
    ThumbnailRole1024: int = Qt.ItemDataRole.UserRole + 3
    ThumbnailRoleFull: int = Qt.ItemDataRole.UserRole + 4

    # Resolution tier (cache directory key) -> Qt role.
    TIER_ROLES = {
        tier_key(RESOLUTION_THUMBNAIL): ThumbnailRole256,
        tier_key(RESOLUTION_PREVIEW): ThumbnailRole1024,
        tier_key(RESOLUTION_FULL): ThumbnailRoleFull,
    }

    def __init__(self, parent: QObject | None = None) -> None:
        """Initialise the model with an empty path list."""
        super().__init__(parent)
        self._paths: list[Path] = []
        # Keys: source path string, values {resolution: cache Path}
        # Resolutions: 256, 1024, None (for full)
        self._thumbnails: dict[str, dict[int | None, Path]] = {}
        # Path string -> row cache so set_thumbnails() avoids an O(paths) scan.
        self._path_index: dict[str, int] = {}

    @override
    def rowCount(self, parent: QModelIndex | QPersistentModelIndex = QPersistentModelIndex()) -> int:
        """Return the number of paths in the model."""
        return len(self._paths)

    @override
    def data(self, index: QModelIndex | QPersistentModelIndex, role: int = Qt.ItemDataRole.DisplayRole) -> object:
        """Return data for *index* according to *role*.

        Returns ``None`` for invalid indices or unsupported roles.
        """
        if not index.isValid() or not (0 <= index.row() < len(self._paths)):
            return None

        path = self._paths[index.row()]

        if role == Qt.ItemDataRole.DisplayRole:
            return path.name
        if role == ThumbnailModel.PathRole:
            return str(path)
        if role == ThumbnailModel.ThumbnailRole256:
            cache_path = self._thumbnails.get(str(path), {}).get(RESOLUTION_THUMBNAIL)
            return str(cache_path) if cache_path else ""
        if role == ThumbnailModel.ThumbnailRole1024:
            cache_path = self._thumbnails.get(str(path), {}).get(RESOLUTION_PREVIEW)
            return str(cache_path) if cache_path else ""
        if role == ThumbnailModel.ThumbnailRoleFull:
            cache_path = self._thumbnails.get(str(path), {}).get(RESOLUTION_FULL)
            return str(cache_path) if cache_path else ""

        return None

    def set_paths(self, paths: list[Path]) -> None:
        """Replace the entire path list, resetting the model.

        Does NOT prune ``_thumbnails``, all cached entries are preserved so
        that filtering/unfiltering doesn't lose thumbnail images.  Stale
        entries for paths no longer present are harmless; they'll be
        overwritten when new thumbnails are generated.
        """
        start = time.perf_counter()
        logger.debug("Called -  items: %d", len(paths))

        self.beginResetModel()
        self._paths = list(paths)
        self._path_index = {str(path): row for row, path in enumerate(self._paths)}
        self.endResetModel()
        elapsed = time.perf_counter() - start
        logger.debug("completed in %.3fs", elapsed)

    def set_thumbnail(
        self,
        source_path: str,
        cache_path: Path,
        resolution: int | None = None,
    ) -> None:
        """Update the cached thumbnail path for a specific resolution.

        Thin wrapper over :meth:`set_thumbnails` kept for callers that
        update a single entry at a time.

        Args:
            source_path: The original file path (as string).
            cache_path: The cached file path on disk.
            resolution: Pixel resolution (256, 1024) or None for full.
        """
        self.set_thumbnails([ThumbnailUpdate(source_path=source_path, cache_path=cache_path, resolution=resolution)])

    def set_thumbnails(self, updates: list[ThumbnailUpdate]) -> None:
        """Apply a batch of cached-path assignments in one pass.

        Stores every entry (including paths not currently in the model, so
        filtering/unfiltering keeps its images) and emits a single
        ``dataChanged`` per affected row with the union of the roles
        written for that row, instead of scanning the path list per call.

        Args:
            updates: Cached-path assignments to apply.
        """
        per_row: dict[int, set[int]] = {}
        for update in updates:
            normalized = str(Path(update.source_path))
            resolutions = self._thumbnails.setdefault(normalized, {})
            resolutions[update.resolution] = update.cache_path
            row = self._path_index.get(normalized)
            if row is not None:
                per_row.setdefault(row, set()).add(self._resolution_to_role(update.resolution))
            logger.debug(
                "Called - path: %s, resolution: %s, cache: %s", normalized, update.resolution, update.cache_path
            )

        for row in sorted(per_row):
            index = self.index(row)
            roles = sorted(per_row[row])
            self.dataChanged.emit(index, index, roles)

    @staticmethod
    def _resolution_to_role(resolution: int | None) -> int:
        """Map a resolution value to the corresponding Qt role.

        Unknown tiers raise a :class:`KeyError` (loud contract) rather
        than silently falling back to the full-resolution role.
        """
        return ThumbnailModel.TIER_ROLES[tier_key(resolution)]
