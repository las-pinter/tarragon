"""Tests for the CLI flags of the application entry point (main.py)."""

from __future__ import annotations

import os
import subprocess
import sys
import tomllib
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent
ENTRY_POINT = PROJECT_ROOT / "src" / "tarragon" / "main.py"


def _run_cli(*args: str, timeout: int = 60) -> subprocess.CompletedProcess[str]:
    """Run the entry point in a subprocess, headless, with src on PYTHONPATH."""
    env = os.environ.copy()
    env["QT_QPA_PLATFORM"] = "offscreen"
    env["PYTHONPATH"] = str(PROJECT_ROOT / "src") + os.pathsep + env.get("PYTHONPATH", "")
    return subprocess.run(
        [sys.executable, str(ENTRY_POINT), *args],
        capture_output=True,
        text=True,
        timeout=timeout,
        env=env,
        cwd=PROJECT_ROOT,
    )


def _pyproject_version() -> str:
    """Read the canonical version from pyproject.toml."""
    with (PROJECT_ROOT / "pyproject.toml").open("rb") as fh:
        data = tomllib.load(fh)
    version = data["project"]["version"]
    assert isinstance(version, str)
    return version


class TestMainCli:
    """The --version and --help flags work headlessly without launching the GUI."""

    def test_version_flag_prints_canonical_version_and_exits_zero(self) -> None:
        """Running with --version prints the pyproject version and exits 0."""
        proc = _run_cli("--version")

        assert proc.returncode == 0
        assert proc.stdout.strip() == f"tarragon {_pyproject_version()}"

    def test_help_flag_lists_version_flag_and_exits_zero(self) -> None:
        """Running with --help prints usage including --version and exits 0."""
        proc = _run_cli("--help")

        assert proc.returncode == 0
        assert "usage:" in proc.stdout
        assert "--version" in proc.stdout

    def test_unknown_flag_exits_nonzero(self) -> None:
        """An unknown flag fails fast with a usage error instead of launching the GUI."""
        proc = _run_cli("--definitely-not-a-flag")

        assert proc.returncode != 0
        assert "usage:" in proc.stderr
