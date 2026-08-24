"""Tests for package_nuitka.py"""

from __future__ import annotations

import ast
import importlib.util
import os
import stat
from pathlib import Path
from typing import Any
from unittest.mock import patch

import pytest

PROJECT_ROOT = Path(__file__).resolve().parent.parent
PACKAGE_SCRIPT = PROJECT_ROOT / "scripts" / "package_nuitka.py"
RELEASE_DOCS = PROJECT_ROOT / "docs" / "release.md"
BUILD_SH = PROJECT_ROOT / "scripts" / "build.sh"
BUILD_BAT = PROJECT_ROOT / "scripts" / "build.bat"


@pytest.fixture
def package_script_path() -> Path:
    """Return the path to the Nuitka packaging script."""
    return PACKAGE_SCRIPT


@pytest.fixture
def release_docs_path() -> Path:
    """Return the path to the release documentation."""
    return RELEASE_DOCS


@pytest.fixture
def package_module() -> Any:
    """Import the package script as a module and return it."""
    spec = importlib.util.spec_from_file_location("package_nuitka", PACKAGE_SCRIPT)
    assert spec is not None, "Could not create module spec"
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None, "Module has no loader"
    spec.loader.exec_module(module)
    return module


class TestPackageScriptStructure:
    """The packaging script exists with a valid structure."""

    def test_package_script_exists(self, package_script_path: Path) -> None:
        """The packaging script file exists."""
        assert package_script_path.exists(), f"Script not found: {package_script_path}"

    def test_package_script_is_executable(self, package_script_path: Path) -> None:
        """The packaging script has a main guard."""
        source = package_script_path.read_text()
        tree = ast.parse(source)
        has_main_guard = any(
            isinstance(node, ast.If)
            and isinstance(node.test, ast.Compare)
            and any(
                isinstance(comparator, ast.Constant) and comparator.value == "__main__"
                for comparator in node.test.comparators
            )
            for node in ast.walk(tree)
        )
        assert has_main_guard, "Script must have an `if __name__ == '__main__'` guard"

    def test_build_function_exists(self, package_module: Any) -> None:
        """The build() function is defined in the script."""
        assert hasattr(package_module, "build"), "build() function not found in script"
        assert callable(package_module.build), "build must be callable"


class TestBuildCommand:
    """The Nuitka build command includes the required flags."""

    def test_build_command_includes_pyside6_plugin(self, package_module: Any) -> None:
        """The Nuitka command includes the PySide6 plugin flag."""
        with patch.object(package_module, "check_dependencies"), patch("subprocess.run") as mock_run:
            package_module.build(target_platform="linux")

        mock_run.assert_called_once()
        cmd = mock_run.call_args[0][0]
        assert "--enable-plugin=pyside6" in cmd, "Nuitka command must include --enable-plugin=pyside6"

    def test_build_command_includes_force_stderr_spec(self, package_module: Any) -> None:
        """The Nuitka command redirects stderr next to the executable."""
        with patch.object(package_module, "check_dependencies"), patch("subprocess.run") as mock_run:
            package_module.build(target_platform="linux")

        mock_run.assert_called_once()
        cmd = mock_run.call_args[0][0]
        assert "--force-stderr-spec={PROGRAM_BASE}.err.txt" in cmd, (
            "Nuitka command must include --force-stderr-spec={PROGRAM_BASE}.err.txt"
        )

    def test_build_command_includes_entry_point(self, package_module: Any) -> None:
        """The Nuitka command references the main.py entry point."""
        with patch.object(package_module, "check_dependencies"), patch("subprocess.run") as mock_run:
            package_module.build(target_platform="linux")

        mock_run.assert_called_once()
        cmd = mock_run.call_args[0][0]
        # The entry point should be the last argument
        entry_point = cmd[-1]
        assert entry_point.endswith("main.py"), f"Last argument should be main.py entry point, got: {entry_point}"
        assert "src" in entry_point and "tarragon" in entry_point, (
            f"Entry point path should contain src/tarragon, got: {entry_point}"
        )

    def test_build_command_includes_tarragon_package(self, package_module: Any) -> None:
        """The Nuitka command includes the tarragon package."""
        with patch.object(package_module, "check_dependencies"), patch("subprocess.run") as mock_run:
            package_module.build(target_platform="linux")

        mock_run.assert_called_once()
        cmd = mock_run.call_args[0][0]
        assert "--include-package=tarragon" in cmd, (
            "Nuitka command must include --include-package=tarragon to bundle the application package"
        )

    def test_build_command_includes_python_path(self, package_module: Any) -> None:
        """The Nuitka build sets PYTHONPATH to the src directory."""
        with patch.object(package_module, "check_dependencies"), patch("subprocess.run") as mock_run:
            package_module.build(target_platform="linux")

        mock_run.assert_called_once()
        call_kwargs = mock_run.call_args[1]
        assert "env" in call_kwargs, "subprocess.run must be called with env parameter"
        env = call_kwargs["env"]
        assert "PYTHONPATH" in env, "PYTHONPATH must be set in environment"
        assert "src" in env["PYTHONPATH"], f"PYTHONPATH must include src directory, got: {env['PYTHONPATH']}"


