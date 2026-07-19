"""Application paths — platform-aware directories for Tarragon data and cache."""

import sys
from pathlib import Path

import platformdirs

_custom_cache_dir: Path | None = None

PORTABLE_DATA_DIRNAME = "data"


def set_cache_dir(custom_path: Path | None) -> None:
    """Set a custom cache directory. Pass None to use the default platform path."""
    global _custom_cache_dir
    _custom_cache_dir = custom_path


def _is_compiled() -> bool:
    """Return True when this module is running from a Nuitka-compiled build.

    Nuitka injects a ``__compiled__`` attribute into the namespace of every
    module it compiles (not just the main module), so checking for it here
    reliably distinguishes a packaged build from a normal `python`/pytest
    run — where portable-mode detection should never kick in.
    """
    return "__compiled__" in globals()


def _portable_data_dir() -> Path | None:
    """Return the portable data directory next to the executable, if present.

    A `data` folder placed next to the executable signals portable mode:
    all app state (db, cache) lives alongside the binary instead of the OS
    user-data directory, so the whole app is self-contained on a USB stick,
    a synced folder, etc.

    Only checked in compiled builds. In Nuitka onefile mode, ``sys.argv[0]``
    is the path to the original launched executable, while ``__file__``
    would point at the temporary extraction directory the bootstrap unpacks
    to — so ``sys.argv[0]`` is the correct anchor for finding a sibling
    `data` folder.
    """
    if not _is_compiled():
        return None
    exe_dir = Path(sys.argv[0]).resolve().parent
    candidate = exe_dir / PORTABLE_DATA_DIRNAME
    if candidate.is_dir():
        return candidate
    return None


def data_dir() -> Path:
    """Return the data directory for Tarragon.

    Resolution order:
        1. Portable mode: a `data` folder next to the executable, when
           running from a compiled build.
        2. Platform-standard user-data directory otherwise, via platformdirs:
            - Linux:   ~/.local/share/tarragon
            - macOS:   ~/Library/Application Support/tarragon
            - Windows: %APPDATA%\\tarragon

    Raises RuntimeError if platformdirs returns None (e.g. headless systems
    without a proper HOME environment variable) and no portable data dir
    was found.
    """
    portable = _portable_data_dir()
    if portable is not None:
        return portable

    result = platformdirs.user_data_dir("tarragon")
    if result is None:
        raise RuntimeError(
            "platformdirs returned None for user_data_dir('tarragon'). "
            "This typically happens on headless systems without a valid "
            "HOME or XDG_DATA_HOME environment variable."
        )
    return Path(result)


def db_path() -> Path:
    """Return the path to the SQLite database file."""
    return data_dir() / "tarragon.db"


def cache_dir() -> Path:
    """Return the path to the thumbnail preview cache directory."""
    if _custom_cache_dir is not None:
        return _custom_cache_dir
    return data_dir() / "cache"


def ensure_dirs() -> None:
    """Create all required directories if they do not already exist.

    Ensures the cache directory hierarchy is present so that no
    FileNotFoundError strikes later when writing thumbnails.

    Raises OSError with context about which path failed, wrapping any
    underlying filesystem error for better debugging.
    """
    try:
        cache_dir().mkdir(parents=True, exist_ok=True)
    except OSError as exc:
        raise OSError(f"Failed to create directory {cache_dir()} (root data dir: {data_dir()}): {exc}") from exc
