#!/usr/bin/env python3
"""Build Tarragon as a Nuitka binary.

Usage:
    python scripts/package_nuitka.py [--dev|--release]

    Or use the build scripts which set up a virtual environment automatically:
        ./scripts/build.sh          # Linux/macOS (release build)
        ./scripts/build.sh --dev    # Linux/macOS (dev build)
        scripts\\build.bat          # Windows (release build)
        scripts\\build.bat --dev    # Windows (dev build)

Options:
    --dev       Quick dev build: skip heavy packages (psd_tools, scipy, skimage,
                IPython, matplotlib, pytest) for fast iteration.
    --release   Full release build with all packages (default).
                Output goes to dist/ as a single onefile binary.

System dependencies:
    - Python packages: nuitka, patchelf, zstandard
    - Linux: python3-dev (for headers), patchelf (apt-get install python3-dev patchelf)
    - Windows: Visual Studio Build Tools (MSVC compiler)
    - Recommended: ccache (for faster repeat builds — apt-get install ccache)
"""

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


def build(dev_mode: bool = False) -> None:
    """Run Nuitka build with correct flags for PySide6 and dependencies.

    Args:
        dev_mode: If True, skip heavy packages (psd_tools, scipy, skimage)
                  for fast iteration builds. Output goes to dist-dev/.
    """
    check_dependencies()
    ccache_path = _find_ccache()

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
        "--include-package-data=tarragon.theme",
    ]

    if dev_mode:
        # Dev mode: skip heavy packages for fast iteration
        cmd.extend([
            "--nofollow-import-to=scipy",
            "--nofollow-import-to=skimage",
            "--nofollow-import-to=psd_tools",
            "--nofollow-import-to=IPython",
            "--nofollow-import-to=matplotlib",
            "--nofollow-import-to=pytest",
            f"--output-dir={project_root / 'dist-dev'}",
        ])
    else:
        # Release mode: include everything, build single onefile binary
        cmd.extend([
            "--include-package=psd_tools",
            "--include-package=PIL",
            "--include-package=platformdirs",
            "--include-package=psutil",
            "--onefile",
            f"--output-dir={project_root / 'dist'}",
            "--output-filename=tarragon-viewer",
        ])

    cmd.append(str(entry_point))

    # Set PYTHONPATH so Nuitka can find the tarragon package in src/
    env = os.environ.copy()
    src_path = str(project_root / "src")
    env["PYTHONPATH"] = src_path + os.pathsep + env.get("PYTHONPATH", "")

    if dev_mode:
        print("\U0001F527 DEV MODE: Skipping: psd_tools, scipy, skimage, IPython, matplotlib, pytest")
        print("   PSD files won't render correctly in dev builds")

    print(f"Running: {' '.join(cmd)}")
    print(f"PYTHONPATH: {env['PYTHONPATH']}")
    subprocess.run(cmd, check=True, cwd=str(project_root), env=env)

    if dev_mode:
        output_dir = project_root / "dist-dev" / "main.dist"
        print(f"\n\u2705 DEV BUILD COMPLETE!")
        print(f"   Output: {output_dir}")
    else:
        output_file = project_root / "dist" / "tarragon-viewer"
        print(f"\n\u2705 RELEASE BUILD COMPLETE!")
        print(f"   Output: {output_file}")


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Build Tarragon with Nuitka")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--dev", action="store_true", help="Quick dev build (skip heavy packages)")
    group.add_argument("--release", action="store_true", help="Full release build (default)")
    args = parser.parse_args()

    # --release is the default; --dev overrides it. No need to read args.release.
    build(dev_mode=args.dev)
