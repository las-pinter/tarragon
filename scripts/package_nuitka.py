#!/usr/bin/env python3
"""Build Tarragon as a Nuitka binary.

Usage:
    python scripts/package_nuitka.py
    python scripts/package_nuitka.py --platform linux
    python scripts/package_nuitka.py --platform windows

System dependencies:
    - Python packages: nuitka, patchelf, zstandard
    - Linux: python3-dev (for headers), patchelf (apt-get install python3-dev patchelf)
    - Windows: Visual Studio Build Tools (MSVC compiler)
    - Recommended: ccache (for faster repeat builds — apt-get install ccache)
"""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path


def _find_ccache() -> str | None:
    """Find ccache binary if available.

    Nuitka auto-detects ccache via SCons when it is in PATH, but we report
    its presence so builders know whether repeat builds will be fast.
    """
    ccache_path = shutil.which("ccache")
    if ccache_path:
        print(f"ccache found: {ccache_path} — repeat builds will be fast")
        return ccache_path
    print("ccache not found — repeat builds will be slower")
    print("  Install ccache for faster builds: sudo apt-get install ccache")
    return None


def _resolve_platform(explicit: str | None) -> str:
    """Return the target platform string: 'linux' or 'windows'.

    If *explicit* is given (from --platform), validate and return it.
    Otherwise, auto-detect from the running system.
    """
    if explicit is not None:
        return explicit

    # Auto-detect from the running platform
    if sys.platform.startswith("linux"):
        return "linux"
    if sys.platform.startswith("win32"):
        return "windows"
    return sys.platform  # best-effort fallback


def check_dependencies(target_platform: str) -> None:
    """Verify that required build dependencies are available."""
    missing: list[str] = []

    # Check Python packages
    for package in ("nuitka", "zstandard"):
        try:
            __import__(package)
        except ImportError:
            missing.append(package)

    # Check patchelf binary (required by Nuitka on Linux)
    if target_platform == "linux" and shutil.which("patchelf") is None:
        missing.append("patchelf (install via: pip install patchelf)")

    if missing:
        print(
            "ERROR: Missing build dependencies:\n"
            + "\n".join(f"  - {dep}" for dep in missing)
            + "\n\nInstall with: pip install nuitka patchelf zstandard"
            + ("\nOn Linux, also install: sudo apt-get install python3-dev" if target_platform == "linux" else ""),
            file=sys.stderr,
        )
        sys.exit(1)


def build(target_platform: str) -> None:
    """Run Nuitka build with correct flags for PySide6 and dependencies."""
    check_dependencies(target_platform)
    _find_ccache()  # prints helpful message about ccache status

    project_root = Path(__file__).resolve().parent.parent
    entry_point = project_root / "src" / "tarragon" / "main.py"

    cmd = [
        sys.executable,
        "-m",
        "nuitka",
        "--standalone",
        "--windows-console-mode=disable",
        "--static-libpython=no",
        "--enable-plugin=pyside6",
        "--include-package=tarragon",
        "--include-package-data=tarragon.theme",
        "--include-package=psd_tools",
        "--include-package=PIL",
        "--include-package=platformdirs",
        "--include-package=psutil",
        "--onefile",
        f"--output-dir={project_root / 'dist'}",
        "--output-filename=tarragon",
    ]

    if target_platform == "windows":
        cmd.append("--mingw64")
    else:
        cmd.append("--gcc")

    cmd.append(str(entry_point))

    # Set PYTHONPATH so Nuitka can find the tarragon package in src/
    env = os.environ.copy()
    src_path = str(project_root / "src")
    env["PYTHONPATH"] = src_path + os.pathsep + env.get("PYTHONPATH", "")

    print(f"Running: {' '.join(cmd)}")
    print(f"PYTHONPATH: {env['PYTHONPATH']}")
    subprocess.run(cmd, check=True, cwd=str(project_root), env=env)

    output_file = project_root / "dist" / "tarragon"
    print(f"\n RELEASE BUILD COMPLETE!")
    print(f"   Output: {output_file}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Build Tarragon as a Nuitka binary.",
    )
    parser.add_argument(
        "--platform",
        choices=["linux", "windows"],
        default=None,
        help=("Target platform for the build. If omitted, the current platform is auto-detected."),
    )
    args = parser.parse_args()
    target = _resolve_platform(args.platform)
    print(f"Target platform: {target}")
    build(target_platform=target)
