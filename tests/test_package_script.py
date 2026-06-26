"""Tests for the Nuitka packaging script and release documentation."""

from __future__ import annotations

import ast
import importlib.util
import os
import stat
from pathlib import Path
from unittest.mock import patch

import pytest

PROJECT_ROOT = Path(__file__).resolve().parent.parent
PACKAGE_SCRIPT = PROJECT_ROOT / "scripts" / "package_nuitka.py"
RELEASE_DOCS = PROJECT_ROOT / "docs" / "release.md"
REQUIREMENTS_TXT = PROJECT_ROOT / "requirements.txt"
REQUIREMENTS_DEV_TXT = PROJECT_ROOT / "requirements-dev.txt"
REQUIREMENTS_BUILD_TXT = PROJECT_ROOT / "requirements-build.txt"
BUILD_SH = PROJECT_ROOT / "scripts" / "build.sh"
BUILD_BAT = PROJECT_ROOT / "scripts" / "build.bat"


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture()
def package_script_path() -> Path:
    """Return the path to the Nuitka packaging script."""
    return PACKAGE_SCRIPT


@pytest.fixture()
def release_docs_path() -> Path:
    """Return the path to the release documentation."""
    return RELEASE_DOCS


@pytest.fixture()
def package_module():
    """Import the package script as a module and return it."""
    spec = importlib.util.spec_from_file_location("package_nuitka", PACKAGE_SCRIPT)
    assert spec is not None, "Could not create module spec"
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None, "Module has no loader"
    spec.loader.exec_module(module)
    return module


# ---------------------------------------------------------------------------
# Script existence and structure
# ---------------------------------------------------------------------------


def test_package_script_exists(package_script_path: Path) -> None:
    """The packaging script file must exist."""
    assert package_script_path.exists(), f"Script not found: {package_script_path}"


def test_package_script_is_executable(package_script_path: Path) -> None:
    """The packaging script must have a valid Python structure with a main guard."""
    source = package_script_path.read_text()
    tree = ast.parse(source)
    # Must have an `if __name__ == "__main__"` block
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


def test_build_function_exists(package_module) -> None:
    """The build() function must be defined in the script."""
    assert hasattr(package_module, "build"), "build() function not found in script"
    assert callable(package_module.build), "build must be callable"


# ---------------------------------------------------------------------------
# Build command construction (using mocks)
# ---------------------------------------------------------------------------


def test_build_command_includes_pyside6_plugin(package_module) -> None:
    """The Nuitka command must include the PySide6 plugin flag."""
    with patch.object(package_module, "check_dependencies"), patch("subprocess.run") as mock_run:
        package_module.build(onefile=True)

    mock_run.assert_called_once()
    cmd = mock_run.call_args[0][0]
    assert "--enable-plugin=pyside6" in cmd, "Nuitka command must include --enable-plugin=pyside6"


def test_build_command_includes_entry_point(package_module) -> None:
    """The Nuitka command must reference the main.py entry point."""
    with patch.object(package_module, "check_dependencies"), patch("subprocess.run") as mock_run:
        package_module.build(onefile=True)

    mock_run.assert_called_once()
    cmd = mock_run.call_args[0][0]
    # The entry point should be the last argument
    entry_point = cmd[-1]
    assert entry_point.endswith("main.py"), f"Last argument should be main.py entry point, got: {entry_point}"
    assert (
        "src" in entry_point and "tarragon" in entry_point
    ), f"Entry point path should contain src/tarragon, got: {entry_point}"


def test_build_command_includes_tarragon_package(package_module) -> None:
    """The Nuitka command must include the tarragon package."""
    with patch.object(package_module, "check_dependencies"), patch("subprocess.run") as mock_run:
        package_module.build(onefile=True)

    mock_run.assert_called_once()
    cmd = mock_run.call_args[0][0]
    assert (
        "--include-package=tarragon" in cmd
    ), "Nuitka command must include --include-package=tarragon to bundle the application package"


def test_build_command_includes_python_path(package_module) -> None:
    """The Nuitka build must set PYTHONPATH environment variable to the src directory."""
    with patch.object(package_module, "check_dependencies"), patch("subprocess.run") as mock_run:
        package_module.build(onefile=True)

    mock_run.assert_called_once()
    call_kwargs = mock_run.call_args[1]
    assert "env" in call_kwargs, "subprocess.run must be called with env parameter"
    env = call_kwargs["env"]
    assert "PYTHONPATH" in env, "PYTHONPATH must be set in environment"
    assert "src" in env["PYTHONPATH"], f"PYTHONPATH must include src directory, got: {env['PYTHONPATH']}"


