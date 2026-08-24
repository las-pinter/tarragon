# Tarragon Release Process

## Overview

This document describes how to build and release Tarragon as a native binary.

## Prerequisites

- Python 3.12+
- C compiler (GCC on Linux, MSVC on Windows)
- Linux: `sudo apt-get install python3-dev patchelf`
- [uv](https://docs.astral.sh/uv/) (recommended) — fast Python package and project manager

## Building

The recommended way to build is using the build scripts, which create an isolated virtual environment:

### Linux/macOS

```bash
./scripts/build.sh
```

### Windows

```batch
scripts\build.bat
```

### Standalone Directory (alternative)

```bash
./scripts/build.sh --standalone
```

The build scripts will:
1. Create a `.venv` virtual environment (if it doesn't exist)
2. Install runtime and build dependencies from `pyproject.toml`
3. Run `scripts/package_nuitka.py`

### Manual Build (without build scripts)

If you prefer to manage the environment yourself:

```bash
# Install build dependencies with uv (recommended)
uv pip install -e ".[build]"

# Or with pip
pip install -e ".[build]"

python scripts/package_nuitka.py
```

## Smoke Testing

After building, verify the binary works:

```bash
# Linux
QT_QPA_PLATFORM=offscreen ./dist/tarragon-viewer --help

# Windows
set QT_QPA_PLATFORM=offscreen
dist\tarragon-viewer.exe --help
```

## Release Checklist

- [ ] All tests pass: `uv run pytest tests/` (or `pytest tests/`)
- [ ] Linting passes: `uv run ruff check .` (or `ruff check .`)
- [ ] Build succeeds on target platform
- [ ] Smoke test passes
- [ ] Version bumped in `pyproject.toml`
- [ ] Changelog updated
- [ ] Git tag created: `git tag v0.x.x`
- [ ] GitHub release created with binary artifacts

## Portable Packaging

Release binaries are distributed as portable zip packages (`tarragon-<version>-<os>-portable.zip`), not installers. Each zip contains the binary alongside an empty `data/` folder.

At runtime, `app_paths.data_dir()` checks for a `data` folder next to the executable (only in compiled builds — never in dev/pytest runs). If present, all app state (SQLite db, thumbnail cache) lives there instead of the OS user-data directory, so the whole app is self-contained and can be moved between machines or run from removable media. Deleting the `data` folder reverts to normal (non-portable) behavior on next launch.

An installed/non-portable variant (Inno Setup on Windows, DMG on macOS, .deb/AppImage on Linux) is a possible future addition, but is out of scope for now — see the packaging discussion for tradeoffs.

## Crashlog

On any unhandled exception, startup failure, or native crash, a `crashlog.txt` file is written to the data directory (the same folder as `tarragon.log`). The crashlog bootstrap is installed before any risky imports so it also catches import-time errors. The Nuitka build additionally passes `--force-stderr-spec={PROGRAM_BASE}.err.txt` to redirect stderr to a file next to the executable, catching even pre-Python/native stderr output.

## Code Signing (Post-MVP)

For production releases, consider code signing:
- **Windows**: Use signtool with EV certificate
- **Linux**: Consider AppImage signing or GPG signatures

## Troubleshooting

### PySide6 plugin not found

Ensure `--enable-plugin=pyside6` is in the Nuitka command.

### Missing dependencies

Add `--include-package=<name>` for any missing packages.

### Large binary size

Consider using `--standalone` mode and manually removing unused Qt plugins.

### uv issues

If `uv pip install` fails, ensure you're using a recent version of uv (`uv --version`). You can update with `uv self update`. If problems persist, fall back to `pip install -e ".[build]"`.
