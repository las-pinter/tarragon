"""Folder scanner — walks a directory, filters by supported image extensions, returns file info."""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass
from pathlib import Path

logger = logging.getLogger(__name__)

SUPPORTED_EXTENSIONS = {
    ".jpg",
    ".jpeg",
    ".png",
    ".webp",
    ".tiff",
    ".tif",
    ".psd",
    ".psb",
}


@dataclass(frozen=True)
class FileInfo:
    """Metadata about a single image file discovered during a folder scan."""

    path: Path
    mtime: float
    size: int
    extension: str  # lowercase, e.g. ".psd"


def scan_folder(folder_path: Path, recursive: bool = False) -> list[FileInfo]:
    """Walk *folder_path*, filter by *SUPPORTED_EXTENSIONS*, return sorted list of *FileInfo*.

    Parameters
    ----------
    folder_path : Path
        Directory to scan (must exist on disk).
    recursive : bool
        When *True*, descend into subdirectories via ``rglob``.
        When *False* (default), only direct children are considered.

    Returns
    -------
    list[FileInfo]
        List of ``FileInfo`` objects sorted by ``path`` for deterministic ordering.
        Returns an empty list if *folder_path* does not exist.

    Notes
    -----
    - Extension matching is **case-insensitive** (``.JPG`` matches ``.jpg``).
    - Hidden files (names starting with ``.``) are **not** excluded — they are
      treated like any other file.
    """
    folder_path = Path(folder_path)
    start = time.perf_counter()
    logger.debug("scan_folder: %s (recursive=%s)", folder_path, recursive)
    try:
        if not folder_path.is_dir():
            logger.debug("scan_folder: path does not exist or is not a directory: %s", folder_path)
            return []

        iterable: list[Path]
        if recursive:
            iterable = sorted(p for p in folder_path.rglob("*") if p.is_file())
        else:
            iterable = sorted(child for child in folder_path.iterdir() if child.is_file())

        results: list[FileInfo] = []

        for path in iterable:
            ext = path.suffix.lower()
            if ext not in SUPPORTED_EXTENSIONS:
                continue
            try:
                stat = path.stat()
            except OSError:
                logger.debug("Could not stat file, skipping: %s", path, exc_info=True)
                continue
            results.append(
                FileInfo(
                    path=path,
                    mtime=stat.st_mtime,
                    size=stat.st_size,
                    extension=ext,
                )
            )

        elapsed = time.perf_counter() - start
        logger.debug("scan_folder found %d files in %.3fs", len(results), elapsed)
        return results
    except OSError as e:
        elapsed = time.perf_counter() - start
        logger.error("scan_folder failed after %.3fs: %s | error: %s", elapsed, folder_path, e)
        return []
