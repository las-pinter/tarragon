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
pip install -e .
```

### 4. Verify Installation

```bash
python -c "import tarragon; print('OK')"
```

## Running the Application

```bash
python -m tarragon.main
```

This launches the Tarragon GUI. On first run, it creates the data directory and database at the platform-specific location (see [Architecture](docs/architecture.md) for paths).

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
├── README.md                   # Project overview
├── LICENSE                     # MIT license
├── .editorconfig               # Editor settings
├── .pre-commit-config.yaml     # Pre-commit hook definitions
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
│   ├── py.typed                # PEP 561 marker
│   ├── models/
│   │   ├── __init__.py
│   │   ├── thumbnail_model.py  # Data model for thumbnail grid
│   │   └── filter_state.py     # Filter state management
│   ├── services/
│   │   ├── __init__.py
│   │   ├── thumbnail_service.py  # Async thumbnail orchestration
│   │   ├── query_service.py      # SQL filter composition
│   │   ├── tag_service.py        # Tag CRUD with Qt signals
│   │   └── settings_service.py   # Typed settings validation
│   ├── widgets/
│   │   ├── __init__.py
│   │   ├── sidebar.py            # Library panel
│   │   ├── thumbnail_grid.py     # Gallery panel
│   │   ├── preview_panel.py      # Preview panel with tag management
│   │   ├── filter_bar.py         # Base filter bar widget
│   │   ├── color_filter_bar.py   # Color filter swatches
│   │   ├── tag_filter_bar.py     # Tag filter bar
│   │   ├── gallery_info_bar.py   # Gallery info display
│   │   ├── gallery_tabs.py       # Gallery tab widget
│   │   ├── log_panel.py          # Log output panel
│   │   ├── settings_dialog.py    # Settings dialog
│   │   └── flow_layout.py        # Custom flow layout
│   └── theme/
│       ├── __init__.py
│       ├── tokens.json           # Design token definitions
│       ├── tokens.py             # Token loader
│       ├── colors.py             # Color utilities
│       ├── color_buckets.py      # Color bucket definitions
│       ├── typography.py         # Typography settings
│       ├── spacing.py            # Spacing constants
│       ├── qss_generator.py      # QSS generation utilities
│       ├── file_type_badge.py    # File type badge rendering
│       ├── loader.py             # Theme loader
│       └── icons/
│           └── search.svg
├── tests/                        # Test suite
└── docs/                         # Documentation
    ├── architecture.md
    ├── database.md
    ├── rendering-pipeline.md
    ├── color-tagging.md
    └── release.md
```

## Architecture Overview

Tarragon follows a layered architecture with a UI layer (PySide6 widgets and dock panels), a service layer (business logic and Qt signal integration), a data layer (SQLite persistence and file discovery), and a rendering layer (image processing pipelines). See [Architecture](docs/architecture.md) for full details.

## Key Dependencies

| Package | Purpose |
|---------|---------|
| `PySide6>=6.6` | Qt 6 GUI framework |
| `Pillow>=10.0` | Image processing (open, resize, convert, save) |
| `psd-tools[composite]>=1.9.7` | PSD/PSB file compositing |
| `platformdirs>=4.0` | Platform-specific directory resolution |
| `psutil>=5.9` | System memory detection for adaptive worker count |

## Building a Release

See [Release](docs/release.md) for the full release process. Quick summary:

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