# ---------------------------------------------------------------------------
# Release documentation
# ---------------------------------------------------------------------------


def test_release_docs_exist(release_docs_path: Path) -> None:
    """The release documentation file must exist."""
    assert release_docs_path.exists(), f"Release docs not found: {release_docs_path}"


def test_release_docs_has_build_instructions(release_docs_path: Path) -> None:
    """Release docs must contain build instructions for both platforms."""
    content = release_docs_path.read_text()
    # Must reference the build script
    assert "package_nuitka.py" in content, "Release docs must reference the packaging script"
    # Must have build instructions
    assert "## Building" in content or "## Build" in content, "Release docs must have a Building section"
    # Must mention both Linux and Windows
    assert "Linux" in content, "Release docs must mention Linux"
    assert "Windows" in content, "Release docs must mention Windows"


# ---------------------------------------------------------------------------
# Requirements files
# ---------------------------------------------------------------------------


def test_requirements_txt_exists() -> None:
    """The main requirements.txt file must exist."""
    assert REQUIREMENTS_TXT.exists(), f"requirements.txt not found: {REQUIREMENTS_TXT}"


def test_requirements_txt_has_runtime_deps() -> None:
    """requirements.txt must list the core runtime dependencies."""
    content = REQUIREMENTS_TXT.read_text()
    for dep in ("PySide6", "Pillow", "psd-tools", "platformdirs", "psutil"):
        assert dep in content, f"requirements.txt must include {dep}"


def test_requirements_dev_txt_exists() -> None:
    """The dev requirements file must exist."""
    assert REQUIREMENTS_DEV_TXT.exists(), f"requirements-dev.txt not found: {REQUIREMENTS_DEV_TXT}"


def test_requirements_dev_txt_has_dev_deps() -> None:
    """requirements-dev.txt must list development dependencies."""
    content = REQUIREMENTS_DEV_TXT.read_text()
    for dep in ("pytest", "ruff", "pre-commit"):
        assert dep in content, f"requirements-dev.txt must include {dep}"


def test_requirements_build_txt_exists() -> None:
    """The build requirements file must exist."""
    assert REQUIREMENTS_BUILD_TXT.exists(), f"requirements-build.txt not found: {REQUIREMENTS_BUILD_TXT}"


def test_requirements_build_txt_has_build_deps() -> None:
    """requirements-build.txt must list build dependencies."""
    content = REQUIREMENTS_BUILD_TXT.read_text()
    for dep in ("nuitka", "zstandard"):
        assert dep in content, f"requirements-build.txt must include {dep}"


# ---------------------------------------------------------------------------
# Build scripts
# ---------------------------------------------------------------------------


def test_build_sh_exists() -> None:
    """The Linux/macOS build script must exist."""
    assert BUILD_SH.exists(), f"build.sh not found: {BUILD_SH}"


def test_build_bat_exists() -> None:
    """The Windows build script must exist."""
    assert BUILD_BAT.exists(), f"build.bat not found: {BUILD_BAT}"


@pytest.mark.skipif(os.name == "nt", reason="Executable permission is a Unix concept")
def test_build_sh_is_executable() -> None:
    """build.sh must have the executable permission bit set."""
    mode = BUILD_SH.stat().st_mode
    assert mode & stat.S_IXUSR, "build.sh must be executable by owner"


def test_build_sh_references_venv() -> None:
    """build.sh must create and activate a virtual environment."""
    content = BUILD_SH.read_text()
    assert "venv" in content, "build.sh must reference a virtual environment"
    assert "activate" in content, "build.sh must activate the virtual environment"


def test_build_bat_references_venv() -> None:
    """build.bat must create and activate a virtual environment."""
    content = BUILD_BAT.read_text()
    assert "venv" in content, "build.bat must reference a virtual environment"
    assert "activate" in content, "build.bat must activate the virtual environment"


def test_build_sh_installs_requirements() -> None:
    """build.sh must install from requirements files."""
    content = BUILD_SH.read_text()
    assert "requirements.txt" in content, "build.sh must install from requirements.txt"
    assert "requirements-build.txt" in content, "build.sh must install from requirements-build.txt"


def test_build_bat_installs_requirements() -> None:
    """build.bat must install from requirements files."""
    content = BUILD_BAT.read_text()
    assert "requirements.txt" in content, "build.bat must install from requirements.txt"
    assert "requirements-build.txt" in content, "build.bat must install from requirements-build.txt"
