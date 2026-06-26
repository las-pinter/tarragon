#!/usr/bin/env python3
"""Build Tarragon as a Nuitka onefile binary.

Usage:
    python scripts/package_nuitka.py [--standalone]

    Or use the build scripts which set up a virtual environment automatically:
        ./scripts/build.sh          # Linux/macOS
        scripts\\build.bat          # Windows

Options:
    --standalone    Build as standalone directory instead of onefile

System dependencies:
    - Python packages: nuitka, patchelf, zstandard
    - Linux: python3-dev (for headers), patchelf (apt-get install python3-dev patchelf)
    - Windows: Visual Studio Build Tools (MSVC compiler)
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path


def check_dependencies() -> None:
    """Verify that required build dependencies are available."""
    missing: list[str] = []

    # Check Python packages
    for package in ("nuitka", "zstandard"):
        try:
            __import__(package)
        except ImportError:
            missing.append(package)

    # Check patchelf binary (required by Nuitka on Linux)
    if sys.platform.startswith("linux") and shutil.which("patchelf") is None:
        missing.append("patchelf (install via: pip install patchelf)")

    if missing:
        print(
            "ERROR: Missing build dependencies:\n"
            + "\n".join(f"  - {dep}" for dep in missing)
            + "\n\nInstall with: pip install nuitka patchelf zstandard"
            + (
                "\nOn Linux, also install: sudo apt-get install python3-dev" if sys.platform.startswith("linux") else ""
            ),
            file=sys.stderr,
        )
        sys.exit(1)


def build(onefile: bool = True) -> None:
    """Run Nuitka build with correct flags for PySide6 and dependencies."""
    check_dependencies()

    project_root = Path(__file__).resolve().parent.parent
    entry_point = project_root / "src" / "tarragon" / "main.py"

    cmd = [
        sys.executable,
        "-m",
        "nuitka",
        "--standalone",
        "--static-libpython=no",
        "--enable-plugin=pyside6",
        "--include-package=tarragon",
        "--include-package=psd_tools",
        "--include-package=PIL",
        "--include-package=platformdirs",
        "--include-package=psutil",
        f"--output-dir={project_root / 'dist'}",
    ]

    if onefile:
        cmd.append("--onefile")
        cmd.append("--output-filename=tarragon-viewer")

    cmd.append(str(entry_point))

    # Set PYTHONPATH so Nuitka can find the tarragon package in src/
    env = os.environ.copy()
    src_path = str(project_root / "src")
    env["PYTHONPATH"] = src_path + os.pathsep + env.get("PYTHONPATH", "")

    print(f"Running: {' '.join(cmd)}")
    print(f"PYTHONPATH: {env['PYTHONPATH']}")
    subprocess.run(cmd, check=True, cwd=str(project_root), env=env)
    print("Build complete! Check dist/ directory for output.")


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Build Tarragon with Nuitka")
    parser.add_argument(
        "--standalone",
        action="store_true",
        help="Build as standalone directory instead of onefile",
    )
    args = parser.parse_args()

    build(onefile=not args.standalone)
