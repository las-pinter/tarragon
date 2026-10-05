# Tarragon Release Process

## Overview

This document describes how to build and release Tarragon as a native binary.
Release artifacts are assembled automatically by GitHub Actions
(`.github/workflows/build-release.yml`); steps below are marked **LOCAL**
(optional manual checks a human can run) or **CI** (automated by the workflow
on release — do not duplicate them by hand).

## Prerequisites (LOCAL — only needed for manual builds/smoke tests; CI installs its own environment on each runner)

- Python 3.12+
- C compiler (GCC on Linux; MinGW-w64 on Windows — Nuitka downloads it automatically via `--mingw64`)
- Linux: `sudo apt-get install python3-dev patchelf`
- [uv](https://docs.astral.sh/uv/) (recommended) — fast Python package and project manager

## Building (LOCAL — optional; CI builds automatically on release)

The recommended way to build is using the build scripts, which create an isolated virtual environment:

### Linux/macOS

```bash
./scripts/build.sh
```

### Windows

```batch
scripts\build.bat
```

The build scripts will:
1. Create a `.venv` virtual environment (if it doesn't exist)
2. Install runtime and build dependencies from `pyproject.toml` (`uv sync --extra build`)
3. Run `scripts/package_nuitka.py` (forces `--platform linux` / `--platform windows`)

### Manual Build (without build scripts)

If you prefer to manage the environment yourself:

```bash
# Install runtime and build dependencies with uv (build extra)
uv sync --extra build

# Target platform is auto-detected from the current OS;
# override with --platform (choices: linux, windows, macos)
uv run python scripts/package_nuitka.py
```

### Artifact names

`scripts/package_nuitka.py` always passes `--output-filename=tarragon`, so the produced onefile binary is:

- Linux: `dist/tarragon`
- macOS: `dist/tarragon`
- Windows: `dist/tarragon.exe` (Nuitka appends `.exe` on Windows)

## Smoke Testing (LOCAL — optional; CI runs its own smoke automatically)

After building, verify the binary works headlessly. `--version` and `--help`
are handled before any Qt or database initialization, so they run fine with the
offscreen platform and no display:

```bash
# Linux/macOS
QT_QPA_PLATFORM=offscreen ./dist/tarragon --version
QT_QPA_PLATFORM=offscreen ./dist/tarragon --help

# Windows
set QT_QPA_PLATFORM=offscreen
dist\tarragon.exe --version
dist\tarragon.exe --help
```

CI runs an equivalent version smoke automatically: the `nuitka-real` job in
`package-smoke.yml` (on push to `dev` or `workflow_dispatch`) builds the real
binary and asserts `./dist/tarragon --version` prints a non-empty output
containing `tarragon`.

## Release Runbook

How a release actually goes out. Steps are marked **LOCAL** (run by a human)
or **CI** (the workflow does it automatically once the release is published —
do not repeat it by hand).

### 1. Local pre-flight (LOCAL — do before tagging)

- [ ] All tests pass: `uv run pytest tests/` (or `pytest tests/`)
- [ ] Linting passes: `uv run ruff check .` (or `ruff check .`)
- [ ] Version bumped: `version` in `pyproject.toml` **and** `__version__` in `src/tarragon/__init__.py` (kept in sync; `--version` prints this)
- [ ] Release notes drafted (paste into the GitHub release body in step 3)
- [ ] Attribution ledger appended (`last_release..HEAD`) and `assets/attribution.svg` regenerated; README `## Attribution` section in sync.

### 2. Create and push the tag (LOCAL)

- [ ] `git tag v0.x.x` — the tag name becomes `<version>` in the artifact filenames (`tarragon-v0.x.x-<os>-portable.zip`). The tag MUST be `v<version>` matching the bumped version in `pyproject.toml` (and `src/tarragon/__init__.py`) so artifact filenames align with `--version` output.
- [ ] `git push origin v0.x.x`

### 3. Publish the GitHub release (LOCAL — triggers CI)

- [ ] Create a GitHub Release for the tag and publish it (a draft does **not** trigger the workflow)

Publishing the release fires `build-release.yml` (`on: release: types: [published]`).

### 4. CI builds and uploads the artifacts (CI — fully automated)

The workflow runs a test job first (reusing `test.yml`), then a build matrix:

- `ubuntu-latest` → binary `tarragon`, asset OS `linux`
- `windows-latest` → binary `tarragon.exe`, asset OS `windows`
- `macos-latest` → binary `tarragon`, asset OS `macos`

For each OS the runner:

1. Sets up uv, Python 3.12, and the build extra (`.github/actions/setup-tarragon`), plus Qt system dependencies (`.github/actions/install-qt`).
2. Builds the binary: `uv run python scripts/package_nuitka.py` → `dist/tarragon` / `dist/tarragon.exe`.
3. Assembles the portable package under `set -e` — a missing binary aborts the job before anything is uploaded (this is the artifact-health check):
   - creates `dist/tarragon-<version>-<os>-portable/data/` (empty),
   - moves the built binary next to it.
4. Zips it as `tarragon-<version>-<os>-portable.zip` (`Compress-Archive` on the Windows runner, `zip -r` on Linux/macOS). The zip contains a top-level `tarragon-<version>-<os>-portable/` folder holding the binary and the empty `data/` directory.
5. Generates a checksum, `<zip>.sha256`: `shasum -a 256` on macOS, `sha256sum` elsewhere.
6. Uploads both files to the release via `softprops/action-gh-release@v2`.

### 5. Verify the published release (LOCAL)

- [ ] The workflow completed and the release page shows three artifact pairs: `tarragon-<version>-linux-portable.zip` (+ `.sha256`), `tarragon-<version>-macos-portable.zip` (+ `.sha256`), `tarragon-<version>-windows-portable.zip` (+ `.sha256`)
- [ ] Spot-check one artifact: download, unzip, and run the binary headlessly (`QT_QPA_PLATFORM=offscreen ./tarragon --version` on Linux/macOS)

## Portable Packaging

Release binaries are distributed as portable zip packages (`tarragon-<version>-<os>-portable.zip`), not installers. Each zip contains the binary alongside an empty `data/` folder.

At runtime, `app_paths.data_dir()` checks for a `data` folder next to the executable (only in compiled builds — never in dev/pytest runs). If present, all app state (SQLite db, thumbnail cache) lives there instead of the OS user-data directory, so the whole app is self-contained and can be moved between machines or run from removable media. Deleting the `data` folder reverts to normal (non-portable) behavior on next launch.

An installed/non-portable variant (Inno Setup on Windows, DMG on macOS, .deb/AppImage on Linux) is a possible future addition, but is out of scope for now — see the packaging discussion for tradeoffs.

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

If `uv sync` fails, ensure you're using a recent version of uv (`uv --version`). You can update with `uv self update`. If problems persist, fall back to `pip install -e ".[build]"`.
