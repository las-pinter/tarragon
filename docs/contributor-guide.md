# Tarragon Contributor Guide

## Prerequisites

- **Python 3.12+** — Tarragon requires Python 3.12 or later.
- **pip** — For package installation.
- **Git** — For version control.

## Development Setup

### 1. Clone the Repository

```bash
git clone https://github.com/las-pinter/tarragon.git
cd tarragon
```

### 2. Create a Virtual Environment

```bash
python -m venv .venv
source .venv/bin/activate        # Linux/macOS
# .venv\Scripts\activate         # Windows
```

### 3. Install in Development Mode

```bash
pip install -e ".[dev]"
```

This installs Tarragon in editable mode along with development dependencies:
- `pytest>=8.0` — Test runner
- `ruff>=0.4` — Linter and formatter
- `pre-commit>=3.6` — Git hook framework

### 4. Verify Installation

```bash
python -c "import tarragon; print('OK')"
```

## Running the Application

```bash
python -m tarragon.main
```

This launches the Tarragon GUI. On first run, it creates the data directory and database at the platform-specific location (see [architecture.md](architecture.md) for paths).

## Running Tests

```bash
pytest
```

Run a specific test file:

```bash
pytest tests/test_thumbnail.py
```

Run with verbose output:

```bash
pytest -v
```

Run with coverage (if pytest-cov is installed):

```bash
pytest --cov=tarragon
```

Test configuration is in `pyproject.toml`:

```toml
[tool.pytest.ini_options]
testpaths = ["tests"]
python_files = ["test_*.py", "*_test.py"]
python_classes = ["Test*"]
python_functions = ["test_*"]
```

## Code Style

### Linting

```bash
ruff check .
```

Auto-fix lint issues:

```bash
ruff check --fix .
```

### Formatting

```bash
ruff format .
```

Check formatting without changes:

```bash
ruff format --check .
```

### Configuration

Ruff is configured in `pyproject.toml`:

```toml
[tool.ruff]
target-version = "py312"
line-length = 120

[tool.ruff.lint]
select = ["E", "F", "I", "N", "W", "UP"]

[tool.ruff.format]
quote-style = "double"
indent-style = "space"
```

Enabled rule sets:
- `E` / `W` — pycodestyle errors and warnings
- `F` — pyflakes
- `I` — isort (import sorting)
- `N` — pep8-naming
- `UP` — pyupgrade (modernize Python syntax)

## Pre-commit Hooks

Install pre-commit hooks to automatically run checks before each commit:

```bash
pre-commit install
```

Run all hooks manually:

```bash
pre-commit run --all-files
```

## Project Structure

```
tarragon/
├── pyproject.toml              # Project metadata, dependencies, tool config
├── requirements.txt            # Runtime dependencies (pinned)
├── requirements-build.txt      # Build dependencies (Nuitka)
├── scripts/
│   ├── build.sh                # Linux/macOS build script
│   ├── build.bat               # Windows build script
│   ├── package_nuitka.py       # Nuitka packaging configuration
│   └── benchmark_rendering.py  # Rendering performance benchmarks
├── src/tarragon/
│   ├── __init__.py
│   ├── __main__.py             # python -m tarragon entry point
│   ├── main.py                 # Application entry point, MainWindow subclass
│   ├── main_window.py          # Base MainWindow with dock panels
│   ├── app_paths.py            # Platform-aware directory resolution
│   ├── db.py                   # SQLite schema and CRUD repository
│   ├── migrations.py           # Schema migration framework
│   ├── scanner.py              # Folder scanning and file discovery
│   ├── thumbnail.py            # Image rendering pipeline (plain + PSD)
│   ├── color_tagger.py         # Dominant color extraction algorithm
│   ├── settings.py             # Typed key-value settings store
│   ├── editors.py              # External editor launching
│   ├── models/
│   │   ├── thumbnail_model.py  # Data model for thumbnail grid
│   │   └── filter_state.py     # Filter state management
│   ├── services/
│   │   ├── thumbnail_service.py  # Async thumbnail orchestration
│   │   ├── query_service.py      # SQL filter composition
│   │   ├── tag_service.py        # Tag CRUD with Qt signals
│   │   └── settings_service.py   # Typed settings validation
│   ├── widgets/
│   │   ├── sidebar.py            # Library panel
│   │   ├── thumbnail_grid.py     # Gallery panel
│   │   ├── preview_panel.py      # Preview panel
│   │   ├── tag_panel.py          # Tags panel
│   │   └── color_filter_bar.py   # Color filter swatches
│   └── theme/
│       ├── tokens.json           # Design token definitions
│       ├── tokens.py             # Token loader
│       ├── app.qss               # Qt stylesheet
│       └── loader.py             # Theme loader
├── tests/                        # Test suite
└── docs/                         # Documentation
    ├── architecture.md
    ├── database.md
    ├── rendering-pipeline.md
    ├── color-tagging.md
    ├── contributor-guide.md
    └── release.md
```

## Architecture Overview

Tarragon follows a layered architecture:

1. **UI Layer** (`widgets/`, `main_window.py`) — PySide6 widgets and dock panels.
2. **Service Layer** (`services/`) — Business logic, Qt signal integration, query composition.
3. **Data Layer** (`db.py`, `migrations.py`, `scanner.py`) — SQLite persistence, schema management, file discovery.
4. **Rendering** (`thumbnail.py`, `color_tagger.py`) — Image processing pipelines.

Key design decisions:
- Single Python process compiled via Nuitka.
- SQLite with WAL mode for concurrent access.
- QThreadPool for plain image renders, ProcessPoolExecutor for PSD compositing.
- SHA-1 of absolute path for cache keys.
- Master resolution of 2048px long edge for all cached images.

See [architecture.md](architecture.md) for full details.

## Key Dependencies

| Package | Purpose |
|---------|---------|
| `PySide6>=6.6` | Qt 6 GUI framework |
| `Pillow>=10.0` | Image processing (open, resize, convert, save) |
| `psd-tools[composite]>=1.9.7` | PSD/PSB file compositing |
| `platformdirs>=4.0` | Platform-specific directory resolution |
| `psutil>=5.9` | System memory detection for adaptive worker count |

## Building a Release

See [release.md](release.md) for the full release process. Quick summary:

```bash
# Linux/macOS
./scripts/build.sh

# Windows
scripts\build.bat
```

## Before Submitting a PR

1. All tests pass: `pytest`
2. Linting passes: `ruff check .`
3. Formatting is clean: `ruff format --check .`
4. No TODO/FIXME comments in new code — implement fully or create a tracked issue.
5. Documentation updated if the change affects public behavior.
