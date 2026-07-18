"""Application paths — platform-aware directories for Tarragon data and cache."""

from pathlib import Path

import platformdirs

_custom_cache_dir: Path | None = None


def set_cache_dir(custom_path: Path | None) -> None:
    """Set a custom cache directory. Pass None to use the default platform path."""
    global _custom_cache_dir
    _custom_cache_dir = custom_path


def data_dir() -> Path:
    """Return the user-data directory for Tarragon.

    Uses platformdirs to resolve the correct location on each OS:
        - Linux:   ~/.local/share/tarragon
        - macOS:   ~/Library/Application Support/tarragon
        - Windows: %APPDATA%\\tarragon

    Raises RuntimeError if platformdirs returns None (e.g. headless systems
    without a proper HOME environment variable).
    """
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
