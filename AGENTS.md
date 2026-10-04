# AGENTS.md

Guide for AI coding agents working in this repository.

## Commit rule (mandatory)

Every commit made by an AI agent MUST have its message heading prefixed with `ai-`.

Example: `ai- docs: create AGENTS.md for AI agents`

There are no exceptions.

## Project

Tarragon is a fast, free image browser for artists. It browses thousands of reference images, sketches, and PSD files instantly, without moving a file. Users find images by dominant color, user tags, and a favorites sidebar, and open them in Photoshop, GIMP, or Krita. It has a dark theme and is a compiled native application, not a web app.

## Environment

- Python >= 3.12
- uv for all tooling

Note: headless GUI tests need `QT_QPA_PLATFORM=offscreen`.

## Commands

### Test

CI: `uv run pytest -v --tb=short`

Pre-push hook: `uv run pytest -n auto tests/ -q`

### Typecheck

Runs inside pre-commit as the mypy hook (strict mode):

`mypy --config-file=pyproject.toml src/tarragon`

### Lint

`uv run pre-commit run --all-files`

This runs ruff, ruff-format, and the mypy hook.

### Build

`uv run python scripts/package_nuitka.py`

Optional flag: `--platform linux|windows|macos`. The platform auto-detects when omitted.
