#!/usr/bin/env python3
"""Build Tarragon as a Nuitka onefile binary.

Usage:
    python scripts/package_nuitka.py [--standalone]

Options:
    --standalone    Build as standalone directory instead of onefile
"""

import subprocess
import sys
from pathlib import Path


def build(onefile: bool = True) -> None:
    """Run Nuitka build with correct flags for PySide6 and dependencies."""
    project_root = Path(__file__).resolve().parent.parent
    entry_point = project_root / "src" / "tarragon" / "main.py"

    cmd = [
        sys.executable,
        "-m",
        "nuitka",
        "--standalone",
        "--enable-plugin=pyside6",
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

    print(f"Running: {' '.join(cmd)}")
    subprocess.run(cmd, check=True, cwd=str(project_root))
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
