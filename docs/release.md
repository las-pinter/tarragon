# Tarragon Release Process

## Overview

This document describes how to build and release Tarragon as a native binary.

## Prerequisites

- Python 3.12+
- All dependencies installed: `pip install -e .`
- Nuitka installed: `pip install nuitka`
- C compiler (GCC on Linux, MSVC on Windows)

## Building

### Linux

```bash
python scripts/package_nuitka.py
```

Output: `dist/tarragon-viewer` (single binary)

### Windows

```bash
python scripts/package_nuitka.py
```

Output: `dist/tarragon-viewer.exe` (single executable)

### Standalone Directory (alternative)

```bash
python scripts/package_nuitka.py --standalone
```

Output: `dist/main.dist/` directory with all dependencies

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
