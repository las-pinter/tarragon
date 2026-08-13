"""Tests for Editors"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from unittest.mock import patch

import pytest
from tarragon.db.database import Database
from tarragon.services.editors import (
    launch_editor,
    resolve_editor_command,
    substitute_file_path,
)


@pytest.fixture
def db() -> Generator[Database, None, None]:
    """Provide an in-memory database with schema initialised."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


class TestResolveEditorCommand:
    """Editor command resolution matches extensions and wildcards."""

    def test_resolve_editor_command_finds_match(self, db: Database) -> None:
        """Extension lookup returns the configured template."""
        db.upsert_editor_association(".psd", "photoshop {file}")

        result = resolve_editor_command(db, ".psd")

        assert result == "photoshop {file}"

    def test_resolve_editor_command_returns_none_for_unknown(self, db: Database) -> None:
        """Unknown extension with no wildcard returns None."""
        result = resolve_editor_command(db, ".xyz")

        assert result is None

    def test_resolve_editor_command_wildcard_fallback(self, db: Database) -> None:
        """Falls back to the '*' wildcard if the extension is not found."""
        db.upsert_editor_association("*", "xdg-open {file}")

        result = resolve_editor_command(db, ".unknown")

        assert result == "xdg-open {file}"


class TestSubstituteFilePath:
    """File path substitution builds safe argument lists."""

    def test_substitute_file_path_handles_spaces(self) -> None:
        """Paths with spaces are passed as a single argument."""
        template = "gimp {file}"
        file_path = Path("/home/user/My Photos/image.png")

        args = substitute_file_path(template, file_path)

        assert args == ["gimp", str(Path("/home/user/My Photos/image.png"))]

    def test_substitute_file_path_handles_special_chars(self) -> None:
        """Paths with quotes and brackets are handled safely via shlex."""
        template = "editor --open {file}"
        file_path = Path("/tmp/file (copy) [v2].txt")

        args = substitute_file_path(template, file_path)

        assert args == ["editor", "--open", str(Path("/tmp/file (copy) [v2].txt"))]

    def test_substitute_file_path_raises_without_placeholder(self) -> None:
        """A template without {file} raises ValueError."""
        with pytest.raises(ValueError, match=r"\{file\}"):
            substitute_file_path("gimp --no-file", Path("/tmp/img.png"))


class TestLaunchEditor:
    """Editor launching uses non-blocking subprocess calls."""

    def test_launch_editor_non_blocking(self, db: Database) -> None:
        """The subprocess.Popen constructor is called rather than subprocess.call or subprocess.run."""
        db.upsert_editor_association(".png", "gimp {file}")

        with patch("tarragon.services.editors.subprocess.Popen") as mock_popen:
            launch_editor(db, Path("/tmp/img.png"), ".png")

        mock_popen.assert_called_once_with(["gimp", str(Path("/tmp/img.png"))], shell=False)

    def test_launch_editor_fallback_to_os_default(self, db: Database) -> None:
        """No association triggers the OS default handler."""
        with (
            patch("tarragon.services.editors.sys") as mock_sys,
            patch("tarragon.services.editors.subprocess.Popen") as mock_popen,
        ):
            mock_sys.platform = "linux"
            launch_editor(db, Path("/tmp/img.png"), ".unknown")

        mock_popen.assert_called_once_with(["xdg-open", str(Path("/tmp/img.png"))])

    def test_launch_editor_windows_uses_startfile(self, db: Database) -> None:
        """The Windows fallback uses os.startfile."""
        with (
            patch("tarragon.services.editors.sys") as mock_sys,
            patch("tarragon.services.editors.os.startfile", create=True) as mock_startfile,
        ):
            mock_sys.platform = "win32"
            launch_editor(db, Path("C:\\img.png"), ".unknown")

        mock_startfile.assert_called_once_with(str(Path("C:\\img.png")))

    def test_launch_editor_linux_uses_xdg_open(self, db: Database) -> None:
        """The Linux fallback uses xdg-open."""
        with (
            patch("tarragon.services.editors.sys") as mock_sys,
            patch("tarragon.services.editors.subprocess.Popen") as mock_popen,
        ):
            mock_sys.platform = "linux"
            launch_editor(db, Path("/home/user/photo.jpg"), ".nope")

        mock_popen.assert_called_once_with(["xdg-open", str(Path("/home/user/photo.jpg"))])
