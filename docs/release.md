# Tarragon Release Process

## Overview

This document describes how to build and release Tarragon as a native binary.

## Prerequisites

- Python 3.12+
- C compiler (GCC on Linux, MSVC on Windows)
- Linux: `sudo apt-get install python3-dev patchelf`

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
2. Install runtime and build dependencies from `requirements.txt` and `requirements-build.txt`
3. Run `scripts/package_nuitka.py`

### Manual Build (without build scripts)

If you prefer to manage the environment yourself:

```bash
pip install -r requirements.txt
pip install -r requirements-build.txt
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

- [ ] All tests pass: `pytest`
- [ ] Linting passes: `ruff check .`
- [ ] Build succeeds on target platform
- [ ] Smoke test passes
- [ ] Version bumped in `pyproject.toml`
- [ ] Changelog updated
- [ ] Git tag created: `git tag v0.x.x`
- [ ] GitHub release created with binary artifacts

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
