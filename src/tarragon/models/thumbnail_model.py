"""ThumbnailModel — QAbstractListModel providing path strings to a thumbnail grid view."""

from __future__ import annotations

from pathlib import Path
from typing import override

from PySide6.QtCore import QAbstractListModel, QModelIndex, QObject, Qt


class ThumbnailModel(QAbstractListModel):
    """A list model that holds file paths for a QListView-based thumbnail grid.

    Provides multiple roles:
        - DisplayRole:        file basename (``path.name``)
        - PathRole:           full path as a string
        - ThumbnailRole256:   256px cached thumbnail path
        - ThumbnailRole1024:  1024px cached preview path
        - ThumbnailRoleFull:  full-resolution cached path

    Use :meth:`set_paths` to replace the entire path list.
    Use :meth:`set_thumbnail` to register a cached path for a specific resolution.
    """

    PathRole = Qt.UserRole + 1
    ThumbnailRole256 = Qt.UserRole + 2   # 256px thumbnail
    ThumbnailRole1024 = Qt.UserRole + 3  # 1024px preview
    ThumbnailRoleFull = Qt.UserRole + 4  # Full resolution

    def __init__(self, parent: QObject | None = None) -> None:
        """Initialise the model with an empty path list."""
        super().__init__(parent)
        self._paths: list[Path] = []
        # Keys: source path string -> {resolution: cache Path}
        # Resolutions: 256, 1024, None (for full)
        self._thumbnails: dict[str, dict[int | None, Path]] = {}

    @override
    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802
        """Return the number of paths in the model."""
        return len(self._paths)

    @override
    def data(self, index: QModelIndex, role: int = Qt.DisplayRole) -> object:
        """Return data for *index* according to *role*.

        Returns ``None`` for invalid indices or unsupported roles.
        """
        if not index.isValid() or not (0 <= index.row() < len(self._paths)):
            return None

        path = self._paths[index.row()]

        if role == Qt.DisplayRole:
            return path.name
        if role == ThumbnailModel.PathRole:
            return str(path)
        if role == ThumbnailModel.ThumbnailRole256:
            cache_path = self._thumbnails.get(str(path), {}).get(256)
            return str(cache_path) if cache_path else ""
        if role == ThumbnailModel.ThumbnailRole1024:
            cache_path = self._thumbnails.get(str(path), {}).get(1024)
            return str(cache_path) if cache_path else ""
        if role == ThumbnailModel.ThumbnailRoleFull:
            cache_path = self._thumbnails.get(str(path), {}).get(None)
            return str(cache_path) if cache_path else ""

        return None

    def set_paths(self, paths: list[Path]) -> None:
        """Replace the entire path list, resetting the model.

        Does NOT prune ``_thumbnails`` — all cached entries are preserved so
        that filtering/unfiltering doesn't lose thumbnail images.  Stale
        entries for paths no longer present are harmless; they'll be
        overwritten when new thumbnails are generated.
        """
        if paths is None:
            raise TypeError("set_paths() expected a list of Path objects, got None")
        self.beginResetModel()
        self._paths = list(paths)
        # Don't prune _thumbnails — preserve cached entries for all paths
        # so filtering/unfiltering doesn't lose thumbnail images
        self.endResetModel()

    def set_thumbnail(
        self,
        source_path: str,
        cache_path: Path,
        resolution: int | None = None,
    ) -> None:
        """Update the cached thumbnail path for a specific resolution.

        Args:
            source_path: The original file path (as string).
            cache_path: The cached file path on disk.
            resolution: Pixel resolution (256, 1024) or None for full.
        """
        if source_path not in self._thumbnails:
            self._thumbnails[source_path] = {}
        self._thumbnails[source_path][resolution] = cache_path

        # Find row and emit dataChanged for the specific role
        for row, path in enumerate(self._paths):
            if str(path) == source_path:
                index = self.index(row)
                role = self._resolution_to_role(resolution)
                self.dataChanged.emit(index, index, [role])
                break

    @staticmethod
    def _resolution_to_role(resolution: int | None) -> int:
        """Map a resolution value to the corresponding Qt role."""
        if resolution == 256:
            return ThumbnailModel.ThumbnailRole256
        if resolution == 1024:
            return ThumbnailModel.ThumbnailRole1024
        return ThumbnailModel.ThumbnailRoleFull
