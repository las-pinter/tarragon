"""Tests for the Nuitka packaging script and release documentation."""

from __future__ import annotations

import ast
import importlib.util
from pathlib import Path
from unittest.mock import patch

import pytest

PROJECT_ROOT = Path(__file__).resolve().parent.parent
PACKAGE_SCRIPT = PROJECT_ROOT / "scripts" / "package_nuitka.py"
RELEASE_DOCS = PROJECT_ROOT / "docs" / "release.md"


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
    with patch("subprocess.run") as mock_run:
        package_module.build(onefile=True)

    mock_run.assert_called_once()
    cmd = mock_run.call_args[0][0]
    assert "--enable-plugin=pyside6" in cmd, "Nuitka command must include --enable-plugin=pyside6"


def test_build_command_includes_entry_point(package_module) -> None:
    """The Nuitka command must reference the main.py entry point."""
    with patch("subprocess.run") as mock_run:
        package_module.build(onefile=True)

    mock_run.assert_called_once()
    cmd = mock_run.call_args[0][0]
    # The entry point should be the last argument
    entry_point = cmd[-1]
    assert entry_point.endswith("main.py"), f"Last argument should be main.py entry point, got: {entry_point}"
    assert (
        "src" in entry_point and "tarragon" in entry_point
    ), f"Entry point path should contain src/tarragon, got: {entry_point}"


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
