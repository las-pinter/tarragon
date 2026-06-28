"""Tests for the application-paths module."""

from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

MOCK_DATA = "/tmp/test-tarragon"


def test_data_dir_returns_platform_path():
    """data_dir() returns the path reported by platformdirs.user_data_dir."""
    from tarragon.app_paths import data_dir

    with patch("tarragon.app_paths.platformdirs.user_data_dir", return_value=MOCK_DATA):
        result = data_dir()

    assert result == Path(MOCK_DATA)


def test_db_path_under_data_dir():
    """db_path() resolves to <data_dir>/tarragon.db."""
    from tarragon.app_paths import db_path

    with patch("tarragon.app_paths.platformdirs.user_data_dir", return_value=MOCK_DATA):
        result = db_path()

    assert result == Path(MOCK_DATA) / "tarragon.db"


def test_cache_dir_under_data_dir():
    """cache_dir() resolves to <data_dir>/cache."""
    from tarragon.app_paths import cache_dir

    with patch("tarragon.app_paths.platformdirs.user_data_dir", return_value=MOCK_DATA):
        result = cache_dir()

    assert result == Path(MOCK_DATA) / "cache"


def test_ensure_dirs_creates_directory_tree(tmp_path: Path) -> None:
    """ensure_dirs() creates the cache hierarchy on disk."""
    from tarragon.app_paths import ensure_dirs

    with patch("tarragon.app_paths.platformdirs.user_data_dir", return_value=str(tmp_path)):
        ensure_dirs()

    assert (tmp_path / "cache").is_dir()


def test_ensure_dirs_is_idempotent(tmp_path: Path) -> None:
    """Calling ensure_dirs() twice should not raise or corrupt anything."""
    from tarragon.app_paths import ensure_dirs

    with patch("tarragon.app_paths.platformdirs.user_data_dir", return_value=str(tmp_path)):
        # First call creates the directories
        ensure_dirs()
        assert (tmp_path / "cache").is_dir()

        # Second call should succeed silently — no error, no corruption
        ensure_dirs()
        assert (tmp_path / "cache").is_dir()


def test_data_dir_raises_runtimeerror_when_platformdirs_returns_none():
    """data_dir() raises RuntimeError when platformdirs returns None."""
    from tarragon.app_paths import data_dir

    with patch("tarragon.app_paths.platformdirs.user_data_dir", return_value=None):
        with pytest.raises(RuntimeError, match="platformdirs returned None"):
            data_dir()


def test_ensure_dirs_wraps_oserror_with_context(tmp_path: Path) -> None:
    """ensure_dirs() re-raises OSError with context about the failed path."""
    from tarragon.app_paths import ensure_dirs

    # Make mkdir raise an OSError (e.g. permission denied)
    mock_path = MagicMock()
    mock_path.mkdir.side_effect = PermissionError("Permission denied")
    mock_path.__truediv__ = lambda self, other: mock_path

    with (
        patch("tarragon.app_paths.platformdirs.user_data_dir", return_value=str(tmp_path)),
        patch("tarragon.app_paths.cache_dir", return_value=mock_path),
    ):
        with pytest.raises(OSError) as exc_info:
            ensure_dirs()

        # Check that the error message contains useful context
        error_msg = str(exc_info.value)
        assert "Failed to create directory" in error_msg
        assert "root data dir:" in error_msg


@pytest.mark.parametrize(
    "platform_path,expected_db,expected_cache",
    [
        (
            "/home/user/.local/share/tarragon",
            "/home/user/.local/share/tarragon/tarragon.db",
            "/home/user/.local/share/tarragon/cache",
        ),
        (
            "/tmp/custom-data",
            "/tmp/custom-data/tarragon.db",
            "/tmp/custom-data/cache",
        ),
    ],
)
def test_path_construction(platform_path: str, expected_db: str, expected_cache: str) -> None:
    """Parametrized test verifying db_path and cache_dir path construction."""
    from tarragon.app_paths import cache_dir, db_path

    with patch("tarragon.app_paths.platformdirs.user_data_dir", return_value=platform_path):
        assert db_path() == Path(expected_db)
        assert cache_dir() == Path(expected_cache)