class TestReleaseDocs:
    """Release documentation exists and covers both platforms."""

    def test_release_docs_exist(self, release_docs_path: Path) -> None:
        """The release documentation file exists."""
        assert release_docs_path.exists(), f"Release docs not found: {release_docs_path}"

    def test_release_docs_has_build_instructions(self, release_docs_path: Path) -> None:
        """Release docs contain build instructions for both platforms."""
        content = release_docs_path.read_text()
        assert "package_nuitka.py" in content, "Release docs must reference the packaging script"
        assert "## Building" in content or "## Build" in content, "Release docs must have a Building section"
        assert "Linux" in content, "Release docs must mention Linux"
        assert "Windows" in content, "Release docs must mention Windows"


class TestBuildScripts:
    """Linux/macOS and Windows build scripts exist."""

    def test_build_sh_exists(self) -> None:
        """The Linux/macOS build script exists."""
        assert BUILD_SH.exists(), f"build.sh not found: {BUILD_SH}"

    def test_build_bat_exists(self) -> None:
        """The Windows build script exists."""
        assert BUILD_BAT.exists(), f"build.bat not found: {BUILD_BAT}"

    @pytest.mark.skipif(os.name == "nt", reason="Executable permission is a Unix concept")
    def test_build_sh_is_executable(self) -> None:
        """The build.sh script has the executable permission bit set."""
        mode = BUILD_SH.stat().st_mode
        assert mode & stat.S_IXUSR, "build.sh must be executable by owner"


class TestBuildScriptContents:
    """Build scripts use uv to sync dependencies and run the build."""

    def test_build_sh_uses_uv_sync(self) -> None:
        """The build.sh script syncs the build extra with uv."""
        content = BUILD_SH.read_text()
        assert "uv sync --extra build" in content, "build.sh must sync the build extra with uv"

    def test_build_bat_uses_uv_sync(self) -> None:
        """The build.bat script syncs the build extra with uv."""
        content = BUILD_BAT.read_text()
        assert "uv sync --extra build" in content, "build.bat must sync the build extra with uv"

    def test_build_sh_runs_build_with_uv(self) -> None:
        """The build.sh script runs the Linux build via uv run."""
        content = BUILD_SH.read_text()
        assert "uv run python scripts/package_nuitka.py --platform linux" in content, (
            "build.sh must run the packaging script with uv for linux"
        )

    def test_build_bat_runs_build_with_uv(self) -> None:
        """The build.bat script runs the Windows build via uv run."""
        content = BUILD_BAT.read_text()
        assert "uv run python scripts\\package_nuitka.py --platform windows" in content, (
            "build.bat must run the packaging script with uv for windows"
        )

    def test_build_sh_has_no_pip_install(self) -> None:
        """The build.sh script does not install with pip."""
        content = BUILD_SH.read_text()
        assert "pip install" not in content, "build.sh must not install with pip"

    def test_build_bat_has_no_pip_install(self) -> None:
        """The build.bat script does not install with pip."""
        content = BUILD_BAT.read_text()
        assert "pip install" not in content, "build.bat must not install with pip"

    def test_build_sh_keeps_virtualbox_venv(self) -> None:
        """The build.sh script uses an external venv in VirtualBox shared folders."""
        content = BUILD_SH.read_text()
        assert "UV_PROJECT_ENVIRONMENT" in content, (
            "build.sh must set UV_PROJECT_ENVIRONMENT for VirtualBox shared folders"
        )
