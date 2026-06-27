"""ThumbnailModel — QAbstractListModel providing path strings to a thumbnail grid view."""

from __future__ import annotations

from pathlib import Path
from typing import override

from PySide6.QtCore import QAbstractListModel, QModelIndex, QObject, Qt


class ThumbnailModel(QAbstractListModel):
    """A list model that holds file paths for a QListView-based thumbnail grid.

    Provides two roles:
        - DisplayRole:  file basename (``path.name``)
        - PathRole:     full path as a string

    Use :meth:`set_paths` to replace the entire path list.
    """

    PathRole = Qt.UserRole + 1
    ThumbnailRole = Qt.UserRole + 2

    def __init__(self, parent: QObject | None = None) -> None:
        """Initialise the model with an empty path list."""
        super().__init__(parent)
        self._paths: list[Path] = []
        self._thumbnails: dict[str, Path] = {}

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
        if role == ThumbnailModel.ThumbnailRole:
            cache_path = self._thumbnails.get(str(path))
            return str(cache_path) if cache_path else ""

        return None

    def set_paths(self, paths: list[Path]) -> None:
        """Replace the entire path list, resetting the model."""
        if paths is None:
            raise TypeError("set_paths() expected a list of Path objects, got None")
        self.beginResetModel()
        self._paths = list(paths)
        self._thumbnails.clear()
        self.endResetModel()

    def set_thumbnail(self, source_path: str, cache_path: Path) -> None:
        """Update the cached thumbnail path for a source file."""
        self._thumbnails[source_path] = cache_path
        for row, path in enumerate(self._paths):
            if str(path) == source_path:
                index = self.index(row)
                self.dataChanged.emit(index, index, [ThumbnailModel.ThumbnailRole])
                break
