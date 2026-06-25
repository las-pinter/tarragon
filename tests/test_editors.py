"""Tests for src/tarragon/editors.py — editor association resolution and launching."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import patch

import pytest
from tarragon.db import Database
from tarragon.editors import (
    launch_editor,
    resolve_editor_command,
    substitute_file_path,
)

# ── Fixtures ────────────────────────────────────────────────────


@pytest.fixture()
def db() -> Database:
    """Provide an in-memory database with schema initialised."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


# ── resolve_editor_command ──────────────────────────────────────


def test_resolve_editor_command_finds_match(db: Database) -> None:
    """Extension lookup returns the configured template."""
    db.upsert_editor_association(".psd", "photoshop {file}")

    result = resolve_editor_command(db, ".psd")

    assert result == "photoshop {file}"


def test_resolve_editor_command_returns_none_for_unknown(db: Database) -> None:
    """Unknown extension with no wildcard returns None."""
    result = resolve_editor_command(db, ".xyz")

    assert result is None


def test_resolve_editor_command_wildcard_fallback(db: Database) -> None:
    """Falls back to '*' wildcard if specific extension not found."""
    db.upsert_editor_association("*", "xdg-open {file}")

    result = resolve_editor_command(db, ".unknown")

    assert result == "xdg-open {file}"


# ── substitute_file_path ────────────────────────────────────────


def test_substitute_file_path_handles_spaces() -> None:
    """Path with spaces is correctly quoted in the resulting args list."""
    template = "gimp {file}"
    file_path = Path("/home/user/My Photos/image.png")

    args = substitute_file_path(template, file_path)

    assert args == ["gimp", "/home/user/My Photos/image.png"]


def test_substitute_file_path_handles_special_chars() -> None:
    """Paths with quotes and brackets are handled safely via shlex."""
    template = "editor --open {file}"
    file_path = Path("/tmp/file (copy) [v2].txt")

    args = substitute_file_path(template, file_path)

    assert args == ["editor", "--open", "/tmp/file (copy) [v2].txt"]


def test_substitute_file_path_raises_without_placeholder() -> None:
    """Template without {file} raises ValueError."""
    with pytest.raises(ValueError, match=r"\{file\}"):
        substitute_file_path("gimp --no-file", Path("/tmp/img.png"))


# ── launch_editor ───────────────────────────────────────────────


def test_launch_editor_non_blocking(db: Database) -> None:
    """subprocess.Popen is called (not subprocess.call or .run)."""
    db.upsert_editor_association(".png", "gimp {file}")

    with patch("tarragon.editors.subprocess.Popen") as mock_popen:
        launch_editor(db, Path("/tmp/img.png"), ".png")

    mock_popen.assert_called_once_with(["gimp", "/tmp/img.png"], shell=False)


def test_launch_editor_fallback_to_os_default(db: Database) -> None:
    """No association triggers OS default handler (xdg-open on non-Windows)."""
    with patch("tarragon.editors.sys") as mock_sys, patch("tarragon.editors.subprocess.Popen") as mock_popen:
        mock_sys.platform = "linux"
        launch_editor(db, Path("/tmp/img.png"), ".unknown")

    mock_popen.assert_called_once_with(["xdg-open", "/tmp/img.png"])


def test_launch_editor_windows_uses_startfile(db: Database) -> None:
    """Windows fallback uses os.startfile."""
    with (
        patch("tarragon.editors.sys") as mock_sys,
        patch("tarragon.editors.os.startfile", create=True) as mock_startfile,
    ):
        mock_sys.platform = "win32"
        launch_editor(db, Path("C:\\img.png"), ".unknown")

    mock_startfile.assert_called_once_with("C:\\img.png")


def test_launch_editor_linux_uses_xdg_open(db: Database) -> None:
    """Linux fallback uses xdg-open."""
    with patch("tarragon.editors.sys") as mock_sys, patch("tarragon.editors.subprocess.Popen") as mock_popen:
        mock_sys.platform = "linux"
        launch_editor(db, Path("/home/user/photo.jpg"), ".nope")

    mock_popen.assert_called_once_with(["xdg-open", "/home/user/photo.jpg"])
