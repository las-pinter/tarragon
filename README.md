# Tarragon

A read-only image browser and cataloging application for local image collections.

Tarragon is positioned as a fast, non-destructive viewer that catalogs images **in place** — no copies, no sidecar files, no database bloat. It reads image metadata from disk and presents them in an organized browsing interface.

## Features (Planned)

- Browse local directories of images without modifying originals
- Support for common formats (JPEG, PNG, WebP, TIFF) via Pillow
- PSD layer inspection via psd-tools
- Lightweight SQLite catalog built on-the-fly from directory scans
- Native performance compiled via Nuitka

## Tech Stack

| Component | Technology |
|-----------|------------|
| UI Framework | PySide6 (Qt for Python) |
| Image Decoding | Pillow, psd-tools |
| Database | SQLite (via stdlib `sqlite3`) |
| Packaging | Nuitka → native binary |
| Platform Support | Linux, macOS, Windows |

## Installation

```bash
pip install -e .
```

Development dependencies:

```bash
pip install -e ".[dev]"
```

## Development

```bash
# Lint check
ruff check .

# Format code
ruff format .

# Run tests
pytest

# Pre-commit hooks
pre-commit install
```

## License

This project is licensed under the [MIT License](LICENSE).
